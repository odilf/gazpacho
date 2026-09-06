use eyre::OptionExt as _;
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, SimpleValue, Str, StrInterner, Time};

use crate::Signature;
use bitflags::bitflags;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    pub time: Time,
    pub resolution: Resolution,
}

impl Request {
    /// A value to pass in when you don't have a request available, for nodes
    /// that you don't expect should depend on the request, i.e., constants.
    pub const fn sentinel() -> Self {
        Self {
            resolution: Resolution {
                width: 0,
                height: 0,
            },
            time: Time::ZERO,
        }
    }

    pub fn select(self, deps: RequestDeps) -> PartialRequest {
        let mut partial = PartialRequest {
            resolution: self.resolution,
            time: self.time,
        };

        if !deps.contains(RequestDeps::TIME) {
            partial.time = Time::ZERO;
        }
        if !deps.contains(RequestDeps::RESOLUTION) {
            partial.resolution = Resolution {
                width: 0,
                height: 0,
            }
        }

        partial
    }
}

/// A [`Request`] where only some of the fields matter. Obtained from [`Request::select`]
///
/// There is an implementation detail leak in the fact that the partial request
/// "forgets" which values it has ignored, so some requests are considered
/// "equal" even though semantically they seem like they shouldn't.
///
/// However, regular equality semantics _are_ guaranteed for partial requests
/// originating from the same [`RequestDeps`].
///
/// Note that this is almost trivial to fix by adding [`RequestDeps`] to the body, but I
/// just think it's unecessary.
// TODO: We could add it only on debug assertions? And then verify that we never compare two different partial requests?
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartialRequest {
    resolution: Resolution,
    time: Time,
}

bitflags! {
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RequestDeps: u8 {
        const TIME = 0b00000001;
        const RESOLUTION = 0b00000010;
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId(u64);

impl std::hash::Hash for NodeId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0)
    }
}

/// Version of the canonical [`NodeId`] encoding.
const NODE_ID_ENCODING_VERSION: u8 = 1;

const NODE_ID_SEED_A: u64 = 0x040104;
const NODE_ID_SEED_B: u64 = 0x040105;

/// `OrderedFloat` considers every NaN bit pattern equal, so they must share
/// one encoding.
const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

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
        let mut buf = Vec::new();
        buf.push(NODE_ID_ENCODING_VERSION);
        buf.extend_from_slice(&(inputs.len() as u64).to_le_bytes());
        for input in inputs {
            encode_node_input(&mut buf, *input, strings);
        }
        Self(museair::bfast::hash128_folded(
            &buf,
            NODE_ID_SEED_A,
            NODE_ID_SEED_B,
        ))
    }
}

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

pub trait OperationDerived {
    const NAME: &str;
    const SIGNATURE: Signature;
    const DEPS: RequestDeps = RequestDeps::empty();
    const INDEPS: RequestDeps = RequestDeps::empty();

    fn inputs(&self) -> &[NodeInput];
    fn inputs_mut(&mut self) -> &mut [NodeInput];
    fn constructor(inputs: Vec<Option<NodeInput>>) -> eyre::Result<Self>
    where
        Self: Sized;

    fn main_input(&self) -> NodeInput {
        #[expect(
            clippy::indexing_slicing,
            reason = "Nodes need at least one input, otherwise they would be
            constants (this needs to be upheld in `Op`)."
        )]
        self.inputs()[0]
    }
}

pub trait Operation {
    fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame>;

    fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent>
    where
        Self: OperationDerived,
    {
        renderer.extent(self.main_input())
    }

    fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution>
    where
        Self: OperationDerived,
    {
        renderer.resolution(self.main_input())
    }

    fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>>
    where
        Self: OperationDerived,
    {
        renderer.fps(self.main_input())
    }

    fn resolve(
        strings: &StrInterner,
        args: impl Iterator<Item = (Option<Str>, eyre::Result<NodeInput>)>,
    ) -> eyre::Result<Self>
    where
        Self: Sized + OperationDerived,
    {
        let mut inputs = vec![None; Self::SIGNATURE.len()];
        let mut first_available = 0;
        for (name, val) in args {
            if let Some(name) = name {
                #[expect(
                    clippy::unwrap_used,
                    reason = "name obtained from module, so it has been added."
                )]
                let i = Self::SIGNATURE
                    .index_of(strings.resolve(name))
                    .ok_or_eyre("Name not in arg list.")?;
                if i == first_available {
                    first_available += 1;
                }

                inputs[i] = Some(val?);
            } else {
                inputs[first_available] = Some(val?);
                first_available += 1;
            }
        }

        Self::constructor(inputs)
    }
}

pub trait Renderer {
    fn extent(&mut self, node: NodeInput) -> eyre::Result<Extent>;
    fn resolution(&mut self, node: NodeInput) -> eyre::Result<Resolution>;
    fn fps(&mut self, node: NodeInput) -> eyre::Result<Option<Fps>>;
    fn eval(&mut self, node: NodeInput, req: Request) -> eyre::Result<Value>;

    fn load_frame(&mut self, path: Str, req: Request) -> eyre::Result<Frame>;
    fn load_extent(&mut self, path: Str) -> eyre::Result<Extent>;
    fn load_resolution(&mut self, path: Str) -> eyre::Result<Resolution>;
    fn load_fps(&mut self, path: Str) -> eyre::Result<Fps>;
}

pub enum Value {
    Simple(SimpleValue),
    Frame(Frame),
    Extent(Extent),
    Fps(Fps),
    Resolution(Resolution),
}

impl Value {
    pub fn to_frame(self) -> eyre::Result<Frame> {
        match self {
            Self::Frame(frame) => Ok(frame),
            _ => eyre::bail!("not a frame"),
        }
    }

    pub fn to_float(self) -> eyre::Result<f64> {
        match self {
            Self::Simple(SimpleValue::Float(v)) => Ok(v.into()),
            _ => eyre::bail!("not a float"),
        }
    }

    pub fn to_str(self) -> eyre::Result<Str> {
        match self {
            Self::Simple(SimpleValue::Str(v)) => Ok(v),
            _ => eyre::bail!("not a string"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let two_fourths = constant_id(SimpleValue::Time(Time::from_secs((2i64, 4i64))), &strings);
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

        let node = NodeId::new(&[NodeInput::Node(NodeId(1))], &strings);
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
                NodeInput::Node(NodeId(0xDEAD_BEEF)),
            ],
            &strings,
        );
        assert_eq!(id.0, 0x8CCA_4390_7E93_9554);
    }
}
