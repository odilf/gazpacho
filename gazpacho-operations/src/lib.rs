pub mod color;

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
