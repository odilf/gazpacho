use std::{fmt, path::PathBuf};

use crate::{
    data::{DataType, DataValue, Frame, Port, SimpleDataValue, track::SourceTrack},
    ffmpeg::get_video_metadata,
};

use color_eyre::eyre;
use ffmpeg_sidecar::metadata;

#[derive(Clone)]
pub struct Node {
    name: &'static str,
    inputs: Vec<Port>,
    outputs: Vec<(Port, Effect)>,
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node {{ {} }}", self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Effect {
    Const(SimpleDataValue),
    Fn(fn(Vec<DataValue>) -> eyre::Result<DataValue>),
}

impl Effect {
    pub fn apply(&self, inputs: Vec<DataValue>) -> eyre::Result<DataValue> {
        match self {
            Self::Const(v) => Ok(DataValue::Simple(v.clone())),
            Self::Fn(f) => f(inputs),
        }
    }
}

impl Node {
    pub fn inputs(&self) -> &[Port] {
        &self.inputs
    }

    pub fn get_named_input_port_position(&self, name: &str) -> Option<usize> {
        self.inputs.iter().position(|port| port.name() == name)
    }

    pub fn get_named_output_port_position(&self, name: &str) -> Option<usize> {
        self.outputs
            .iter()
            .position(|(port, _)| port.name() == name)
    }

    pub fn effect(&self, index: usize) -> Option<&Effect> {
        self.outputs.get(index).map(|(_, effect)| effect)
    }

    pub fn outputs(&self) -> &[(Port, Effect)] {
        &self.outputs
    }
}

pub fn contrast_node() -> Node {
    Node {
        name: "contrast",
        inputs: vec![
            DataType::float().named("amount"),
            DataType::vframe().named("frame"),
        ],
        outputs: vec![(
            DataType::vframe().named("output"),
            Effect::Fn(|inputs| {
                let mut inputs = inputs.into_iter();
                let amount: f64 = inputs.next().unwrap().try_into().unwrap();
                let frame: Frame = inputs.next().unwrap().try_into().unwrap();
                assert!(inputs.next().is_none());

                let output = contrast(amount, frame);

                Ok(DataValue::vframe(output))
            }),
        )],
    }
}

fn contrast(amount: f64, frame: Frame) -> Frame {
    let average = frame.average();
    frame.map(|pixel| (average + amount * (pixel as f64 - average)) as u8)
}

pub fn video_source_node() -> Node {
    Node {
        name: "video-source",
        inputs: vec![DataType::path().named("path")],
        outputs: vec![
            (
                DataType::video_track().named("output"),
                Effect::Fn(|inputs| {
                    let path: PathBuf = inputs.into_iter().next().unwrap().try_into().unwrap();

                    Ok(DataValue::Track(Box::new(SourceTrack::new(
                        path.to_str().unwrap().to_owned(),
                    )?)))
                }),
            ),
            // TODO: All these are horribly inneficient, should be cached.
            (
                DataType::float().named("fps"),
                Effect::Fn(|inputs| {
                    let path: PathBuf = inputs.into_iter().next().unwrap().try_into().unwrap();
                    let metadata = get_video_metadata(path.to_str().unwrap())?;

                    Ok(DataValue::float(metadata.fps as f64))
                }),
            ),
            (
                DataType::float().named("duration"),
                Effect::Fn(|inputs| {
                    let path: PathBuf = inputs.into_iter().next().unwrap().try_into().unwrap();
                    let metadata = get_video_metadata(path.to_str().unwrap())?;

                    Ok(DataValue::float(metadata.duration.as_secs_f64()))
                }),
            ),
            (
                DataType::float().named("frame-count"),
                Effect::Fn(|inputs| {
                    let path: PathBuf = inputs.into_iter().next().unwrap().try_into().unwrap();
                    let metadata = get_video_metadata(path.to_str().unwrap())?;

                    Ok(DataValue::int(metadata.frame_count as i64))
                }),
            ),
        ],
    }
}

pub fn const_node(data: SimpleDataValue) -> Node {
    Node {
        name: "const",
        inputs: vec![],
        outputs: vec![(
            DataType::Simple(data.typ()).named("output"),
            Effect::Const(data),
        )],
    }
}
