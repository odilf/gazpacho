//! The abstract syntax tree. Stored as an arena of expressions with source
//! spans.
//!
//! Spans are kept so that GUI edits can be emitted as span-based text patches.

use num_rational::Rational64;
use ordered_float::OrderedFloat;

/// A byte range in the source text.
///
/// Guaranteed to be at UTF8 boundaries of the source.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

impl ExprId {
    // TODO: I feel this shouldn't be necessary?
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// An interned stringid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Str(u32);

/// "Proper" names that appear in a program (as opposed to string values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name(pub Str);

/// A literal such as `42`, `2.5`, `true`, `"hello"` or `24000/101`. Ratios
/// are the more unusal built-in literal, but it's useful for exact time
/// calculations.
#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum Literal {
    Int(i64),
    Float(OrderedFloat<f64>),
    Bool(bool),
    Str(Str),
    Time(Rational64),
}

/// A function arument _value_. They can be named, optionally.
///
/// See [`Param`].
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub ty: Option<TypeExpr>,
    pub default: Option<ExprId>,
}

/// Type annotation such as `Clip<Frame>`.
// NOTE: Enum because we will add variants later (such as records or fns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Named { name: Name, args: Vec<TypeExpr> },
}

/// An expression.
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
#[derive(Debug, Clone, PartialEq)]
pub struct Def {
    pub name: Name,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: ExprId,
}

/// An import (`import "grades.gazpacho" as grades`)
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub path: Str,
    pub alias: Name,
}

/// A gazpacho module. It contains [`Def`]s and a tail [`Expr`] the module
/// evaluates to.
///
#[derive(Debug, Clone)]
pub struct Module {
    exprs: Vec<Expr>,
    spans: Vec<Span>,
    strings: Vec<String>,
    // TODO: Import semantic need work. Right now they are just parsed and that's it.
    // I think it might make more sense to have imports as expressions that evaluate to
    // the tail expression of their given module. Nix style.
    pub imports: Vec<Import>,
    pub defs: Vec<Def>,
    /// The module's value as a tail expression.
    pub value: Option<ExprId>,
}

impl Module {
    pub fn empty() -> Self {
        Self {
            exprs: Vec::new(),
            spans: Vec::new(),
            strings: Vec::new(),
            imports: Vec::new(),
            defs: Vec::new(),
            value: None,
        }
    }
    pub fn name_str(&self, name: Name) -> &str {
        self.str(name.0)
    }

    pub fn def(&self, name: &str) -> Option<&Def> {
        self.defs.iter().find(|d| self.name_str(d.name) == name)
    }

    /// If the expression is a variable, get its name.
    pub fn var_name(&self, expr: ExprId) -> Option<&str> {
        let Expr::Var(Name(str)) = self.expr(expr) else {
            return None;
        };

        Some(self.str(*str))
    }
}

#[expect(
    clippy::indexing_slicing,
    reason = "an `ExprId` is almost always read back against the same `Module`
    it came from, but we admit a silent panic condition if one made multiple
    `Module`s and cross-referenced them. Reads from one arena always stay in
    bounds since the arena never deletes elements (same for `Str`)"
)]
impl Module {
    pub fn alloc(&mut self, expr: Expr, span: Span) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(expr);
        self.spans.push(span);
        id
    }

    /// Gets the interned string or interns a new string.
    pub fn get_str_or_intern(&mut self, value: &str) -> Str {
        let i = self
            .strings
            // FIXME: Lookup is linear in time.
            .iter()
            .position(|s| s == value)
            .unwrap_or_else(|| {
                let i = self.strings.len();
                self.strings.push(value.to_string());
                i
            });

        Str(i as u32)
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

    pub fn str(&self, value: Str) -> &str {
        &self.strings[usize::try_from(value.0).expect("less than 2^32 strings")]
    }
}
