use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, Str, StrInterner};

pub mod basic;
pub mod color;

mod traits;
pub use traits::*;

use crate::{
    basic::{Concat, Load},
    color::Contrast,
};

macro_rules! decl_op {
    ($($op:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Op {
            $($op($op)),*
        }

        impl Op {
            pub fn inputs(&self) -> &[NodeInput] {
                match self { $(Self::$op(op) => op.inputs()),* }
            }
            pub fn inputs_mut(&mut self) -> &mut [NodeInput] {
                match self { $(Self::$op(op) => op.inputs_mut()),* }
            }
            pub fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame> {
                match self { $(Self::$op(op) => op.frame(renderer, req)),* }
            }
            pub fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent> {
                match self { $(Self::$op(op) => op.extent(renderer)),* }
            }
            pub fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution> {
                match self { $(Self::$op(op) => op.resolution(renderer)),* }
            }
            pub fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>> {
                match self { $(Self::$op(op) => op.fps(renderer)),* }
            }
            pub fn deps(&self) -> RequestDeps {
                match self { $(Self::$op(_) => $op::DEPS),* }
            }
            pub fn indeps(&self) -> RequestDeps {
                match self { $(Self::$op(_) => $op::INDEPS),* }
            }
            pub fn try_load(
                name: Str,
                strings: &StrInterner,
                args: impl Iterator<Item = (Option<Str>, eyre::Result<NodeInput>)>,
            ) -> eyre::Result<Option<Self>> {
                let op = match strings.resolve(name).unwrap() {
                    $($op::NAME => Op::$op($op::resolve(strings, args)?),)*
                    _ => return Ok(None),
                };

                Ok(Some(op))
            }
        }
    };
}

decl_op! {
    Load,
    Concat,
    Contrast,
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

#[macro_export]
macro_rules! op {
    (
        pub struct $Name:ident{ $($field:tt),* $(,)? } as $lower:expr
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $Name([$crate::NodeInput; $crate::op!(@length $($field)*)]);

        impl $Name {
            $crate::op!(@accessor { 0 } $($field)*);
        }

        impl $crate::OperationMacro for $Name {
            const NAME: &str = $lower;
            const SIGNATURE: $crate::Signature = $crate::Signature::new(&[$(stringify!($field)),*]);

            fn inputs(&self) -> &[$crate::NodeInput] {
                &self.0
            }

            fn inputs_mut(&mut self) -> &mut [$crate::NodeInput] {
                &mut self.0
            }

            fn constructor(inputs: Vec<Option<$crate::NodeInput>>) -> eyre::Result<Self>
            where
                Self: Sized,
            {
                if inputs.len() != Self::SIGNATURE.len() {
                    eyre::bail!(
                        "Expected {} inputs, got {}",
                        Self::SIGNATURE.len(),
                        inputs.len()
                    )
                }

                let fixed: [_; Self::SIGNATURE.len()] =
                    inputs.try_into().map_err(|e: Vec<_>| {
                        eyre::eyre!(
                            "Expected {} inputs, found {}",
                            Self::SIGNATURE.len(),
                            e.len()
                        )
                    })?;

                Ok(Self(fixed.map(|v| v.unwrap())))
            }
        }
    };

    (@length ) => {
        0
    };
    (@length $field:tt $($fields:tt)*) => {
        1 + $crate::op!(@length $($fields)*)
    };

    (@accessor { $acc:expr }) => {};
    (@accessor { $acc:expr } $field:tt $($fields:tt)*) => {
        pub fn $field(self) -> $crate::NodeInput {
            self.0[$acc]
        }

        $crate::op!(@accessor { $acc + 1 } $($fields)*);
    }
}
