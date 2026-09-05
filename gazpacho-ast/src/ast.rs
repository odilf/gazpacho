//! The abstract syntax tree. Stored as an arena of expressions with source
//! spans.
//!
//! Spans are kept so that GUI edits can be emitted as span-based text patches.

use gazpacho_datatypes::{Bool, Float, Int, SimpleValue, Str, StrInterner, Time};

/// A byte range in the source text.
///
/// Guaranteed to be at UTF8 boundaries of the source.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

impl ExprId {
    // TODO: I feel this shouldn't be necessary?
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// "Proper" names that appear in a program. This is opposed to string values, which are surrounded in quotes.
///
/// Names cannot be arbitrary, but we still cache them with the same string interning.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name(Option<Str>);

impl Name {
    pub fn new(name: Str) -> Self {
        Self(Some(name))
    }

    /// Sentinel value to use in place where a name was expected but couldn't be parsed.
    pub fn error() -> Name {
        Name(None)
    }

    pub fn str(&self) -> Option<Str> {
        self.0
    }

    /// Resolve the name. Returns `<error>` for names that have errored.
    pub fn resolve(self, str_interner: &StrInterner) -> &str {
        self.str()
            .map(|str| str_interner.resolve(str))
            .unwrap_or("<error>")
    }
}

/// A literal such as `42`, `2.5`, `true`, `"hello"` or `24000/101`. Ratios
/// are the more unusal built-in literal, but it's useful for exact time
/// calculations.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum Literal {
    Int(Int),
    Float(Float),
    Bool(Bool),
    Str(Str),
    Time(Time),
}

impl From<Literal> for SimpleValue {
    fn from(value: Literal) -> Self {
        match value {
            Literal::Int(v) => SimpleValue::Int(v),
            Literal::Float(v) => SimpleValue::Float(v),
            Literal::Bool(v) => SimpleValue::Bool(v),
            Literal::Str(v) => SimpleValue::Str(v),
            Literal::Time(v) => SimpleValue::Time(v),
        }
    }
}

/// A function arument _value_. They can be named, optionally.
///
/// See [`Param`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<Name>,
    pub value: ExprId,
}

impl Arg {
    pub fn anon(value: ExprId) -> Self {
        Self { name: None, value }
    }
}

/// A built-in operator with dedicated surface syntax (`-x`, `a + b`, `a..b`).
///
/// Distinct from built-in *functions* like `load`, which are ordinary
/// [`Expr::Call`]s resolved by name.
///
/// Not compatible with pipes.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Unary {
        op: UnaryOp,
        operand: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// A flattened run of one associative operator: `a + b + c`.
    Variadic {
        op: VariadicOp,
        operands: Vec<ExprId>,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    Subtract,
    Divide,
    Range,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariadicOp {
    Sum,
    Multiply,
}

impl UnaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
        }
    }
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Subtract => "-",
            BinaryOp::Divide => "/",
            BinaryOp::Range => "..",
        }
    }
}

impl VariadicOp {
    pub fn symbol(self) -> &'static str {
        match self {
            VariadicOp::Sum => "+",
            VariadicOp::Multiply => "*",
        }
    }
}

/// A function parameter _declaration_. Can be typed and given a default.
///
/// See [`Arg`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub ty: Option<TypeExpr>,
    pub default: Option<ExprId>,
}

/// Type annotation such as `Clip<Frame>`.
// NOTE: Enum because we will add variants later (such as records or fns).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Named { name: Name, args: Vec<TypeExpr> },
}

/// An expression.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Literal),
    Var(Name),
    Call {
        callee: ExprId,
        args: Vec<Arg>,
    },
    Operator(Operator),
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
    /// The `.field` accessor shorthand: a function that projects `field` out of
    /// its single argument (`map(xs, .at)`). Kept as its own node rather than
    /// desugared to a lambda so it needs no synthetic parameter name.
    FieldAccessor {
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

impl Expr {
    pub fn type_name(&self) -> &str {
        match self {
            Expr::Lit(..) => "literal",
            Expr::Var(..) => "var",
            Expr::Call { .. } => "call",
            Expr::Operator(..) => "operator",
            Expr::Let { .. } => "let",
            Expr::Lambda { .. } => "lambda",
            Expr::List(..) => "list",
            Expr::Record(..) => "record",
            Expr::Field { .. } => "field",
            Expr::FieldAccessor { .. } => "field accessor",
            Expr::Wgsl { .. } => "wgsl",
            Expr::Script { .. } => "script",
            Expr::Error => "error",
        }
    }
}

/// A top-level definition (`def name(params) = body` or `def const = value`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Def {
    pub name: Name,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: ExprId,
}

/// An import (`import "grades.gzp" as grades`)
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub path: Str,
    pub alias: Name,
}

/// A gazpacho module. It contains [`Def`]s and a tail [`Expr`] the module
/// evaluates to.
///
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct Module {
    exprs: Vec<Expr>,
    spans: Vec<Span>,
    pub defs: Vec<Def>,
    /// The module's value as a tail expression.
    pub value: Option<ExprId>,
}

impl Module {
    pub fn empty() -> Self {
        Self {
            exprs: Vec::new(),
            spans: Vec::new(),
            defs: Vec::new(),
            value: None,
        }
    }
}

#[expect(
    clippy::indexing_slicing,
    reason = "an `ExprId` is almost always read back against the same `Module`
    it came from. We admit a silent panic condition if one made multiple
    `Module`s and cross-referenced them. Reads from one arena always stay in
    bounds since the arena never deletes elements"
)]
impl Module {
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
}
