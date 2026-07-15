//! Integration tests for the AST: parsing (structure, sugar, spans,
//! recovery), and printing (round-trips, fragments).

use gazpacho_ir::ast::Span;
use gazpacho_ir::{Expr, Literal, Module, Name, parse, print, print_expr};
use num_rational::Rational64;

// --- helpers ----------------------------------------------------------------

#[track_caller]
fn parse_ok(src: &str) -> Module {
    let (module, errors) = parse(src);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    module
}

#[track_caller]
fn body_of<'m>(module: &'m Module, name: &str) -> &'m Expr {
    module.expr(module.def(name).expect("binding not found").body)
}

#[track_caller]
fn result_of(module: &Module) -> &Expr {
    module.expr(module.value.expect("module should have a result"))
}

/// Asserts the expression is a call to a variable named `name` and returns
/// its positional argument expressions.
#[track_caller]
fn call_to<'m>(module: &'m Module, expr: &Expr, name: &str) -> Vec<&'m Expr> {
    let Expr::Call { callee, args } = expr else {
        panic!("expected call to `{name}`, got {expr:?}");
    };
    assert_eq!(
        module.expr(callee.expr().unwrap()),
        &Expr::Var(Name::from(name)),
        "expected callee `{name}`"
    );
    args.iter().map(|a| module.expr(a.value)).collect()
}

// --- parser: structure and sugar ---------------------------------------------

#[test]
fn precedence_mul_binds_tighter_than_add() {
    let module = parse_ok("def x = 1 + 2 * 3\n");
    let args = call_to(&module, body_of(&module, "x"), "+");
    assert_eq!(args[0], &Expr::Lit(Literal::Int(1)));
    let rhs = call_to(&module, args[1], "*");
    assert_eq!(rhs[0], &Expr::Lit(Literal::Int(2)));
    assert_eq!(rhs[1], &Expr::Lit(Literal::Int(3)));
}

#[test]
fn parens_override_precedence() {
    let module = parse_ok("def x = (1 + 2) * 3\n");
    let args = call_to(&module, body_of(&module, "x"), "*");
    call_to(&module, args[0], "+");
}

#[test]
fn unary_minus_desugars_to_neg() {
    let module = parse_ok("def x = -2s\n");
    let args = call_to(&module, body_of(&module, "x"), "neg");
    assert_eq!(
        args[0],
        &Expr::Lit(Literal::Time(Rational64::from_integer(2)))
    );
}

#[test]
fn pipeline_chains_left_associatively() {
    // a |> f |> g(b)  ==>  g(f(a), b)
    let module = parse_ok("def x = a |> f |> g(b)\n");
    let g_args = call_to(&module, body_of(&module, "x"), "g");
    assert_eq!(g_args.len(), 2);
    let f_args = call_to(&module, g_args[0], "f");
    assert_eq!(f_args[0], &Expr::Var(Name::from("a")));
    assert_eq!(g_args[1], &Expr::Var(Name::from("b")));
}

#[test]
fn bare_pipe_is_an_error() {
    // Only `|>` is a pipeline; `|` is reserved.
    let (_, errors) = parse("def x = a | f(b)\n");
    assert!(!errors.is_empty());
}

#[test]
fn field_access_chains() {
    let module = parse_ok("def x = clip.extent.start\n");
    let Expr::Field { base, field } = body_of(&module, "x") else {
        panic!("expected field access");
    };
    assert_eq!(field, &Name::from("start"));
    assert!(matches!(module.expr(*base), Expr::Field { .. }));
}

#[test]
fn mixed_positional_and_named_args() {
    let module = parse_ok("def x = f(a, size: b, c)\n");
    let Expr::Call { args, .. } = body_of(&module, "x") else {
        panic!("expected call");
    };
    assert_eq!(args[0].name, None);
    assert_eq!(args[1].name, Some(Name::from("size")));
    assert_eq!(args[2].name, None);
}

#[test]
fn trailing_commas_everywhere() {
    parse_ok("def f(a, b,) = g(a, b,)\ndef x = [1, 2,]\ndef r = { a: 1, b: 2, }\n");
}

#[test]
fn semicolons_separate_items() {
    let module = parse_ok("def x = 1; def y = 2\n");
    assert!(module.def("x").is_some());
    assert!(module.def("y").is_some());
}

#[test]
fn multiline_expressions_inside_brackets() {
    let module = parse_ok("stack([\n  a,\n  b,\n])\n");
    let args = call_to(&module, result_of(&module), "stack");
    assert!(matches!(args[0], Expr::List(items) if items.len() == 2));
}

#[test]
fn last_expression_is_the_module_result() {
    let module = parse_ok("def a = 1\nf(a)\n");
    call_to(&module, result_of(&module), "f");
}

#[test]
fn second_result_expression_is_an_error() {
    let (module, errors) = parse("f(a)\ng(b)\n");
    assert!(!errors.is_empty());
    // The first expression stays the result.
    call_to(&module, result_of(&module), "f");
}

#[test]
fn module_without_result_is_valid() {
    let module = parse_ok("def f(x) = x\ndef a = 1\n");
    assert!(module.value.is_none());
}

#[test]
fn imports_are_recorded() {
    let module = parse_ok("import \"lib/grades.gazpacho\" as grades\ndef x = grades.warm\n");
    assert_eq!(module.imports.len(), 1);
    assert_eq!(module.imports[0].path, "lib/grades.gazpacho");
    assert_eq!(module.imports[0].alias, Name::from("grades"));
}

#[test]
fn lambda_with_expression_body() {
    let module = parse_ok("def f = map(xs, p -> trim(p, 1s..2s))\n");
    let Expr::Call { args, .. } = body_of(&module, "f") else {
        panic!("expected call");
    };
    let Expr::Lambda { params, body } = module.expr(args[1].value) else {
        panic!("expected lambda");
    };
    assert_eq!(params[0].name, Name::from("p"));
    call_to(&module, module.expr(*body), "trim");
}

// --- parser: spans ------------------------------------------------------------

#[test]
fn spans_point_at_source_text() {
    let src = "def x = load(\"a.mp4\")\n";
    let module = parse_ok(src);
    let body = module.def("x").unwrap().body;
    let Span { start, end } = module.span(body);
    assert_eq!(&src[start as usize..end as usize], "load(\"a.mp4\")");
}

#[test]
fn literal_spans_are_exact() {
    let src = "def t = 250ms\n";
    let module = parse_ok(src);
    let body = module.def("t").unwrap().body;
    let Span { start, end } = module.span(body);
    assert_eq!(&src[start as usize..end as usize], "250ms");
}

// --- parser: error recovery ----------------------------------------------------

#[test]
fn recovery_keeps_later_items() {
    let (module, errors) = parse("def a = )))\ndef b = 1\ndef f(x) = x\n");
    assert!(!errors.is_empty());
    assert!(module.def("b").is_some());
    assert!(module.def("f").is_some());
}

#[test]
fn recovery_produces_error_nodes_not_panics() {
    for src in [
        "let x = \nlet y = 1\n",
        "let x = f(\n",
        "let x = [1,\n",
        "def f( = 1\n",
        "import 3 as x\n",
        "let x = { a 1 }\n",
        "..\n",
    ] {
        let (_, errors) = parse(src);
        assert!(!errors.is_empty(), "expected errors for {src:?}");
    }
}

// --- printer -------------------------------------------------------------------

#[test]
fn print_expr_renders_fragments() {
    let module = parse_ok("def x = trim(clip, 2s..2500ms)\n");
    let body = module.def("x").unwrap().body;
    assert_eq!(print_expr(&module, body), "trim(clip, (2s..2500ms))");
}

#[test]
fn print_time_fallback_is_exact_but_structural() {
    // A rational that is not representable as ms (1/3 s) can only be built
    // programmatically; printing falls back to an exact division expression.
    let mut module = Module::empty();
    let id = module.alloc(
        Expr::Lit(Literal::Time(Rational64::new(1, 3))),
        Span::new(0, 0),
    );
    assert_eq!(print_expr(&module, id), "(1s / 3)");
}

#[test]
fn roundtrip_small_snippets() {
    let snippets = [
        "def x = 1\n",
        "def x = -1.5\n",
        "def x = true\n",
        "def x = \"a\\nb\"\n",
        "def x = a + b * c - d / e\n",
        "def x = a < b == c\n",
        "def x = [\n]\n",
        "def x = f()\n",
        "def x = f(a)(b)\n",
        "def x = xs |> map(.at)\n",
        "def x = { at: 2s, name: \"n\" }\n",
        "def id(x) = x\n",
        "def f(x: Int = 3) -> Int = x + 1\n",
        "def g(xs: List<Clip<Frame>>) = head(xs)\n",
    ];
    for src in snippets {
        let (module, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors for {src:?}: {errors:?}");
        let once = print(&module);
        let (module2, errors2) = parse(&once);
        assert!(
            errors2.is_empty(),
            "reparse errors for {src:?} printed as {once:?}: {errors2:?}"
        );
        assert_eq!(once, print(&module2), "not idempotent for {src:?}");
    }
}
