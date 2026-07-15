//! The gazpacho AST: the ground truth of a project.
//!
//! A project is text file (`.gazpacho`) that parses into a small pure functional
//! expression language. This can be compiled into a render plan as, essentially,
//! bytecode. The editor is just a tool to modify the AST and see the result via
//! just-in-time compiling the project.

pub mod ast;
pub mod lex;
pub mod parse;
pub mod print;

pub use ast::{Arg, Def, Expr, ExprId, Import, Literal, Module, Name, Param, Span, TypeExpr};
pub use parse::{ParseError, parse};
pub use print::{print, print_expr};
