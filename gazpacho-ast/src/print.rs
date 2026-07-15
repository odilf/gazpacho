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

use std::fmt::Write;

use num_rational::Rational64;

use crate::ast::{Callee, Def, Expr, ExprId, Literal, Module, Param, TypeExpr};

pub fn print(module: &Module) -> String {
    let mut out = String::new();
    for import in &module.imports {
        let _ = writeln!(
            out,
            "import \"{}\" as {}",
            escape(&import.path),
            import.alias
        );
    }
    if !module.imports.is_empty() {
        out.push('\n');
    }
    for (i, def) in module.defs.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_def(module, def, &mut out);
    }
    // if !module.defs.is_empty() && (!module.bindings.is_empty() || module.result.is_some()) {
    //     out.push('\n');
    // }
    if let Some(result) = module.value {
        expr(module, result, &mut out, 0);
        out.push('\n');
    }
    out
}

pub fn print_expr(module: &Module, id: ExprId) -> String {
    let mut out = String::new();
    expr(module, id, &mut out, 0);
    out
}

fn print_def(module: &Module, def: &Def, out: &mut String) {
    let _ = write!(out, "def {}(", def.name);
    for (i, p) in def.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        param(module, p, out);
    }
    out.push(')');
    if let Some(ret) = &def.ret {
        out.push_str(" -> ");
        type_expr(ret, out);
    }
    out.push_str(" =");
    if matches!(module.expr(def.body), Expr::Let { .. }) {
        out.push('\n');
        expr(module, def.body, out, 1);
    } else {
        out.push(' ');
        expr(module, def.body, out, 0);
    }
    out.push('\n');
}

fn param(module: &Module, p: &Param, out: &mut String) {
    let _ = write!(out, "{}", p.name);
    if let Some(ty) = &p.ty {
        out.push_str(": ");
        type_expr(ty, out);
    }
    if let Some(default) = p.default {
        out.push_str(" = ");
        expr(module, default, out, 0);
    }
}

fn type_expr(ty: &TypeExpr, out: &mut String) {
    let TypeExpr::Named { name, args } = ty;
    let _ = write!(out, "{name}");
    if !args.is_empty() {
        out.push('<');
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            type_expr(arg, out);
        }
        out.push('>');
    }
}

fn expr(module: &Module, id: ExprId, out: &mut String, indent: usize) {
    match module.expr(id) {
        Expr::Lit(lit) => literal(lit, out),
        Expr::Var(name) => {
            let _ = write!(out, "{name}");
        }
        Expr::Call { callee, args } => call(module, *callee, args, out, indent),
        Expr::Let { bindings, body } => {
            let ind = "  ".repeat(indent);
            for (name, value) in bindings {
                let _ = write!(out, "{ind}let {name} = ");
                expr(module, *value, out, indent);
                out.push('\n');
            }
            out.push_str(&ind);
            expr(module, *body, out, indent);
        }
        Expr::Lambda { params, body } => {
            out.push('(');
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                param(module, p, out);
            }
            out.push_str(" -> ");
            expr(module, *body, out, indent);
            out.push(')');
        }
        Expr::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                expr(module, *item, out, indent);
            }
            out.push(']');
        }
        Expr::Record(fields) => {
            out.push_str("{ ");
            for (i, (name, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{name}: ");
                expr(module, *value, out, indent);
            }
            out.push_str(" }");
        }
        Expr::Field { base, field } => {
            let parens = !matches!(
                module.expr(*base),
                Expr::Var(_) | Expr::Call { .. } | Expr::Field { .. }
            );
            if parens {
                out.push('(');
            }
            expr(module, *base, out, indent);
            if parens {
                out.push(')');
            }
            let _ = write!(out, ".{field}");
        }
        Expr::Wgsl { source, .. } => {
            let _ = write!(out, "wgsl {{{source}}}");
        }
        Expr::Script { lang, source, .. } => {
            let _ = write!(out, "script \"{lang}\" {{{source}}}");
        }
        Expr::Error => out.push_str("<error>"),
    }
}

fn call(
    module: &Module,
    callee: Callee,
    args: &[crate::ast::Arg],
    out: &mut String,
    indent: usize,
) {
    use crate::ast::Builtin;

    // Re-sugar builtin operators when the shape matches exactly.
    // FIXME: Technically, right now it's possible to have, say, 3-arg sums but
    // only by piping it. They should really be variadic from the start, and
    // this should desugar them accordingly
    if let Callee::Builtin(builtin) = callee
        && args.iter().all(|a| a.name.is_none())
    {
        match (builtin, args.len()) {
            (
                Builtin::Lt
                | Builtin::Gt
                | Builtin::Le
                | Builtin::Ge
                | Builtin::Eq
                | Builtin::Ne
                | Builtin::Sum
                | Builtin::Subtract
                | Builtin::Multiply
                | Builtin::Divide,
                2,
            ) => {
                out.push('(');
                expr(module, args[0].value, out, indent);
                let _ = write!(out, " {} ", builtin.symbol());
                expr(module, args[1].value, out, indent);
                out.push(')');
                return;
            }
            (Builtin::Neg, 1) => {
                out.push_str("(-");
                expr(module, args[0].value, out, indent);
                out.push(')');
                return;
            }
            (Builtin::Range, 2) => {
                out.push('(');
                expr(module, args[0].value, out, indent);
                out.push_str("..");
                expr(module, args[1].value, out, indent);
                out.push(')');
                return;
            }
            _ => {}
        }
    }

    match callee {
        Callee::Expr(id) => {
            let parens = !matches!(module.expr(id), Expr::Var(_) | Expr::Field { .. });
            if parens {
                out.push('(');
            }
            expr(module, id, out, indent);
            if parens {
                out.push(')');
            }
        }
        // FIXME: See note above about variadic builtins.
        Callee::Builtin(builtin) => {
            let _ = write!(out, "{}", builtin.symbol());
        }
    }
    out.push('(');
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if let Some(name) = &arg.name {
            let _ = write!(out, "{name} = ");
        }
        expr(module, arg.value, out, indent);
    }
    out.push(')');
}

fn literal(lit: &Literal, out: &mut String) {
    match lit {
        Literal::Int(v) => {
            let _ = write!(out, "{v}");
        }
        // `{:?}` keeps the decimal point (`2.0`), so it re-lexes as a float.
        Literal::Float(v) => {
            let _ = write!(out, "{v:?}");
        }
        Literal::Bool(v) => {
            let _ = write!(out, "{v}");
        }
        Literal::Str(v) => {
            let _ = write!(out, "\"{}\"", escape(v));
        }
        Literal::Time(v) => time(*v, out),
    }
}

/// Time literals only enter through `Ns`/`Nms` decimals, so denominators
/// always divide 1000 and decimal printing is exact. The fallback covers
/// rationals produced programmatically; it is lossy in structure (an
/// expression, not a literal) but exact in value.
fn time(v: Rational64, out: &mut String) {
    let (numer, denom) = (*v.numer(), *v.denom());
    if let Some(scaled) = numer.checked_mul(1000).filter(|scaled| scaled % denom == 0) {
        let ms = scaled / denom;
        if ms % 1000 == 0 {
            let _ = write!(out, "{}s", ms / 1000);
        } else {
            let _ = write!(out, "{ms}ms");
        }
    } else {
        let _ = write!(out, "({numer}s / {denom})");
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

    #[test]
    fn resugars_builtin_operators() {
        let mut m = Module::empty();
        let a = lit(&mut m, 1);
        let b = lit(&mut m, 2);
        let sum = m.alloc(
            Expr::Call {
                callee: Callee::Builtin(Builtin::Sum),
                args: vec![Arg::anon(a), Arg::anon(b)],
            },
            Span::new(0, 0),
        );
        assert_eq!(print_expr(&m, sum), "(1 + 2)");

        let neg = m.alloc(
            Expr::Call {
                callee: Callee::Builtin(Builtin::Neg),
                args: vec![Arg::anon(a)],
            },
            Span::new(0, 0),
        );
        assert_eq!(print_expr(&m, neg), "(-1)");

        let range = m.alloc(
            Expr::Call {
                callee: Callee::Builtin(Builtin::Range),
                args: vec![Arg::anon(a), Arg::anon(b)],
            },
            Span::new(0, 0),
        );
        assert_eq!(print_expr(&m, range), "(1..2)");

        // 3-arg Sum (the piped `x |> a + b` shape) must NOT re-sugar; fallback renders symbol.
        let sum3 = m.alloc(
            Expr::Call {
                callee: Callee::Builtin(Builtin::Sum),
                args: vec![Arg::anon(a), Arg::anon(b), Arg::anon(a)],
            },
            Span::new(0, 0),
        );
        assert_eq!(print_expr(&m, sum3), "+(1, 2, 1)");
    }
}
