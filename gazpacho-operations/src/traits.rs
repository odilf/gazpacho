use std::hash::{Hash, Hasher};

use eyre::OptionExt as _;
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, SimpleValue, Str, StrInterner, Time};
use rapidhash::fast::RapidHasher;

use crate::Signature;
use bitflags::bitflags;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    pub time: Time,
    pub resolution: Resolution,
}

impl Request {
    /// A value to pass in when you don't have a request available, for nodes
    /// that you don't expect should depend on the request, i.e., constants.
    pub const fn sentinel() -> Self {
        Self {
            resolution: Resolution {
                width: 0,
                height: 0,
            },
            time: Time::ZERO,
        }
    }

    pub fn select(self, deps: RequestDeps) -> PartialRequest {
        let mut partial = PartialRequest {
            resolution: self.resolution,
            time: self.time,
        };

        if !deps.contains(RequestDeps::TIME) {
            partial.time = Time::ZERO;
        }
        if !deps.contains(RequestDeps::RESOLUTION) {
            partial.resolution = Resolution {
                width: 0,
                height: 0,
            }
        }

        partial
    }
}

/// A [`Request`] where only some of the fields matter. Obtained from [`Request::select`]
///
/// There is an implementation detail leak in the fact that the partial request
/// "forgets" which values it has ignored, so some requests are considered
/// "equal" even though semantically they seem like they shouldn't.
///
/// However, regular equality semantics _are_ guaranteed for partial requests
/// originating from the same [`RequestDeps`].
///
/// Note that this is almost trivial to fix by adding [`RequestDeps`] to the body, but I
/// just think it's unecessary.
// TODO: We could add it only on debug assertions? And then verify that we never compare two different partial requests?
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartialRequest {
    resolution: Resolution,
    time: Time,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RequestDeps: u8 {
        const TIME = 0b00000001;
        const RESOLUTION = 0b00000010;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId(u64);

impl std::hash::Hash for NodeId {
    /// No-op hash, since [`NodeId`] is already a hash.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0)
    }
}

impl NodeId {
    pub fn new(inputs: &[NodeInput]) -> Self {
        // TODO: Make sure this is guaranteed to be portable.
        let mut hasher = RapidHasher::new(0x040104);
        for input in inputs {
            input.hash(&mut hasher);
        }
        Self(hasher.finish())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeInput {
    Constant(SimpleValue),
    Node(NodeId),
}

impl NodeInput {
    pub fn as_node(&self) -> Option<NodeId> {
        match self {
            NodeInput::Node(node) => Some(*node),
            _ => None,
        }
    }
}

pub trait OperationMacro {
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
        Self: OperationMacro,
    {
        renderer.extent(self.main_input())
    }

    fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution>
    where
        Self: OperationMacro,
    {
        renderer.resolution(self.main_input())
    }

    fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>>
    where
        Self: OperationMacro,
    {
        renderer.fps(self.main_input())
    }

    fn resolve(
        strings: &StrInterner,
        args: impl Iterator<Item = (Option<Str>, eyre::Result<NodeInput>)>,
    ) -> eyre::Result<Self>
    where
        Self: Sized + OperationMacro,
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
                    .index_of(strings.resolve(name).unwrap())
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

pub enum Value {
    Simple(SimpleValue),
    Frame(Frame),
    Extent(Extent),
    Fps(Fps),
    Resolution(Resolution),
}

impl Value {
    pub fn to_frame(self) -> eyre::Result<Frame> {
        match self {
            Self::Frame(frame) => Ok(frame),
            _ => eyre::bail!("not a frame"),
        }
    }

    pub fn to_float(self) -> eyre::Result<f64> {
        match self {
            Self::Simple(SimpleValue::Float(v)) => Ok(v.into()),
            _ => eyre::bail!("not a float"),
        }
    }

    pub fn to_str(self) -> eyre::Result<Str> {
        match self {
            Self::Simple(SimpleValue::Str(v)) => Ok(v),
            _ => eyre::bail!("not a string"),
        }
    }
}
