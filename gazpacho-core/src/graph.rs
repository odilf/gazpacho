mod map;
mod node_instance;
mod port;
mod render;

pub use node_instance::{NodeIo, NodeRef};
pub use port::{GenericPortRef, InputPort, OutputPort, PortInRef, PortOutRef, PortRef, PortType};

use std::{any::Any, collections::HashSet};

use color_eyre::eyre::{self};
use serde::{Deserialize, Serialize};

use crate::{
    data::{DataValue, Port, SimpleDataValue},
    graph::{
        map::{NodeMap, PortMap as _},
        node_instance::NodeInstance,
    },
    node::NodeSpec,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    nodes: Vec<NodeInstance>,
    computed_values: Vec<Box<[Option<DataValue>]>>,
    // TODO: Don't skip, the `NodeSpec` should know how to serialize and deserialize its data.
    #[serde(skip)]
    node_data: Vec<Box<dyn Any>>,
}

/// Immutable version of [`Graph`].
///
/// Useful to be able to modify computed values while knowing that the graph structure won't change.
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
            self.get(port_ref.node)
                .spec()
                .inputs()
                .nth(port_ref.port_index)
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
            nodes: Vec::new(),
            computed_values: Vec::new(),
            node_data: Vec::new(),
        }
    }

    #[inline]
    pub fn as_simple(&self) -> SimpleGraph<'_> {
        SimpleGraph { nodes: &self.nodes }
    }

    #[inline]
    pub fn split<'a>(
        &'a mut self,
    ) -> (
        SimpleGraph<'a>,
        &'a mut [Box<[Option<DataValue>]>],
        &'a mut [Box<dyn Any>],
    ) {
        (
            SimpleGraph { nodes: &self.nodes },
            &mut self.computed_values,
            &mut self.node_data,
        )
    }

    pub fn insert_node(&mut self, node: &'static NodeSpec) -> NodeRef {
        let node_ref = NodeRef(self.nodes.len());
        let inputs = node.inputs().map(|_| None).collect();
        let outputs = node.outputs().iter().map(|_| HashSet::new()).collect();
        let computed = node.outputs().iter().map(|_| None).collect();
        self.nodes.push(NodeInstance {
            spec: node,
            inputs,
            outputs,
            self_ref: node_ref,
        });
        self.computed_values.push(computed);
        self.node_data.push(node.init_data());

        node_ref
    }

    // NOTE: Not `pub`, because then you could change values without properly
    // invalidating the cache!
    pub(self) fn get_mut(&mut self, node_ref: NodeRef) -> &mut NodeInstance {
        self.nodes
            .get_mut(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    pub fn invalidate_computed(&mut self, node_ref: NodeRef) {
        fn invalidate_impl(
            graph: SimpleGraph<'_>,
            computed: &mut [Box<[Option<DataValue>]>],
            node_ref: NodeRef,
        ) {
            let mut done = true;
            for port in graph.port_refs::<OutputPort>(node_ref) {
                let removed = computed.get_port(port).take();
                if removed.is_some() {
                    done = false;
                }
            }

            if done {
                return;
            }

            // TODO: I'd like to do this, but it's hard to communicate that I only want to modify `computed_values`...
            for inputs in &graph.get(node_ref).outputs {
                for input in inputs {
                    invalidate_impl(graph, computed, input.node);
                }
            }
        }

        let (graph, computed, _node_data) = self.split();
        invalidate_impl(graph, computed, node_ref)
    }

    pub fn connect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = Some(output);
        self.get_mut(output.node).outputs[output.port_index].insert(input);
        self.invalidate_computed(input.node);
    }

    pub fn disconnect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = None;
        self.get_mut(output.node).outputs[output.port_index].remove(&input);
        self.invalidate_computed(input.node);
    }

    pub fn get_const(&self, node_ref: NodeRef) -> Option<&DataValue> {
        self.node_data.get_node(node_ref).downcast_ref()
    }

    pub fn set_const(&mut self, node_ref: NodeRef, value: SimpleDataValue) -> eyre::Result<()> {
        let node = self.get_mut(node_ref);
        if node.spec().is_const().is_none() {
            eyre::bail!("Trying to set a const on a non-const node");
        }

        *self.node_data.as_mut_slice().get_node(node_ref) = Box::new(value);
        self.invalidate_computed(node_ref);
        Ok(())
    }

    pub fn set_const_input(
        &mut self,
        input: PortInRef,
        value: impl Into<SimpleDataValue>,
    ) -> PortOutRef {
        let value = value.into();
        let const_node = self.insert_node(value.typ().const_node());
        *self.node_data.as_mut_slice().get_node(const_node) = Box::new(value);
        let const_port = self
            .get(const_node)
            .port_refs()
            .next()
            .expect("Const nodes have exactly one output.");

        self.connect(const_port, input);
        self.invalidate_computed(const_node);

        const_port
    }

    fn transitive_named_port_ref<T: PortType>(
        &self,
        output: NodeRef,
        name: &str,
    ) -> eyre::Result<PortRef<T>> {
        // TODO: Technically this only gets the first one if there are multiple with the same name.
        if let Some(port) = self.get(output).named_port_ref(name) {
            return Ok(port);
        }

        let mut recursives = self
            .get(output)
            .inputs()
            .filter_map(|(_, output)| self.transitive_named_port_ref(output?.node, name).ok());

        let port = recursives
            .next()
            .ok_or_else(|| eyre::eyre!("No `{name}` port found."));

        // TODO: Do we really need to reject if a port has multiple candidates?
        if recursives.next().is_some() {
            eyre::bail!("Mutliple potential `{name}` ports found");
        }

        port
    }
}
