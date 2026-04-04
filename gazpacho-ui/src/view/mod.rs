mod graph;
mod node;
mod timeline;

pub use graph::GraphViewState;
pub use node::NodeViewState;
pub use timeline::TimelineViewState;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum View {
    #[default]
    Graph,
    Node,
    Timeline,
}
