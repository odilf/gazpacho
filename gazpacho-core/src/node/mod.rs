mod defs;

pub use defs::*;

use crate::data::{DataValue, Port, SimpleDataValue};
use ::serde::Serialize;
use std::fmt;

#[derive(Clone, Copy)]
pub struct NodeSpec {
    id: NodeId,
    inputs: &'static [Port],
    outputs: &'static [(Port, Effect)],
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

pub type Effect = fn(&[&DataValue], Option<&SimpleDataValue>) -> DataValue;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NodeId(&'static str);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

pub static ALL: phf::Map<&'static str, NodeSpec> = phf::phf_map! {
    // TODO: Handle names properly in macro
    "video_source" => VIDEO_SOURCE,
    "contrast" => CONTRAST,
    "const-int" => INT,
    "const-float" => FLOAT,
    "const-vframe" => VFRAME,
    "const-string" => STRING,
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
    use crate::node::ALL;
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
                .ok_or_else(|| D::Error::custom("Uknown node id"))
                .copied()
        }
    }

    impl<'de> Deserialize<'de> for &'static NodeSpec {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let id = String::deserialize(deserializer)?;
            ALL.get(&id)
                .ok_or_else(|| D::Error::custom("Uknown node id"))
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
