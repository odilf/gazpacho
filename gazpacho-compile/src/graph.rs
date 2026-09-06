use std::collections::HashMap;

use gazpacho_datatypes::StrInterner;
use gazpacho_graph::{NodeId, NodeInput, RequestDeps};
use gazpacho_operations::Op;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct Node {
    op: Op,
    deps: RequestDeps,
}

impl Node {
    pub const fn op(&self) -> Op {
        self.op
    }

    pub const fn deps(&self) -> RequestDeps {
        self.deps
    }
}

/// The graph for how to render a video.
///
/// This is defined here instead of in [`gazpacho_graph`] because the [`Node`]s contain
/// the [`Op`], which depends on the types in [`gazpacho_graph`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct RenderGraph {
    // TODO: Use nohash_hasher.
    nodes: HashMap<NodeId, Node>,
}

impl RenderGraph {
    pub(crate) fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn get(&self, node: NodeId) -> &Node {
        #[expect(
            clippy::indexing_slicing,
            reason = "If `node` comes from this graph, it resolves. If it
            doesn't, it panics, and that's ok."
        )]
        &self.nodes[&node]
    }

    pub fn insert(&mut self, op: Op, str_interner: &StrInterner) -> NodeId {
        let id = NodeId::new(op.inputs(), str_interner);

        let mut deps = op.deps();
        for input in op.inputs() {
            let NodeInput::Node(node) = input else {
                continue;
            };

            deps.insert(self.get(*node).deps);
        }
        deps.remove(op.indeps());

        self.nodes.insert(id, Node { op, deps });

        id
    }
}
