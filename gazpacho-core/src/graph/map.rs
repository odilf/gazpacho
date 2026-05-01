use std::{
    marker::PhantomData,
    ops::{Deref, Index, IndexMut},
};

use serde::{Deserialize, Serialize};

use crate::graph::{
    OutputPort,
    node_instance::NodeRef,
    port::{PortRef, PortType},
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeMap<V>(pub(super) Vec<V>);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortMap<V, T: PortType = OutputPort>(pub(super) NodeMap<Box<[V]>>, PhantomData<T>);

impl<V> NodeMap<V> {
    pub const fn new() -> Self {
        NodeMap(Vec::new())
    }
}

impl<V, T: PortType> PortMap<V, T> {
    pub const fn new() -> Self {
        PortMap(NodeMap::new(), PhantomData)
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

impl<V, T: PortType> Index<PortRef<T>> for PortMap<V, T> {
    type Output = V;
    fn index(&self, index: PortRef<T>) -> &Self::Output {
        &self.0[index.node][index.port_index]
    }
}

impl<V, T: PortType> IndexMut<PortRef<T>> for PortMap<V, T> {
    fn index_mut(&mut self, index: PortRef<T>) -> &mut Self::Output {
        &mut self.0[index.node][index.port_index]
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
impl<V, T: PortType> Default for PortMap<V, T> {
    fn default() -> Self {
        Self(NodeMap::default(), PhantomData)
    }
}
