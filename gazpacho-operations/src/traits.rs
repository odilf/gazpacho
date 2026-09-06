use eyre::OptionExt as _;
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, Str, StrInterner};

use crate::{NodeInput, Request, RequestDeps, Signature, Value};

pub trait OperationDerived {
    const NAME: &str;
    const SIGNATURE: Signature;
    const DEPS: RequestDeps = RequestDeps::empty();
    const INDEPS: RequestDeps = RequestDeps::empty();

    fn inputs(&self) -> &[NodeInput];
    fn inputs_mut(&mut self) -> &mut [NodeInput];
    fn constructor(inputs: Vec<Option<NodeInput>>) -> eyre::Result<Self>
    where
        Self: Sized;

    fn main_input(&self) -> NodeInput {
        #[expect(
            clippy::indexing_slicing,
            reason = "Nodes need at least one input, otherwise they would be
            constants (this needs to be upheld in `Op`)."
        )]
        self.inputs()[0]
    }
}

pub trait Operation {
    fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame>;

    fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent>
    where
        Self: OperationDerived,
    {
        renderer.extent(self.main_input())
    }

    fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution>
    where
        Self: OperationDerived,
    {
        renderer.resolution(self.main_input())
    }

    fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>>
    where
        Self: OperationDerived,
    {
        renderer.fps(self.main_input())
    }

    fn resolve(
        strings: &StrInterner,
        args: impl Iterator<Item = (Option<Str>, eyre::Result<NodeInput>)>,
    ) -> eyre::Result<Self>
    where
        Self: Sized + OperationDerived,
    {
        let mut inputs = vec![None; Self::SIGNATURE.len()];
        let mut first_available = 0;
        for (name, val) in args {
            if let Some(name) = name {
                #[expect(
                    clippy::unwrap_used,
                    reason = "name obtained from module, so it has been added."
                )]
                let i = Self::SIGNATURE
                    .index_of(strings.resolve(name))
                    .ok_or_eyre("Name not in arg list.")?;
                if i == first_available {
                    first_available += 1;
                }

                inputs[i] = Some(val?);
            } else {
                inputs[first_available] = Some(val?);
                first_available += 1;
            }
        }

        Self::constructor(inputs)
    }
}

pub trait Renderer {
    fn extent(&mut self, node: NodeInput) -> eyre::Result<Extent>;
    fn resolution(&mut self, node: NodeInput) -> eyre::Result<Resolution>;
    fn fps(&mut self, node: NodeInput) -> eyre::Result<Option<Fps>>;
    fn eval(&mut self, node: NodeInput, req: Request) -> eyre::Result<Value>;

    fn load_frame(&mut self, path: Str, req: Request) -> eyre::Result<Frame>;
    fn load_extent(&mut self, path: Str) -> eyre::Result<Extent>;
    fn load_resolution(&mut self, path: Str) -> eyre::Result<Resolution>;
    fn load_fps(&mut self, path: Str) -> eyre::Result<Fps>;
}
