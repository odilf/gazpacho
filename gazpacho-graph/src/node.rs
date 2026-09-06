use gazpacho_datatypes::{SimpleValue, StrInterner};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId(u64);

impl std::hash::Hash for NodeId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0)
    }
}

impl NodeId {
    /// Computes the identity of a node from its inputs.
    ///
    /// Deliberately avoids [`std::hash::Hash`], which makes no portability
    /// guarantees: it feeds native-endian, pointer-width-sized bytes to the
    /// hasher and may change between compiler versions. Since `NodeId`s can
    /// be persisted alongside a serialized graph, the encoding here is a
    /// hand-rolled canonical form (explicit tags, fixed little-endian
    /// widths) that is stable across platforms, toolchains, and runs.
    ///
    /// Strings are identified by their content, resolved through `strings`,
    /// not by their interner symbol, so graphs compiled in a different order
    /// still produce the same ids.
    pub fn new(inputs: &[NodeInput], strings: &StrInterner) -> Self {
        let hash = Self::content_hash(inputs, strings);
        Self(hash)
    }

    #[cfg(test)]
    fn hard_coded(id: u64) -> Self {
        Self(id)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeInput {
    Constant(SimpleValue),
    Node(NodeId),
}

impl NodeInput {
    pub fn as_node(&self) -> Option<NodeId> {
        match self {
            NodeInput::Node(node) => Some(*node),
            _ => None,
        }
    }
}

mod content_hash {
    use gazpacho_datatypes::{SimpleValue, StrInterner};

    use crate::{NodeId, NodeInput};

    /// Version of the canonical [`NodeId`] encoding.
    const NODE_ID_ENCODING_VERSION: u8 = 1;

    const NODE_ID_SEED_A: u64 = 0x040104;
    const NODE_ID_SEED_B: u64 = 0x040105;

    impl NodeId {
        pub(super) fn content_hash(inputs: &[NodeInput], strings: &StrInterner) -> u64 {
            let mut buf = Vec::new();
            buf.push(NODE_ID_ENCODING_VERSION);
            buf.extend_from_slice(&(inputs.len() as u64).to_le_bytes());
            for input in inputs {
                encode_node_input(&mut buf, *input, strings);
            }
            museair::bfast::hash128_folded(&buf, NODE_ID_SEED_A, NODE_ID_SEED_B)
        }
    }

    /// `OrderedFloat` considers every NaN bit pattern equal, so they must share
    /// one encoding.
    const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

    fn encode_node_input(out: &mut Vec<u8>, input: NodeInput, strings: &StrInterner) {
        match input {
            NodeInput::Constant(value) => {
                out.push(0);
                encode_simple_value(out, value, strings);
            }
            NodeInput::Node(node) => {
                out.push(1);
                out.extend_from_slice(&node.0.to_le_bytes());
            }
        }
    }

    fn encode_simple_value(out: &mut Vec<u8>, value: SimpleValue, strings: &StrInterner) {
        match value {
            SimpleValue::Bool(v) => {
                out.push(0);
                out.push(u8::from(bool::from(v)));
            }
            SimpleValue::Int(v) => {
                out.push(1);
                out.extend_from_slice(&i64::from(v).to_le_bytes());
            }
            SimpleValue::Float(v) => {
                out.push(2);
                out.extend_from_slice(&canonical_float_bits(f64::from(v)).to_le_bytes());
            }
            SimpleValue::Time(v) => {
                out.push(3);
                // `Rational64` is always stored in canonical (reduced,
                // positive-denominator) form.
                let secs = v.as_secs();
                out.extend_from_slice(&secs.numer().to_le_bytes());
                out.extend_from_slice(&secs.denom().to_le_bytes());
            }
            SimpleValue::Str(v) => {
                out.push(4);
                let resolved = strings.resolve(v);
                out.extend_from_slice(&(resolved.len() as u64).to_le_bytes());
                out.extend_from_slice(resolved.as_bytes());
            }
        }
    }

    /// Normalizes floats to match `OrderedFloat`'s `Eq`: `-0.0` collapses to
    /// `+0.0` and all NaN patterns collapse to a single canonical quiet NaN.
    fn canonical_float_bits(value: f64) -> u64 {
        let bits = value.to_bits();
        if bits & 0x7FFF_FFFF_FFFF_FFFF == 0 {
            0
        } else if value.is_nan() {
            CANONICAL_NAN_BITS
        } else {
            bits
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use gazpacho_datatypes::Time;

        // TODO(test): snapshot test specific hashes.

        fn constant_id(value: SimpleValue, strings: &StrInterner) -> NodeId {
            NodeId::new(&[NodeInput::Constant(value)], strings)
        }

        #[test]
        fn deterministic() {
            let mut strings = StrInterner::new();
            let path = strings.get_or_intern("./video.mp4");
            let inputs = [NodeInput::Constant(SimpleValue::Str(path))];
            assert_eq!(
                NodeId::new(&inputs, &strings),
                NodeId::new(&inputs, &strings)
            );
        }

        #[test]
        fn float_zeroes_and_nans_collapse() {
            let strings = StrInterner::new();
            let pos_zero = constant_id(SimpleValue::Float(0.0.into()), &strings);
            let neg_zero = constant_id(
                SimpleValue::Float(f64::from_bits(0x8000_0000_0000_0000).into()),
                &strings,
            );
            assert_eq!(pos_zero, neg_zero);

            let nan_a = constant_id(
                SimpleValue::Float(f64::from_bits(0x7FF8_0000_0000_0001).into()),
                &strings,
            );
            let nan_b = constant_id(
                SimpleValue::Float(f64::from_bits(0xFFF8_0000_0000_0000).into()),
                &strings,
            );
            assert_eq!(nan_a, nan_b);
            assert_ne!(pos_zero, nan_a);
        }

        #[test]
        fn time_rationals_canonicalize() {
            let strings = StrInterner::new();
            let half = constant_id(SimpleValue::Time(Time::from_secs((1i64, 2i64))), &strings);
            let two_fourths =
                constant_id(SimpleValue::Time(Time::from_secs((2i64, 4i64))), &strings);
            assert_eq!(half, two_fourths);
        }

        #[test]
        fn distinct_types_and_tags() {
            let strings = StrInterner::new();
            let boolean = constant_id(SimpleValue::Bool(true.into()), &strings);
            let int = constant_id(SimpleValue::Int(1i64.into()), &strings);
            let float = constant_id(SimpleValue::Float(1.0.into()), &strings);
            assert_ne!(boolean, int);
            assert_ne!(int, float);
            assert_ne!(boolean, float);

            let node = NodeId::new(&[NodeInput::Node(NodeId::hard_coded(1))], &strings);
            assert_ne!(node, int);
        }

        #[test]
        fn prefix_free() {
            let mut strings = StrInterner::new();
            let a = strings.get_or_intern("a");
            let ab = strings.get_or_intern("ab");
            let abc = strings.get_or_intern("abc");
            let bc = strings.get_or_intern("bc");
            let c = strings.get_or_intern("c");
            let s = |v| NodeInput::Constant(SimpleValue::Str(v));

            let whole = NodeId::new(&[s(abc)], &strings);
            let split_one = NodeId::new(&[s(ab), s(c)], &strings);
            let split_two = NodeId::new(&[s(a), s(bc)], &strings);
            assert_ne!(whole, split_one);
            assert_ne!(split_one, split_two);
            assert_ne!(whole, split_two);
        }

        #[test]
        fn order_matters() {
            let mut strings = StrInterner::new();
            let a = strings.get_or_intern("a");
            let b = strings.get_or_intern("b");
            let s = |v| NodeInput::Constant(SimpleValue::Str(v));
            assert_ne!(
                NodeId::new(&[s(a), s(b)], &strings),
                NodeId::new(&[s(b), s(a)], &strings)
            );
        }

        #[test]
        fn interner_independent() {
            // Same content, different symbol indices.
            let mut strings_a = StrInterner::new();
            strings_a.get_or_intern("dummy");
            let path_a = strings_a.get_or_intern("./video.mp4");
            let mut strings_b = StrInterner::new();
            let path_b = strings_b.get_or_intern("./video.mp4");
            assert_ne!(path_a, path_b);
            assert_eq!(
                NodeId::new(&[NodeInput::Constant(SimpleValue::Str(path_a))], &strings_a),
                NodeId::new(&[NodeInput::Constant(SimpleValue::Str(path_b))], &strings_b),
            );
        }

        #[test]
        fn golden_vector() {
            let mut strings = StrInterner::new();
            let path = strings.get_or_intern("./sample.mp4");
            let id = NodeId::new(
                &[
                    NodeInput::Constant(SimpleValue::Str(path)),
                    NodeInput::Constant(SimpleValue::Float(1.5.into())),
                    NodeInput::Node(NodeId::hard_coded(0xDEAD_BEEF)),
                ],
                &strings,
            );
            assert_eq!(id.0, 0x8CCA_4390_7E93_9554);
        }
    }
}
