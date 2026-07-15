mod map;
mod node_instance;
mod port;
mod render;

pub use node_instance::{NodeIo, NodeRef};
pub use port::{GenericPortRef, InputPort, OutputPort, PortInRef, PortOutRef, PortRef, PortType};

use std::{any::Any, collections::HashSet};

use color_eyre::eyre;
use serde::{Deserialize, Serialize};

use crate::{
    data::{Port, SimpleDataValue},
    graph::{map::NodeMap, node_instance::NodeInstance},
    node::NodeSpec,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    nodes: NodeMap<NodeInstance>,
    // TODO: Don't skip, the `NodeSpec` should know how to serialize and deserialize its data.
    #[serde(skip)]
    node_data: NodeMap<Box<dyn Any>>,
}

/// Immutable version of [`Graph`].
#[derive(Debug, Clone, Copy)]
pub struct SimpleGraph<'a> {
    nodes: &'a [NodeInstance],
}

/// Navigation of immutable graphs.
pub trait ImmutableGraph {
    fn nodes(&self) -> &[NodeInstance];

    /// Iterates references to all nodes of the graph.
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + use<Self> {
        // Assuming internal invariant: `NodeId`s are always in order, starting from `0`.
        (0..self.nodes().len()).map(NodeRef)
    }

    fn get(&self, node_ref: NodeRef) -> &NodeInstance {
        self.nodes()
            .get(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    fn port_refs<T: PortType>(
        &self,
        node_ref: NodeRef,
    ) -> impl ExactSizeIterator<Item = PortRef<T>> {
        self.get(node_ref).port_refs::<T>()
    }

    fn get_port<T: PortType>(&self, port_ref: PortRef<T>) -> Port {
        if T::IS_INPUT {
            *self
                .get(port_ref.node)
                .spec()
                .inputs()
                .get(port_ref.port_index)
                .unwrap()
        } else {
            self.get(port_ref.node)
                .spec()
                .outputs()
                .get(port_ref.port_index)
                .unwrap()
                .0
        }
    }

    fn connection(&self, port: PortInRef) -> Option<PortOutRef> {
        self.get(port.node).inputs[port.port_index()]
    }

    fn is_connected(&self, output: PortOutRef, input: PortInRef) -> bool {
        self.get(input.node).inputs[input.port_index] == Some(output)
    }
}

impl ImmutableGraph for Graph {
    fn nodes(&self) -> &[NodeInstance] {
        &self.nodes
    }
}

impl ImmutableGraph for SimpleGraph<'_> {
    fn nodes(&self) -> &[NodeInstance] {
        self.nodes
    }
}

impl Graph {
    /// Constructs a new empty [`Graph`].
    pub fn new() -> Self {
        Self {
            nodes: NodeMap::new(),
            node_data: NodeMap::new(),
        }
    }

    #[inline]
    pub fn as_simple(&self) -> SimpleGraph<'_> {
        SimpleGraph { nodes: &self.nodes }
    }

    pub fn insert_node(&mut self, node: &'static NodeSpec) -> NodeRef {
        let node_ref = NodeRef(self.nodes.len());
        let inputs = node.inputs().iter().map(|_| None).collect();
        let outputs = node.outputs().iter().map(|_| HashSet::new()).collect();

        self.nodes.0.push(NodeInstance {
            spec: node,
            inputs,
            outputs,
            self_ref: node_ref,
        });
        self.node_data.0.push(node.init_data());

        node_ref
    }

    pub(self) fn get_mut(&mut self, node_ref: NodeRef) -> &mut NodeInstance {
        &mut self.nodes[node_ref]
    }

    pub fn connect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = Some(output);
        self.get_mut(output.node).outputs[output.port_index].insert(input);
    }

    pub fn disconnect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = None;
        self.get_mut(output.node).outputs[output.port_index].remove(&input);
    }

    pub fn get_const(&self, node_ref: NodeRef) -> Option<SimpleDataValue> {
        self.node_data[node_ref]
            .downcast_ref::<SimpleDataValue>()
            .cloned()
    }

    pub fn set_const(&mut self, node_ref: NodeRef, value: SimpleDataValue) -> eyre::Result<()> {
        let node = self.get_mut(node_ref);
        if node.spec().is_const().is_none() {
            eyre::bail!("Trying to set a const on a non-const node");
        }

        self.node_data[node_ref] = Box::new(value);
        Ok(())
    }

    pub fn set_const_input(
        &mut self,
        input: PortInRef,
        value: impl Into<SimpleDataValue>,
    ) -> PortOutRef {
        let value = value.into();
        let const_node = self.insert_node(value.typ().const_node());
        self.node_data[const_node] = Box::new(value);
        let const_port = self
            .get(const_node)
            .port_refs()
            .next()
            .expect("Const nodes have exactly one output.");

        self.connect(const_port, input);
        const_port
    }
}
