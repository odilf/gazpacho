use egui::{Pos2, ahash::HashMap};
use jamon_core::graph::{Graph, NodeRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphViewState {
    graph: Graph,
    node_positions: HashMap<NodeRef, Pos2>,
}
