use std::fmt;

use crate::{
    data::{DataType, DataValue, Frame, Port, SimpleDataValue, track::SourceTrack},
    ffmpeg::get_video_metadata,
};

use ::serde::Serialize;
use color_eyre::eyre;

#[derive(Clone)]
pub enum Node {
    Regular {
        id: NodeId,
        inputs: Vec<Port>,
        outputs: Vec<(Port, Effect)>,
    },
    Const {
        value: SimpleDataValue,
    },
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Regular { id, .. } => write!(f, "Node {{ {id} }}"),
            Node::Const { value } => write!(f, "Node {{ const {value} }}"),
        }
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Node::Regular { id: id_a, .. }, Node::Regular { id: id_b, .. }) => id_a == id_b,
            (Node::Const { value: v_a }, Node::Const { value: v_b }) => v_a == v_b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Effect {
    Fn(fn(&[&DataValue]) -> eyre::Result<DataValue>),
}

// TODO: Remove
// impl<'a> Effect<'a> {
//     pub fn apply(&self, inputs: &[&DataValue]) -> eyre::Result<DataValue> {
//         match self {
//             Self::Const(v) => Ok(DataValue::Simple(v())),
//             Self::Fn(f) => f(inputs),
//         }
//     }
// }

impl Node {
    pub fn inputs(&self) -> &[Port] {
        match self {
            Self::Regular { inputs, .. } => &inputs,
            Self::Const { .. } => &[],
        }
    }

    pub fn outputs(&self) -> Option<&[(Port, Effect)]> {
        match self {
            Self::Regular { outputs, .. } => Some(&outputs),
            Self::Const { .. } => None,
        }
    }

    pub fn get_named_input_port_position(&self, name: &str) -> Option<usize> {
        self.inputs().iter().position(|port| port.name() == name)
    }

    pub fn get_named_output_port_position(&self, name: &str) -> Option<usize> {
        self.outputs()?
            .iter()
            .position(|(port, _)| port.name() == name)
    }

    pub fn effect(&self, index: usize) -> Option<&Effect> {
        self.outputs()?.get(index).map(|(_, effect)| effect)
    }

    pub fn id(&self) -> NodeId {
        match self {
            Node::Regular { id, .. } => *id,
            Node::Const { .. } => NodeId("const"),
        }
    }

    pub fn port_by_index(&self, port_index: usize) -> Option<Port> {
        match self {
            Node::Regular { outputs, .. } => Some(outputs.get(port_index)?.0),
            Node::Const { value } => {
                if port_index == 0 {
                    Some(DataType::Simple(value.typ()).named("output"))
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NodeId(&'static str);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

pub static ALL: phf::Map<&'static str, (fn() -> Node, usize)> = phf::phf_map! {
    "contrast" => (contrast_node, 0),
    "video-source" => (video_source_node, 1),
};

#[derive(Debug, Clone)]
pub struct UnknownNodeIdError(String);

impl fmt::Display for UnknownNodeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node id `{}` is not known", self.0)
    }
}

impl std::error::Error for UnknownNodeIdError {}

pub fn contrast_node() -> Node {
    Node::Regular {
        id: NodeId("contrast"),
        inputs: vec![
            DataType::float().named("amount"),
            DataType::vframe().named("frame"),
        ],
        outputs: vec![(
            DataType::vframe().named("output"),
            Effect::Fn(|inputs| {
                let mut inputs = inputs.into_iter();
                let amount: &f64 = inputs.next().copied().unwrap().try_into().unwrap();
                let frame: &Frame = inputs.next().copied().unwrap().try_into().unwrap();
                assert!(inputs.next().is_none());

                let output = contrast(*amount, frame);

                Ok(DataValue::vframe(output))
            }),
        )],
    }
}

fn contrast(amount: f64, frame: &Frame) -> Frame {
    let average = frame.average();
    frame
        .clone()
        .map(|pixel| (average + amount * (pixel as f64 - average)) as u8)
}

pub fn video_source_node() -> Node {
    Node::Regular {
        id: NodeId("video-source"),
        inputs: vec![DataType::string().named("path")],
        outputs: vec![
            (
                DataType::video_track().named("output"),
                Effect::Fn(|inputs| {
                    let path: &String = inputs
                        .into_iter()
                        .next()
                        .copied()
                        .unwrap()
                        .try_into()
                        .unwrap();

                    Ok(DataValue::Track(Box::new(SourceTrack::new(path.clone())?)))
                }),
            ),
            // TODO: All these are horribly inneficient, should be cached.
            (
                DataType::float().named("fps"),
                Effect::Fn(|inputs| {
                    let path: &String = inputs
                        .into_iter()
                        .next()
                        .copied()
                        .unwrap()
                        .try_into()
                        .unwrap();
                    let metadata = get_video_metadata(path)?;

                    Ok(DataValue::float(metadata.fps as f64))
                }),
            ),
            (
                DataType::float().named("duration"),
                Effect::Fn(|inputs| {
                    let path: &String = inputs
                        .into_iter()
                        .next()
                        .copied()
                        .unwrap()
                        .try_into()
                        .unwrap();
                    let metadata = get_video_metadata(path)?;

                    Ok(DataValue::float(metadata.duration.as_secs_f64()))
                }),
            ),
            (
                DataType::float().named("frame-count"),
                Effect::Fn(|inputs| {
                    let path: &String = inputs
                        .into_iter()
                        .next()
                        .copied()
                        .unwrap()
                        .try_into()
                        .unwrap();
                    let metadata = get_video_metadata(path)?;

                    Ok(DataValue::int(metadata.frame_count as i64))
                }),
            ),
        ],
    }
}

pub fn const_node(value: SimpleDataValue) -> Node {
    Node::Const { value }
}

// Custom serialization for nodes.
mod serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::Node;
    use crate::{
        data::SimpleDataValue,
        node::{NodeId, UnknownNodeIdError},
    };
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NodeDeserialization {
        Regular(String),
        Const(SimpleDataValue),
    }

    impl Serialize for Node {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self {
                Self::Regular { id, .. } => serializer.serialize_str(id.0),
                Self::Const { value } => value.serialize(serializer),
            }
        }
    }

    impl<'de> Deserialize<'de> for Node {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let data = NodeDeserialization::deserialize(deserializer)?;
            match data {
                NodeDeserialization::Regular(id) => super::ALL
                    .get(&id)
                    .map(|&(f, _)| f())
                    .ok_or_else(|| D::Error::custom(UnknownNodeIdError(id))),
                NodeDeserialization::Const(value) => Ok(Node::Const { value }),
            }
        }
    }

    impl<'de> Deserialize<'de> for NodeId {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let name = String::deserialize(deserializer)?;
            if &name == "const" {
                return Ok(NodeId("const"));
            }

            super::ALL
                .get(&name)
                .map(|(f, _)| f().id())
                .ok_or_else(|| D::Error::custom(UnknownNodeIdError(name)))
        }
    }

    #[cfg(test)]
    mod test {
        use color_eyre::eyre;

        use crate::{
            data::SimpleDataValue,
            node::{ALL, Node, const_node},
        };

        #[test]
        fn round_trip() -> eyre::Result<()> {
            for (node, _) in ALL.values() {
                let serialized = ron::to_string(&node())?;
                let deserialized: Node = ron::from_str(&serialized)?;

                assert_eq!(deserialized, node());
            }

            for value in [
                SimpleDataValue::int(5),
                SimpleDataValue::float(5.0),
                // SimpleDataValue::float(f64::NAN), // NaN doesn't compare properly...
                SimpleDataValue::float(0.0),
                SimpleDataValue::float(-0.0),
                SimpleDataValue::string("hello world".into()),
            ] {
                let node = const_node(value);
                let serialized = ron::to_string(&node)?;
                let deserialized: Node = ron::from_str(&serialized)?;

                assert_eq!(deserialized, node);
            }

            Ok(())
        }
    }
}
