use std::{any::Any, collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    data::DataType,
    graph::port::{DynPortType, PortInRef, PortOutRef, PortRef, PortType},
    node::NodeSpec,
};

/// A reference to a [`NodeInstance`] in a particular [`Graph`].
///
/// It is guaranteed to point to a valid node of the graph it came from. At least, guaranteed until
/// I implement deleting nodes...
///
/// See also [`PortRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef(pub(super) usize);

/// An instance of a [`NodeSpec`] in a [`Graph`].
#[derive(Clone, Serialize, Deserialize)]
pub struct NodeInstance {
    pub(super) spec: &'static NodeSpec,

    /// The ports that the input of this node connects to.
    ///
    /// Invariant: `self.inputs` has exactly the same length as the number of input ports of the
    /// node it refers to.
    pub(super) inputs: Box<[Option<PortOutRef>]>,

    /// The ports that the input of this node connects to.
    ///
    /// Invariant: `self.outputs` has exactly the same length as the number of output ports of the
    /// node it refers to.
    // TODO: Using a whole ass `HashSet` here is overkill. Maybe try a `VecSet`.
    pub(super) outputs: Box<[HashSet<PortInRef>]>,

    // TODO: This feels unecessary and ugly. But it is practical...
    pub(super) self_ref: NodeRef,
}

impl fmt::Debug for NodeInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}, ", self.self_ref)?;
        let mut fs = f.debug_struct(self.spec.id().as_str());

        for (input, source) in self.inputs() {
            fs.field("input", &format_args!("{source:?} -> {input:?}"));
        }

        for (output, dests) in self.outputs() {
            fs.field("output", &format_args!("{output:?} -> {dests:?}"));
        }

        fs.finish()
    }
}

impl NodeInstance {
    pub fn spec(&self) -> &'static NodeSpec {
        self.spec
    }

    pub fn port_refs<T: PortType>(&self) -> impl ExactSizeIterator<Item = PortRef<T>> {
        let n = match T::TYPE {
            DynPortType::Input => self.spec().inputs_ref().len() + self.spec().inputs_own().len(),
            DynPortType::Output => self.spec().outputs().len(),
        };

        // Assuming internal invariant: Port indices are always in order, starting from `0`.
        (0..n).map(move |i| PortRef {
            node: self.self_ref,
            port_index: i,
            meta: T::INSTANCE,
        })
    }

    pub fn inputs(
        &self,
    ) -> impl ExactSizeIterator<Item = (PortInRef, Option<PortOutRef>)> + use<'_> {
        self.port_refs().zip(&self.inputs).map(|(p, &i)| (p, i))
    }

    pub fn outputs(
        &self,
    ) -> impl ExactSizeIterator<Item = (PortOutRef, &HashSet<PortInRef>)> + use<'_> {
        self.port_refs().zip(&self.outputs)
    }

    pub fn named_port_ref<T: PortType>(&self, name: &str) -> Option<PortRef<T>> {
        self.io().get_named_port(name)
    }

    pub fn typed_port_ref<T: PortType>(&self, typ: DataType) -> Option<PortRef<T>> {
        let port_index = if T::IS_INPUT {
            self.spec().inputs().position(|port| port.typ() == typ)?
        } else {
            self.spec()
                .outputs()
                .iter()
                .position(|(port, _)| port.typ() == typ)?
        };

        Some(PortRef {
            node: self.self_ref,
            port_index,
            meta: T::INSTANCE,
        })
    }

    #[inline]
    pub const fn io(&self) -> NodeIo {
        NodeIo {
            node_ref: self.self_ref,
            spec: self.spec,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeIo {
    node_ref: NodeRef,
    spec: &'static NodeSpec,
}

impl NodeIo {
    pub fn spec(&self) -> &'static NodeSpec {
        self.spec
    }

    pub fn get_named_port<T: PortType>(&self, name: &str) -> Option<PortRef<T>> {
        let port_index = if T::IS_INPUT {
            self.spec().inputs().position(|port| port.name() == name)?
        } else {
            self.spec()
                .outputs()
                .iter()
                .position(|(port, _)| port.name() == name)?
        };

        Some(PortRef {
            node: self.node_ref,
            port_index,
            meta: T::INSTANCE,
        })
    }

    pub fn as_node_ref(&self) -> NodeRef {
        self.node_ref
    }

    pub fn port<T: PortType>(&self, name: &str) -> PortRef<T> {
        self.get_named_port(name).unwrap()
    }

    // /// References to the node's input ports. Returns a tuple of `(borrowed, owned)` inputs.
    // fn input_port_refs(
    //     &self,
    // ) -> (
    //     impl ExactSizeIterator<Item = PortInRef>,
    //     impl ExactSizeIterator<Item = PortInRef>,
    // ) {
    //     (
    //         (0..self.spec.inputs_ref().len()).map(|i| PortRef {
    //             node: self.node_ref,
    //             port_index: i,
    //             meta: InputPort,
    //         }),
    //         (0..self.spec.inputs_own().len()).map(|i| PortRef {
    //             node: self.node_ref,
    //             port_index: i + self.spec.inputs_ref().len(),
    //             meta: InputPort,
    //         }),
    //     )
    // }
}
