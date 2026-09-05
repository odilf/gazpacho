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

use gazpacho_datatypes::StrInterner;

use crate::ast::{Arg, BinaryOp, Def, Expr, ExprId, Literal, Module, Operator, Param, TypeExpr};

pub fn print(module: &Module, str_interner: &StrInterner) -> String {
    let mut out = String::new();
    let mut printer = Printer {
        out: &mut out,
        module,
        str_interner,
    };
    #[expect(clippy::unwrap_used, reason = "writing to a String can't fail")]
    printer.module().unwrap();
    out
}

pub fn print_expr(module: &Module, str_interner: &StrInterner, id: ExprId) -> String {
    let mut out = String::new();
    let mut printer = Printer {
        out: &mut out,
        module,
        str_interner,
    };
    #[expect(clippy::unwrap_used, reason = "writing to a String can't fail")]
    printer.expr(id, 0).unwrap();
    out
}

struct Printer<'a, W> {
    out: W,
    module: &'a Module,
    str_interner: &'a StrInterner,
}

impl<'a, W: Write> Printer<'a, W> {
    fn module(&mut self) -> fmt::Result {
        for (i, def) in self.module.defs.iter().enumerate() {
            if i > 0 {
                self.out.write_char('\n')?;
            }
            self.print_def(def)?;
        }

        if let Some(result) = self.module.value {
            self.expr(result, 0)?;
            self.out.write_char('\n')?;
        }

        Ok(())
    }

    fn print_def(&mut self, def: &Def) -> fmt::Result {
        write!(self.out, "def {}(", def.name.resolve(self.str_interner))?;
        for (i, p) in def.params.iter().enumerate() {
            if i > 0 {
                self.out.write_str(", ")?;
            }
            self.param(p)?;
        }
        self.out.write_char(')')?;
        if let Some(ret) = &def.ret {
            self.out.write_str(" -> ")?;
            self.type_expr(ret)?;
        }
        self.out.write_str(" =")?;
        if matches!(self.module.expr(def.body), Expr::Let { .. }) {
            self.out.write_char('\n')?;
            self.expr(def.body, 1)?;
        } else {
            self.out.write_char(' ')?;
            self.expr(def.body, 0)?;
        }
        self.out.write_char('\n')
    }

    fn param(&mut self, p: &Param) -> fmt::Result {
        write!(self.out, "{}", p.name.resolve(self.str_interner))?;
        if let Some(ty) = &p.ty {
            self.out.write_str(": ")?;
            self.type_expr(ty)?;
        }
        if let Some(default) = p.default {
            self.out.write_str(" = ")?;
            self.expr(default, 0)?;
        }
        Ok(())
    }

    fn type_expr(&mut self, ty: &TypeExpr) -> fmt::Result {
        let TypeExpr::Named { name, args } = ty;
        write!(self.out, "{}", name.resolve(self.str_interner))?;
        if !args.is_empty() {
            self.out.write_char('<')?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    self.out.write_str(", ")?;
                }
                self.type_expr(arg)?;
            }
            self.out.write_char('>')?;
        }
        Ok(())
    }

    fn expr(&mut self, id: ExprId, indent: usize) -> fmt::Result {
        match self.module.expr(id) {
            Expr::Lit(lit) => self.literal(lit),
            Expr::Var(name) => write!(self.out, "{}", name.resolve(self.str_interner)),
            Expr::Call { callee, args } => self.call(*callee, args, indent),
            Expr::Operator(op) => self.operator(op, indent),
            Expr::Let { bindings, body } => {
                let ind = "  ".repeat(indent);
                for (name, value) in bindings {
                    write!(self.out, "{ind}let {} = ", name.resolve(self.str_interner))?;
                    self.expr(*value, indent)?;
                    self.out.write_char('\n')?;
                }
                self.out.write_str(&ind)?;
                self.expr(*body, indent)
            }
            Expr::Lambda { params, body } => {
                self.out.write_char('(')?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.out.write_str(", ")?;
                    }
                    self.param(p)?;
                }
                self.out.write_str(" -> ")?;
                self.expr(*body, indent)?;
                self.out.write_char(')')
            }
            Expr::List(items) => {
                self.out.write_char('[')?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.write_str(", ")?;
                    }
                    self.expr(*item, indent)?;
                }
                self.out.write_char(']')
            }
            Expr::Record(fields) => {
                self.out.write_str("{ ")?;
                for (i, (name, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.out.write_str(", ")?;
                    }
                    write!(self.out, "{}: ", name.resolve(self.str_interner))?;
                    self.expr(*value, indent)?;
                }
                self.out.write_str(" }")
            }
            Expr::Field { base, field } => {
                let parens = !matches!(
                    self.module.expr(*base),
                    Expr::Var(_) | Expr::Call { .. } | Expr::Field { .. }
                );
                if parens {
                    self.out.write_char('(')?;
                }
                self.expr(*base, indent)?;
                if parens {
                    self.out.write_char(')')?;
                }
                write!(self.out, ".{}", field.resolve(self.str_interner))
            }
            Expr::FieldAccessor { field } => {
                write!(self.out, ".{}", field.resolve(self.str_interner))
            }
            Expr::Wgsl { source, .. } => write!(self.out, "wgsl {{{source}}}"),
            Expr::Script { lang, source, .. } => {
                write!(
                    self.out,
                    "script \"{}\" {{{source}}}",
                    lang.resolve(self.str_interner)
                )
            }
            Expr::Error => self.out.write_str("<error>"),
        }
    }

    fn call(&mut self, callee: ExprId, args: &[Arg], indent: usize) -> fmt::Result {
        let parens = !matches!(self.module.expr(callee), Expr::Var(_) | Expr::Field { .. });
        if parens {
            self.out.write_char('(')?;
        }
        self.expr(callee, indent)?;
        if parens {
            self.out.write_char(')')?;
        }
        self.out.write_char('(')?;
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.out.write_str(", ")?;
            }
            if let Some(name) = &arg.name {
                write!(self.out, "{} = ", name.resolve(self.str_interner))?;
            }
            self.expr(arg.value, indent)?;
        }
        self.out.write_char(')')
    }

    /// Re-sugars an operator node. Operands are parenthesized unconditionally (the
    /// whole operator is wrapped) so precedence can never be misprinted.
    fn operator(&mut self, op: &Operator, indent: usize) -> fmt::Result {
        match op {
            Operator::Unary { op, operand } => {
                write!(self.out, "({}", op.symbol())?;
                self.expr(*operand, indent)?;
                self.out.write_char(')')
            }
            // `..` binds its operands directly (`a..b`); other binary operators are
            // spaced (`a + b`).
            Operator::Binary {
                op: BinaryOp::Range,
                lhs,
                rhs,
            } => {
                self.out.write_char('(')?;
                self.expr(*lhs, indent)?;
                self.out.write_str("..")?;
                self.expr(*rhs, indent)?;
                self.out.write_char(')')
            }
            Operator::Binary { op, lhs, rhs } => {
                self.out.write_char('(')?;
                self.expr(*lhs, indent)?;
                write!(self.out, " {} ", op.symbol())?;
                self.expr(*rhs, indent)?;
                self.out.write_char(')')
            }
            Operator::Variadic { op, operands } => {
                self.out.write_char('(')?;
                for (i, operand) in operands.iter().enumerate() {
                    if i > 0 {
                        write!(self.out, " {} ", op.symbol())?;
                    }
                    self.expr(*operand, indent)?;
                }
                self.out.write_char(')')
            }
        }
    }

    fn literal(&mut self, lit: &Literal) -> fmt::Result {
        match lit {
            Literal::Int(v) => write!(self.out, "{v}"),
            Literal::Float(v) => write!(self.out, "{v}"),
            Literal::Bool(v) => write!(self.out, "{v}"),
            Literal::Str(v) => write!(
                self.out,
                "\"{}\"",
                self.escape(self.str_interner.resolve(*v))
            ),
            Literal::Time(v) => write!(self.out, "{v}"),
        }
    }

    // TODO: This can (should?) be `Cow`.
    fn escape(&self, s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    }
}

#[cfg(test)]
mod tests {
    use gazpacho_datatypes::StrInterner;

    use crate::parse::parse;

    use super::print;

    /// `print` then `parse` must be idempotent: printing a parsed module and
    /// reparsing it yields the same printed text (note that `parse` then
    /// `print` not necessarilly, because of formatting decisions).
    // TODO(fixtures): Run test on all example files.
    #[test]
    fn roundtrip_is_idempotent() {
        let src = r#"
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
        let mut interner = StrInterner::new();
        let (module, errors) = parse(src, &mut interner);
        assert!(errors.is_empty(), "parse errors: {errors:?}");

        let once = print(&module, &interner);
        let mut interner = StrInterner::new();
        let (module2, errors2) = parse(&once, &mut interner);
        assert!(
            errors2.is_empty(),
            "reparse errors on:\n{once}\n{errors2:?}"
        );
        let twice = print(&module2, &interner);
        assert_eq!(once, twice);
    }

    #[test]
    fn roundtrip_operators_and_literals() {
        let src = "def x = -(1 + 2) * 3.5 / len(a)\ndef t = 250ms\ndef b = x < 3 != true\ndef r = { at: 2s, name: \"a\\\"b\" }\n";

        let mut interner = StrInterner::new();
        let (module, errors) = parse(src, &mut interner);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let once = print(&module, &interner);

        let mut interner = StrInterner::new();
        let (module2, errors2) = parse(&once, &mut interner);
        assert!(
            errors2.is_empty(),
            "reparse errors on:\n{once}\n{errors2:?}"
        );
        assert_eq!(once, print(&module2, &interner));
    }
}

#[cfg(test)]
mod resugar_tests {
    use gazpacho_datatypes::StrInterner;

    use super::print_expr;
    use crate::ast::*;

    fn lit(m: &mut Module, n: i64) -> ExprId {
        m.alloc(Expr::Lit(Literal::Int(n.into())), Span::new(0, 0))
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
        assert_eq!(print_expr(&m, &StrInterner::new(), sum), "(1 + 2)");

        let neg = op(
            &mut m,
            Operator::Unary {
                op: UnaryOp::Neg,
                operand: a,
            },
        );
        assert_eq!(print_expr(&m, &StrInterner::new(), neg), "(-1)");

        let range = op(
            &mut m,
            Operator::Binary {
                op: BinaryOp::Range,
                lhs: a,
                rhs: b,
            },
        );
        assert_eq!(print_expr(&m, &StrInterner::new(), range), "(1..2)");

        // A variadic node renders every operand, however many there are.
        let sum3 = op(
            &mut m,
            Operator::Variadic {
                op: VariadicOp::Sum,
                operands: vec![a, b, a],
            },
        );
        assert_eq!(print_expr(&m, &StrInterner::new(), sum3), "(1 + 2 + 1)");
    }
}
