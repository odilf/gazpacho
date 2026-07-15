mod defs;

use color_eyre::eyre;
pub use defs::*;

use crate::data::{DataValue, Port};
use ::serde::Serialize;
use std::{any::Any, fmt};

/// The evaluation context, threaded upstream by [`Inputs::eval`].
///
/// This is how time-remapping nodes (concat, cut, speed, loop, …) work: they
/// rewrite the [`Ctx`] before pulling from their inputs. Most nodes just
/// forward it unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Ctx {
    pub frame_index: u64,
}

/// Lazy handle to a node's inputs.
///
/// An [`Effect`] calls [`Inputs::eval`] to pull each input on demand, at a
/// [`Ctx`] of its choosing. This is the mechanism that lets concat dispatch to
/// one branch or another, cut shift the index, etc.
pub struct Inputs<'a> {
    resolve: &'a mut dyn FnMut(usize, Ctx) -> eyre::Result<DataValue>,
}

impl<'a> Inputs<'a> {
    pub(crate) fn new(
        resolve: &'a mut dyn FnMut(usize, Ctx) -> eyre::Result<DataValue>,
    ) -> Self {
        Self { resolve }
    }

    /// Evaluate the `index`-th input at the given [`Ctx`].
    pub fn eval(&mut self, index: usize, ctx: Ctx) -> eyre::Result<DataValue> {
        (self.resolve)(index, ctx)
    }
}

pub type Effect = fn(Inputs<'_>, Ctx, &mut dyn Any) -> eyre::Result<DataValue>;

#[derive(Clone, Copy)]
pub struct NodeSpec {
    /// Globally unique identifier for the node.
    id: NodeId,
    /// Input ports, evaluated lazily by the effect via [`Inputs::eval`].
    inputs: &'static [Port],
    /// Output ports and their corresponding [`Effect`].
    outputs: &'static [(Port, Effect)],
    init_data: fn() -> Box<dyn Any>,
}

impl fmt::Debug for NodeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node {{ {} }}", self.id)
    }
}

impl PartialEq for NodeSpec {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for NodeSpec {}

impl NodeSpec {
    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn inputs(&self) -> &'static [Port] {
        self.inputs
    }

    pub fn outputs(&self) -> &'static [(Port, Effect)] {
        self.outputs
    }

    pub fn init_data(&self) -> Box<dyn Any> {
        (self.init_data)()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NodeId(&'static str);

impl NodeId {
    pub fn as_str(&self) -> &str {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

// TODO: Write using codegen
pub static ALL: phf::Map<&'static str, NodeSpec> = phf::phf_map! {
    // TODO: Handle names properly in macro
    "video-source" => basic::VIDEO_SOURCE,
    "const-int" => basic::INT,
    "const-float" => basic::FLOAT,
    "const-vframe" => basic::VFRAME,
    "const-string" => basic::STRING,
    "const-any" => basic::ANY,

    "concat" => basic::CONCAT,

    "contrast" => color::CONTRAST,
};

#[derive(Debug, Clone)]
pub struct UnknownNodeIdError(String);

impl fmt::Display for UnknownNodeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node id `{}` is not known", self.0)
    }
}

impl std::error::Error for UnknownNodeIdError {}

// Custom serialization for nodes.
mod serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::NodeSpec;
    use crate::node::{ALL, UnknownNodeIdError};
    use serde::de::Error;

    impl Serialize for NodeSpec {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(self.id.0)
        }
    }

    impl<'de> Deserialize<'de> for NodeSpec {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let id = String::deserialize(deserializer)?;
            ALL.get(&id)
                .ok_or_else(|| D::Error::custom(UnknownNodeIdError(id)))
                .copied()
        }
    }

    impl<'de> Deserialize<'de> for &'static NodeSpec {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let id = String::deserialize(deserializer)?;
            ALL.get(&id)
                .ok_or_else(|| D::Error::custom(UnknownNodeIdError(id)))
        }
    }

    #[cfg(test)]
    mod test {
        use color_eyre::eyre;

        use crate::node::{ALL, NodeSpec};

        #[test]
        fn round_trip() -> eyre::Result<()> {
            for node in ALL.values() {
                let serialized = ron::to_string(&node)?;
                let deserialized: NodeSpec = ron::from_str(&serialized)?;

                assert_eq!(deserialized, *node);
            }

            Ok(())
        }
    }
}
