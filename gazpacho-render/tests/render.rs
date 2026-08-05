use std::{env, fs, path::Path};

use gazpacho_ast::parse;
use gazpacho_compile::compile;
use gazpacho_render::render;
use tracing_subscriber::EnvFilter;

#[test]
pub fn render_examples() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/"))?;

    let mut count = 0;
    // TODO: Copied from AST, should be more structured
    for file in fs::read_dir(".")? {
        let file = file?;
        if file.path().extension().and_then(|ext| ext.to_str()) != Some("gazpacho") {
            continue;
        }

        count += 1;
        tracing::info!(file = ?file.path().file_name());

        let program = fs::read_to_string(&file.path())?.replace("sample.mp4", "sample-short.mp4");
        let (module, errors) = parse(&program);
        assert!(
            errors.is_empty(),
            "{}, {:?}",
            &file.file_name().to_string_lossy(),
            errors
        );

        let (graph, output) = compile(&module)?;

        fs::create_dir_all("../target/renders/")?;
        render(
            graph,
            output,
            &format!(
                "../target/renders/{}.mp4",
                file.file_name().to_str().unwrap()
            ),
        )?;
    }

    assert_ne!(count, 0, "No files were found");

    Ok(())
}
