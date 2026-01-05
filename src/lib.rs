use std::{iter::Map, ops, path::PathBuf};

use itertools::Itertools as _;

#[derive(Debug, Clone, Copy)]
pub struct FrameIndex(u16);

pub struct Range {
    start: FrameIndex,
    end: FrameIndex,
}

impl IntoIterator for Range {
    type Item = FrameIndex;
    type IntoIter = Map<ops::RangeInclusive<u16>, fn(u16) -> FrameIndex>;

    fn into_iter(self) -> Self::IntoIter {
        (self.start.0..=self.end.0).map(FrameIndex)
    }
}

pub struct Frame {
    width: u16,
    height: u16,
    data: Vec<u8>,
}

pub enum Node {
    Shader(ShaderNode),
    Source(SourceNode),
}

pub enum Input {
    Bool(bool),
    Int(u64),
    Float(f64),
    Frame(Frame),
    // Vec(Vec<Input>),
    // Map(Vec<(String, Input)>),
}

pub struct ShaderNode {
    name: String,
    /// Shader that contains one vertex shader with the given name, inputs and outputs.
    shader: String,
    inputs: Vec<Input>,
    outputs: Vec<Input>,
}

pub struct SourceNode {
    file: PathBuf,
}

pub struct Source {
    file: PathBuf,
}

trait Clip {
    fn frame_indices(&self) -> Range;
    fn sources(&self, frame_index: FrameIndex) -> Vec<Source>;
    fn shaders(&self, frame_index: FrameIndex) -> Vec<ShaderNode>;
}

fn render(clip: &dyn Clip) {
    for frame_index in clip.frame_indices() {
        let sources = clip.sources(frame_index);
        let shaders = clip.shaders(frame_index);
        // TODO: Pass in sources as uniforms.
        let shader = shaders.into_iter().map(|s| s.shader).join("\n\n");
        // TODO: Wire it up?
    }
}
