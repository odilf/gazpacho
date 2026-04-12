use std::fmt;

use serde::{Deserialize, Serialize};

use crate::graph::node_instance::NodeRef;

/// A reference to a [`Port`] in a particular [`Graph`].
/// Can be either a [`PortInRef`] or [`PortOutRef`].
///
/// See also [`NodeRef`].
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortRef<T> {
    pub(super) node: NodeRef,
    pub(super) port_index: usize,
    pub(super) meta: T,
}

impl<T: PortType> fmt::Debug for PortRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.node.0, self.port_index)
    }
}

impl fmt::Debug for GenericPortRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} ({})", self.node.0, self.port_index, self.meta)
    }
}

impl<T> PortRef<T> {
    pub const fn node(&self) -> NodeRef {
        self.node
    }

    pub const fn port_index(&self) -> usize {
        self.port_index
    }
}

mod private {
    pub trait PortTypeSeal {}
}

pub trait PortType: private::PortTypeSeal + Copy + std::hash::Hash + Send + Sync {
    const TYPE: DynPortType;
    const IS_INPUT: bool = matches!(Self::TYPE, DynPortType::Input);
    const INSTANCE: Self;
    type Other: PortType;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DynPortType {
    Input,
    Output,
}

impl fmt::Display for DynPortType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Input => "in",
            Self::Output => "out",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct InputPort;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutputPort;

impl private::PortTypeSeal for InputPort {}
impl private::PortTypeSeal for OutputPort {}

impl PortType for InputPort {
    const TYPE: DynPortType = DynPortType::Input;
    const INSTANCE: Self = Self;
    type Other = OutputPort;
}

impl PortType for OutputPort {
    const TYPE: DynPortType = DynPortType::Output;
    const INSTANCE: Self = Self;
    type Other = InputPort;
}

pub type PortInRef = PortRef<InputPort>;
pub type PortOutRef = PortRef<OutputPort>;
pub type GenericPortRef = PortRef<DynPortType>;

impl<T: PortType> PortRef<T> {
    pub fn as_generic(self) -> GenericPortRef {
        PortRef {
            node: self.node,
            port_index: self.port_index,
            meta: T::TYPE,
        }
    }
}

impl GenericPortRef {
    pub fn input_output<T: PortType>(
        a: PortRef<T>,
        b: GenericPortRef,
    ) -> Option<(PortInRef, PortOutRef)> {
        match (T::TYPE, b.meta) {
            (DynPortType::Input, DynPortType::Output) => Some((
                PortInRef {
                    node: a.node,
                    port_index: a.port_index,
                    meta: InputPort::INSTANCE,
                },
                PortOutRef {
                    node: b.node,
                    port_index: b.port_index,
                    meta: OutputPort::INSTANCE,
                },
            )),
            (DynPortType::Output, DynPortType::Input) => Some((
                PortInRef {
                    node: b.node,
                    port_index: b.port_index,
                    meta: InputPort::INSTANCE,
                },
                PortOutRef {
                    node: a.node,
                    port_index: a.port_index,
                    meta: OutputPort::INSTANCE,
                },
            )),
            _ => None,
        }
    }
}
