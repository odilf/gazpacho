use crate::graph::{
    node_instance::NodeRef,
    port::{PortRef, PortType},
};

pub trait NodeMap<V> {
    fn get_node(self, node: NodeRef) -> V;
}

pub trait PortMap<V, T: PortType> {
    fn get_port(self, port: PortRef<T>) -> V;
}

impl<'a, V> NodeMap<&'a V> for &'a [V] {
    fn get_node(self, node: NodeRef) -> &'a V {
        &self[node.0]
    }
}
impl<'a, V> NodeMap<&'a mut V> for &'a mut [V] {
    fn get_node(self, node: NodeRef) -> &'a mut V {
        &mut self[node.0]
    }
}

impl<'a, T: PortType, V> PortMap<&'a V, T> for &'a [Box<[V]>] {
    fn get_port(self, port: PortRef<T>) -> &'a V {
        &self.get_node(port.node)[port.port_index()]
    }
}

impl<'a, T: PortType, V> PortMap<&'a mut V, T> for &'a mut [Box<[V]>] {
    fn get_port(self, port: PortRef<T>) -> &'a mut V {
        &mut self.get_node(port.node)[port.port_index()]
    }
}
