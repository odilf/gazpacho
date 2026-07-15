use std::ops::{Deref, Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::graph::node_instance::NodeRef;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeMap<V>(pub(super) Vec<V>);

impl<V> NodeMap<V> {
    pub const fn new() -> Self {
        NodeMap(Vec::new())
    }
}

impl<V> Index<NodeRef> for NodeMap<V> {
    type Output = V;
    fn index(&self, index: NodeRef) -> &Self::Output {
        &self.0[index.0]
    }
}

impl<V> IndexMut<NodeRef> for NodeMap<V> {
    fn index_mut(&mut self, index: NodeRef) -> &mut Self::Output {
        &mut self.0[index.0]
    }
}

impl<V> Deref for NodeMap<V> {
    type Target = [V];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> Default for NodeMap<V> {
    fn default() -> Self {
        Self(Vec::default())
    }
}
