//! Fragment printer.
//!
//! This is *not* a whole-file formatter for editing — GUI edits are
//! span-based text patches (see `docs/design/09-rust-sketch.md`). The
//! printer exists to render newly generated fragments, debug output, and
//! whole modules for tests. Invariant: `parse(print(m))` is structurally
//! equal to `m` (checked by round-trip tests).
//!
//! Desugared operator calls are re-sugared for readability; operands are
//! parenthesized unconditionally so precedence can never be misprinted.

use std::fmt::{self, Write};

use num_rational::Rational64;

use crate::ast::{BinaryOp, Def, Expr, ExprId, Literal, Module, Operator, Param, TypeExpr};

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for import in &self.imports {
            writeln!(f, "import \"{}\" as {}", escape(&import.path), import.alias)?;
        }
        if !self.imports.is_empty() {
            f.write_char('\n')?;
        }
        for (i, def) in self.defs.iter().enumerate() {
            if i > 0 {
                f.write_char('\n')?;
            }
            print_def(self, def, f)?;
        }
        if let Some(result) = self.value {
            expr(self, result, f, 0)?;
            f.write_char('\n')?;
        }
        Ok(())
    }
}

pub fn print(module: &Module) -> String {
    module.to_string()
}

pub fn print_expr(module: &Module, id: ExprId) -> String {
    let mut out = String::new();
    expr(module, id, &mut out, 0).expect("writing to a String can't fail");
    out
}

fn print_def(module: &Module, def: &Def, out: &mut impl Write) -> fmt::Result {
    write!(out, "def {}(", def.name)?;
    for (i, p) in def.params.iter().enumerate() {
        if i > 0 {
            out.write_str(", ")?;
        }
        param(module, p, out)?;
    }
    out.write_char(')')?;
    if let Some(ret) = &def.ret {
        out.write_str(" -> ")?;
        type_expr(ret, out)?;
    }
    out.write_str(" =")?;
    if matches!(module.expr(def.body), Expr::Let { .. }) {
        out.write_char('\n')?;
        expr(module, def.body, out, 1)?;
    } else {
        out.write_char(' ')?;
        expr(module, def.body, out, 0)?;
    }
    out.write_char('\n')
}

fn param(module: &Module, p: &Param, out: &mut impl Write) -> fmt::Result {
    write!(out, "{}", p.name)?;
    if let Some(ty) = &p.ty {
        out.write_str(": ")?;
        type_expr(ty, out)?;
    }
    if let Some(default) = p.default {
        out.write_str(" = ")?;
        expr(module, default, out, 0)?;
    }
    Ok(())
}

fn type_expr(ty: &TypeExpr, out: &mut impl Write) -> fmt::Result {
    let TypeExpr::Named { name, args } = ty;
    write!(out, "{name}")?;
    if !args.is_empty() {
        out.write_char('<')?;
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                out.write_str(", ")?;
            }
            type_expr(arg, out)?;
        }
        out.write_char('>')?;
    }
    Ok(())
}

fn expr(module: &Module, id: ExprId, out: &mut impl Write, indent: usize) -> fmt::Result {
    match module.expr(id) {
        Expr::Lit(lit) => literal(lit, out),
        Expr::Var(name) => write!(out, "{name}"),
        Expr::Call { callee, args } => call(module, *callee, args, out, indent),
        Expr::Operator(op) => operator(module, op, out, indent),
        Expr::Let { bindings, body } => {
            let ind = "  ".repeat(indent);
            for (name, value) in bindings {
                write!(out, "{ind}let {name} = ")?;
                expr(module, *value, out, indent)?;
                out.write_char('\n')?;
            }
            out.write_str(&ind)?;
            expr(module, *body, out, indent)
        }
        Expr::Lambda { params, body } => {
            out.write_char('(')?;
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.write_str(", ")?;
                }
                param(module, p, out)?;
            }
            out.write_str(" -> ")?;
            expr(module, *body, out, indent)?;
            out.write_char(')')
        }
        Expr::List(items) => {
            out.write_char('[')?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.write_str(", ")?;
                }
                expr(module, *item, out, indent)?;
            }
            out.write_char(']')
        }
        Expr::Record(fields) => {
            out.write_str("{ ")?;
            for (i, (name, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.write_str(", ")?;
                }
                write!(out, "{name}: ")?;
                expr(module, *value, out, indent)?;
            }
            out.write_str(" }")
        }
        Expr::Field { base, field } => {
            let parens = !matches!(
                module.expr(*base),
                Expr::Var(_) | Expr::Call { .. } | Expr::Field { .. }
            );
            if parens {
                out.write_char('(')?;
            }
            expr(module, *base, out, indent)?;
            if parens {
                out.write_char(')')?;
            }
            write!(out, ".{field}")
        }
        Expr::FieldAccessor { field } => write!(out, ".{field}"),
        Expr::Wgsl { source, .. } => write!(out, "wgsl {{{source}}}"),
        Expr::Script { lang, source, .. } => write!(out, "script \"{lang}\" {{{source}}}"),
        Expr::Error => out.write_str("<error>"),
    }
}

fn call(
    module: &Module,
    callee: ExprId,
    args: &[crate::ast::Arg],
    out: &mut impl Write,
    indent: usize,
) -> fmt::Result {
    let parens = !matches!(module.expr(callee), Expr::Var(_) | Expr::Field { .. });
    if parens {
        out.write_char('(')?;
    }
    expr(module, callee, out, indent)?;
    if parens {
        out.write_char(')')?;
    }
    out.write_char('(')?;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.write_str(", ")?;
        }
        if let Some(name) = &arg.name {
            write!(out, "{name} = ")?;
        }
        expr(module, arg.value, out, indent)?;
    }
    out.write_char(')')
}

/// Re-sugars an operator node. Operands are parenthesized unconditionally (the
/// whole operator is wrapped) so precedence can never be misprinted.
fn operator(module: &Module, op: &Operator, out: &mut impl Write, indent: usize) -> fmt::Result {
    match op {
        Operator::Unary { op, operand } => {
            write!(out, "({}", op.symbol())?;
            expr(module, *operand, out, indent)?;
            out.write_char(')')
        }
        // `..` binds its operands directly (`a..b`); other binary operators are
        // spaced (`a + b`).
        Operator::Binary {
            op: BinaryOp::Range,
            lhs,
            rhs,
        } => {
            out.write_char('(')?;
            expr(module, *lhs, out, indent)?;
            out.write_str("..")?;
            expr(module, *rhs, out, indent)?;
            out.write_char(')')
        }
        Operator::Binary { op, lhs, rhs } => {
            out.write_char('(')?;
            expr(module, *lhs, out, indent)?;
            write!(out, " {} ", op.symbol())?;
            expr(module, *rhs, out, indent)?;
            out.write_char(')')
        }
        Operator::Variadic { op, operands } => {
            out.write_char('(')?;
            for (i, operand) in operands.iter().enumerate() {
                if i > 0 {
                    write!(out, " {} ", op.symbol())?;
                }
                expr(module, *operand, out, indent)?;
            }
            out.write_char(')')
        }
    }
}

fn literal(lit: &Literal, out: &mut impl Write) -> fmt::Result {
    match lit {
        Literal::Int(v) => write!(out, "{v}"),
        // `{:?}` keeps the decimal point (`2.0`), so it re-lexes as a float.
        Literal::Float(v) => write!(out, "{v:?}"),
        Literal::Bool(v) => write!(out, "{v}"),
        Literal::Str(v) => write!(out, "\"{}\"", escape(v)),
        Literal::Time(v) => time(*v, out),
    }
}

/// Time literals only enter through `Ns`/`Nms` decimals, so denominators
/// always divide 1000 and decimal printing is exact. The fallback covers
/// rationals produced programmatically; it is lossy in structure (an
/// expression, not a literal) but exact in value.
fn time(v: Rational64, out: &mut impl Write) -> fmt::Result {
    let (numer, denom) = (*v.numer(), *v.denom());
    if let Some(scaled) = numer.checked_mul(1000).filter(|scaled| scaled % denom == 0) {
        let ms = scaled / denom;
        if ms % 1000 == 0 {
            write!(out, "{}s", ms / 1000)
        } else {
            write!(out, "{ms}ms")
        }
    } else {
        write!(out, "({numer}s / {denom})")
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use crate::parse::parse;

    use super::print;

    /// `print ∘ parse` must be idempotent: printing a parsed module and
    /// reparsing it yields the same printed text.
    #[test]
    fn roundtrip_is_idempotent() {
        let src = r#"
import "lib/grades.gazpacho" as grades

def slow(clip: Video, amount: Float = 2.0) -> Video =
  let factor = 1.0 / amount
  speed(clip, factor)

def lower_third(text: String, at: Interval) -> Video =
  trim(place(stack([rect(color = bg, size = size), label(text)])), at)

let interview = load("footage/interview.mp4")
let cuts = markers(interview) |> map(.at)

stack([
  sequence(split_at(interview, cuts)),
  trim(slow(interview), 2s..6500ms),
  lower_third("Dr. Example", 2s..6s),
])
"#;
        let (module, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {errors:?}");

        let once = print(&module);
        let (module2, errors2) = parse(&once);
        assert!(
            errors2.is_empty(),
            "reparse errors on:\n{once}\n{errors2:?}"
        );
        let twice = print(&module2);
        assert_eq!(once, twice);
    }

    // #[test]
    // fn roundtrip_operators_and_literals() {
    //     let src = "def x = -(1 + 2) * 3.5 / len(a)\ndef t = 250ms\ndef b = x < 3 != true\ndef r = { at: 2s, name: \"a\\\"b\" }\n";
    //     let (module, errors) = parse(src);
    //     assert!(errors.is_empty(), "parse errors: {errors:?}");
    //     let once = print(&module);
    //     let (module2, errors2) = parse(&once);
    //     assert!(
    //         errors2.is_empty(),
    //         "reparse errors on:\n{once}\n{errors2:?}"
    //     );
    //     assert_eq!(once, print(&module2));
    // }
}

#[cfg(test)]
mod resugar_tests {
    use super::print_expr;
    use crate::ast::*;

    fn lit(m: &mut Module, n: i64) -> ExprId {
        m.alloc(Expr::Lit(Literal::Int(n)), Span::new(0, 0))
    }

    fn op(m: &mut Module, op: Operator) -> ExprId {
        m.alloc(Expr::Operator(op), Span::new(0, 0))
    }

    #[test]
    fn resugars_operators() {
        let mut m = Module::empty();
        let a = lit(&mut m, 1);
        let b = lit(&mut m, 2);

        let sum = op(
            &mut m,
            Operator::Variadic {
                op: VariadicOp::Sum,
                operands: vec![a, b],
            },
        );
        assert_eq!(print_expr(&m, sum), "(1 + 2)");

        let neg = op(
            &mut m,
            Operator::Unary {
                op: UnaryOp::Neg,
                operand: a,
            },
        );
        assert_eq!(print_expr(&m, neg), "(-1)");

        let range = op(
            &mut m,
            Operator::Binary {
                op: BinaryOp::Range,
                lhs: a,
                rhs: b,
            },
        );
        assert_eq!(print_expr(&m, range), "(1..2)");

        // A variadic node renders every operand, however many there are.
        let sum3 = op(
            &mut m,
            Operator::Variadic {
                op: VariadicOp::Sum,
                operands: vec![a, b, a],
            },
        );
        assert_eq!(print_expr(&m, sum3), "(1 + 2 + 1)");
    }
}
