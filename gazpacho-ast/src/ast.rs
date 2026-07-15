//! The abstract syntax tree. Stored as an arena of expressions with source spans.
//!
//! Syntactic sugar (`|>`, `2s`, `a..b`) is desugared to plain `Call`s at parse
//! time, and spans are kept so that GUI edits can be emitted as span-based
//! text patches.

use num_rational::Rational64;
use std::fmt;

/// A byte range in the source text.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span {
            start: start as u32,
            end: end as u32,
        }
    }

    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Index of an expression in a [`Module`]'s arena.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

impl ExprId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

// NOTE: Plain string for now; interning can come later without changing the API surface much.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name(pub String);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Name {
    fn from(s: &str) -> Self {
        Name(s.to_owned())
    }
}

/// A literal such as `42`, `2.5`, `true`, `"hello"` or `24000/101`. Ratios
/// are the more unusal built-in literal, but it's useful for exact time
/// calculations.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Time(Rational64),
}

/// A function arument _value_. They can be named, optionally.
///
/// See [`Param`].
#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub name: Option<Name>,
    pub value: ExprId,
}

impl Arg {
    pub fn anon(value: ExprId) -> Self {
        Self { name: None, value }
    }
}

/// Things that can be called. Either names of bindings, or [`Builtin`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callee {
    /// User-defined
    Expr(ExprId),
    Builtin(Builtin),
}

impl Callee {
    pub fn expr(&self) -> Option<ExprId> {
        match self {
            &Self::Expr(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Built-in functions
pub enum Builtin {
    // Binops
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    // TODO: Think about tense naming
    Sum,
    Subtract,
    Multiply,
    Divide,

    Range,
    Neg,
}

impl Builtin {
    pub fn symbol(self) -> &'static str {
        match self {
            Builtin::Lt => "<",
            Builtin::Gt => ">",
            Builtin::Le => "<=",
            Builtin::Ge => ">=",
            Builtin::Eq => "==",
            Builtin::Ne => "!=",
            Builtin::Sum => "+",
            Builtin::Subtract => "-",
            Builtin::Multiply => "*",
            Builtin::Divide => "/",
            Builtin::Range => "..",
            Builtin::Neg => "-",
        }
    }
}

/// A function parameter _declaration_. Can be typed and given a default.
///
/// See [`Arg`].
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: Name,
    pub ty: Option<TypeExpr>,
    pub default: Option<ExprId>,
}

/// Type annotation such as `Clip<Frame>`.
// NOTE: Enum because we will add variants later (such as records or fns).
#[derive(Clone, Debug, PartialEq)]
pub enum TypeExpr {
    Named { name: Name, args: Vec<TypeExpr> },
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Lit(Literal),
    Var(Name),
    Call {
        callee: Callee,
        args: Vec<Arg>,
    },
    Let {
        bindings: Vec<(Name, ExprId)>,
        body: ExprId,
    },
    Lambda {
        params: Vec<Param>,
        body: ExprId,
    },
    List(Vec<ExprId>),
    Record(Vec<(Name, ExprId)>),
    Field {
        base: ExprId,
        field: Name,
    },
    // Opaque until the GPU phase; kept so the AST doesn't need to change.
    Wgsl {
        source: String,
        inputs: Vec<Param>,
    },
    Script {
        lang: Name,
        source: String,
        inputs: Vec<Param>,
    },
    /// Produced by error recovery so projections keep working mid-edit.
    Error,
}

/// A top-level definition (`def name(params) = body` or `def const = value`).
#[derive(Clone, Debug, PartialEq)]
pub struct Def {
    pub name: Name,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: ExprId,
}

/// An import (`import "grades.gazpacho" as grades`)
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub path: String,
    pub alias: Name,
}

/// A gazpacho module. It contains [`Def`]s and a tail [`Expr`] the module
/// evaluates to.
#[derive(Debug, Clone)]
pub struct Module {
    // Invariant: `exprs.len() == spans.len()`
    // Could use soa...
    // TODO: Does it even make sense to store as soa? What's the access pattern?
    exprs: Vec<Expr>,
    spans: Vec<Span>,
    pub imports: Vec<Import>,
    pub defs: Vec<Def>,
    /// The module's value as a tail expression.
    pub value: Option<ExprId>,
}

impl Module {
    pub const fn empty() -> Self {
        Self {
            exprs: Vec::new(),
            spans: Vec::new(),
            imports: Vec::new(),
            defs: Vec::new(),
            value: None,
        }
    }

    pub fn alloc(&mut self, expr: Expr, span: Span) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(expr);
        self.spans.push(span);
        id
    }

    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id.index()]
    }

    pub(crate) fn expr_mut(&mut self, id: ExprId) -> &mut Expr {
        &mut self.exprs[id.index()]
    }

    pub fn span(&self, id: ExprId) -> Span {
        self.spans[id.index()]
    }

    pub(crate) fn span_mut(&mut self, id: ExprId) -> &mut Span {
        &mut self.spans[id.index()]
    }

    pub fn def(&self, name: &str) -> Option<&Def> {
        self.defs.iter().find(|d| d.name.0 == name)
    }
}
