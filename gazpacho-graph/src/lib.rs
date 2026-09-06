//! Definition and types for working with a render graph.
//!
//! This crate serves as glue for `gazpacho_operations` to have the necessary
//! types to define the operations. The render graph itself is defined in
//! `gazpacho_compile`, since it includes the operations.

mod node;
mod request;
mod value;

pub use node::{NodeId, NodeInput};
pub use request::{PartialRequest, Request, RequestDeps};
pub use value::Value;
