use std::{
    collections::HashMap,
    hash::{Hash as _, Hasher as _},
};

use gazpacho_ast::{Literal, Module};
use rapidhash::fast::RapidHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    fn new(inputs: &[NodeInput]) -> Self {
        let mut s = RapidHasher::new(0x040104);
        inputs.hash(&mut s);
        Self(s.finish())
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    op: Op,
    inputs: Box<[NodeInput]>,
}

impl Node {
    pub const fn op(&self) -> Op {
        self.op
    }

    pub const fn inputs(&self) -> &[NodeInput] {
        &self.inputs
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum NodeInput {
    Constant(Literal),
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Load,
    Contrast,
    Constant(Literal),
}

pub struct RenderGraph {
    // TODO: Use nohash_hasher.
    nodes: HashMap<NodeId, Node>,
    // TODO: This shouldn't be here, we only use it for the string interning.
    // And it certainly shouldn't be `pub`...
    pub module: Module,
}

impl RenderGraph {
    pub(crate) fn new(module: Module) -> Self {
        Self {
            nodes: HashMap::new(),
            module,
        }
    }

    pub fn get(&self, node: NodeId) -> &Node {
        self.nodes.get(&node).unwrap()
    }

    pub fn insert(&mut self, op: Op, inputs: Box<[NodeInput]>) -> NodeId {
        let id = NodeId::new(&inputs);
        self.nodes.insert(id, Node { op, inputs });

        id
    }
}
