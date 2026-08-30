use bitflags::bitflags;

use gazpacho_datatypes::{Extent, Fps, Resolution};

pub mod color;

pub trait Operation {
    const NAME: &str;
    fn extent(&self) -> Extent;
    fn resolution(&self) -> Resolution;
    fn fps(&self) -> Option<Fps>;

    // fn render(&mut self,)
}

// TODO: We should be able to have node inputs and `Op` statically typed (as `NodeInputs`, but with cardinality/names typed)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Load,
    Contrast,
    Concat,
}

impl Op {
    pub const fn name(self) -> &'static str {
        match self {
            Op::Load => "load",
            Op::Contrast => "contrast",
            Op::Concat => "concat",
        }
    }

    pub const fn signature(self) -> Signature {
        match self {
            Op::Load => Signature::new(&["path"]),
            Op::Contrast => Signature::new(&["frame", "amount"]),
            Op::Concat => Signature::new(&["a", "b"]),
        }
    }

    /// _Additional_ dependencies the operator demands.
    pub fn deps(self) -> RequestDeps {
        match self {
            Op::Load => RequestDeps::TIME | RequestDeps::RESOLUTION,
            Op::Contrast => RequestDeps::empty(),
            Op::Concat => RequestDeps::empty(),
        }
    }

    // TODO: Give example of independent.
    /// Operator is _independent_ of these.
    pub fn indeps(self) -> RequestDeps {
        match self {
            Op::Load => RequestDeps::empty(),
            Op::Contrast => RequestDeps::empty(),
            Op::Concat => RequestDeps::empty(),
        }
    }

    pub fn iter() -> impl Iterator<Item = Op> {
        [Op::Load, Op::Contrast, Op::Concat].into_iter()
    }

    pub fn strings() -> impl Iterator<Item = &'static str> {
        Self::iter().flat_map(|op| {
            use std::iter::once;
            op.signature().names.iter().copied().chain(once(op.name()))
        })
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RequestDeps: u8 {
        const TIME = 0b00000001;
        const RESOLUTION = 0b00000010;
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
