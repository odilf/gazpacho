use std::{fmt, path::PathBuf};

use crate::{
    data::{DataType, DataValue, Frame, Port, SimpleDataType, SimpleDataValue},
    ffmpeg::get_frame_count,
};

use color_eyre::eyre;
use ffmpeg_sidecar::command::FfmpegCommand;

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

    pub fn get_named_bind_position(&self, name: &str) -> Option<usize> {
        self.inputs.iter().position(|bind| bind.name() == name)
    }

    pub fn effect(&self, index: usize) -> &Effect {
        &self.outputs[index].1
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
        outputs: vec![(
            DataType::video_track().named("output"),
            Effect::Fn(|inputs| {
                let path: PathBuf = inputs.into_iter().next().unwrap().try_into().unwrap();

                let metadata = get_frame_count(&path.to_string_lossy())?;

                Ok(DataValue::Track {
                    length: metadata.frame_count,
                    renderer: Box::new(move |index| {
                        let iter = FfmpegCommand::new()
                            .input(path.to_str().unwrap())
                            .rawvideo()
                            .spawn()
                            .unwrap()
                            .iter()
                            .unwrap();

                        let frame = iter.filter_frames().skip(index as usize).next().unwrap();
                        Frame::from(frame).into()
                    }),
                    typ: SimpleDataType::VideoFrame,
                })
            }),
        )],
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
