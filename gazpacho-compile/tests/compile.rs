#![expect(
    clippy::panic_in_result_fn,
    reason = "Assertions are the point of tests."
)]

use gazpacho_ast::parse;
use gazpacho_compile::compile;
use gazpacho_datatypes::StrInterner;

const SOURCE: &str = "load(\"./sample.mp4\") |> contrast(1.5)";

fn compile_source(strings: &mut StrInterner) -> eyre::Result<gazpacho_graph::NodeId> {
    let (module, errors) = parse(SOURCE, strings);
    assert!(errors.is_empty(), "{errors:?}");
    let (_graph, output) = compile(&module, strings)?;
    Ok(output)
}

#[test]
fn output_id_stable_across_interners() -> eyre::Result<()> {
    // The dummy string shifts every symbol index in the first interner, so
    // ids derived from symbol numbers would differ.
    let mut strings_a = StrInterner::new();
    strings_a.get_or_intern("dummy");
    let output_a = compile_source(&mut strings_a)?;

    let mut strings_b = StrInterner::new();
    let output_b = compile_source(&mut strings_b)?;

    assert_eq!(output_a, output_b);
    Ok(())
}

#[test]
fn compile_is_deterministic() -> eyre::Result<()> {
    let mut strings_a = StrInterner::new();
    let mut strings_b = StrInterner::new();
    assert_eq!(compile_source(&mut strings_a)?, compile_source(&mut strings_b)?);
    Ok(())
}
