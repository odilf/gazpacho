use crate::color::Contrast;

pub mod color;

mod traits;
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution};
pub use traits::*;

// TODO: We should be able to have node inputs and `Op` statically typed (as `NodeInputs`, but with cardinality/names typed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    // Load,
    Contrast(Contrast),
    // Concat,
}

impl Op {
    pub fn inputs(&self) -> &[NodeInput] {
        match self {
            Op::Contrast(contrast) => contrast.inputs(),
        }
    }

    pub fn inputs_mut(&mut self) -> &mut [NodeInput] {
        match self {
            Op::Contrast(contrast) => contrast.inputs_mut(),
        }
    }

    pub fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame> {
        match self {
            Op::Contrast(contrast) => contrast.frame(renderer, req),
        }
    }

    pub fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent> {
        match self {
            Op::Contrast(contrast) => contrast.extent(renderer),
        }
    }

    pub fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution> {
        match self {
            Op::Contrast(contrast) => contrast.resolution(renderer),
        }
    }

    pub fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>> {
        match self {
            Op::Contrast(contrast) => contrast.fps(renderer),
        }
    }

    pub fn deps(&self) -> RequestDeps {
        match self {
            Op::Contrast(_) => Contrast::DEPS,
        }
    }

    pub fn indeps(&self) -> RequestDeps {
        match self {
            Op::Contrast(_) => Contrast::INDEPS,
        }
    }
}

pub struct Signature {
    names: &'static [&'static str],
}

impl Signature {
    pub const fn new(names: &'static [&'static str]) -> Self {
        Self { names }
    }

    pub const fn len(&self) -> usize {
        self.names.len()
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names
            .iter()
            .enumerate()
            .find_map(|(i, &argname)| (argname == name).then_some(i))
    }
}
