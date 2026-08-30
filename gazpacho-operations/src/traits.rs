use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, SimpleValue, Time};

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
        todo!("hash inputs")
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

pub trait Operation {
    const NAME: &str;
    const SIGNATURE: Signature;
    const DEPS: RequestDeps;
    const INDEPS: RequestDeps;

    fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame>;
    fn inputs(&self) -> &[NodeInput];
    fn inputs_mut(&mut self) -> &mut [NodeInput];

    fn main_input(&self) -> NodeInput {
        #[expect(
            clippy::indexing_slicing,
            reason = "Nodes need at least one input, otherwise they would be
            constants (this needs to be upheld in `Op`)."
        )]
        self.inputs()[0]
    }

    fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent> {
        renderer.extent(self.main_input())
    }

    fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution> {
        renderer.resolution(self.main_input())
    }
    fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>> {
        renderer.fps(self.main_input())
    }
}

pub trait Renderer {
    fn extent(&mut self, node: NodeInput) -> eyre::Result<Extent>;
    fn resolution(&mut self, node: NodeInput) -> eyre::Result<Resolution>;
    fn fps(&mut self, node: NodeInput) -> eyre::Result<Option<Fps>>;
    fn eval(&mut self, node: NodeInput, request: Request) -> eyre::Result<Value>;
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
}
