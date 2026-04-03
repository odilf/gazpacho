mod defs;

pub use defs::*;

use crate::data::{DataValue, Port};
use ::serde::Serialize;
use std::fmt;

#[derive(Clone, Copy)]
pub struct NodeDescriptor {
    id: NodeId,
    inputs: &'static [Port],
    outputs: &'static [(Port, Effect)],
}

impl fmt::Debug for NodeDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node {{ {} }}", self.id)
    }
}

impl PartialEq for NodeDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

pub type Effect = fn(&[&DataValue]) -> DataValue;

impl NodeDescriptor {
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

pub static ALL: phf::Map<&'static str, NodeDescriptor> = phf::phf_map! {
    "video-source" => VIDEO_SOURCE,
    "contrast" => CONTRAST,
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

    use super::NodeDescriptor;
    use crate::node::ALL;
    use serde::de::Error;

    impl Serialize for NodeDescriptor {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(self.id.0)
        }
    }

    impl<'de> Deserialize<'de> for NodeDescriptor {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let id = String::deserialize(deserializer)?;
            ALL.get(&id)
                .ok_or_else(|| D::Error::custom("Uknown node id"))
                .copied()
        }
    }

    impl<'de> Deserialize<'de> for &'static NodeDescriptor {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let id = String::deserialize(deserializer)?;
            ALL.get(&id)
                .ok_or_else(|| D::Error::custom("Uknown node id"))
        }
    }

    #[cfg(test)]
    mod test {
        use color_eyre::eyre;

        use crate::{
            node::{ALL, NodeDescriptor},
        };

        #[test]
        fn round_trip() -> eyre::Result<()> {
            for node in ALL.values() {
                let serialized = ron::to_string(&node)?;
                let deserialized: NodeDescriptor = ron::from_str(&serialized)?;

                assert_eq!(deserialized, *node);
            }

            Ok(())
        }
    }
}
