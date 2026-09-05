use std::{env, fs, path::Path};

use gazpacho_ast::parse;
use gazpacho_compile::compile;
use gazpacho_datatypes::StrInterner;
use gazpacho_media::read::ResolutionRequest;
use gazpacho_render::Engine;
use tracing_subscriber::EnvFilter;

#[test]
pub fn render_examples() -> eyre::Result<()> {
    // let subscriber = Registry::default().with(HierarchicalLayer::new(2));
    // tracing::subscriber::set_global_default(subscriber).unwrap();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .with_line_number(true)
        .init();
    env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/"))?;

    let mut count = 0;
    // TODO: Copied from AST, should be more structured
    for file in fs::read_dir(".")? {
        let file = file?;
        if file.path().extension().and_then(|ext| ext.to_str()) != Some("gzp") {
            continue;
        }

        count += 1;
        tracing::info!(file = ?file.path().file_name());

        let program = fs::read_to_string(&file.path())?.replace("sample.mp4", "sample-short.mp4");
        let mut str_interner = StrInterner::new();
        let (module, errors) = parse(&program, &mut str_interner);
        assert!(
            errors.is_empty(),
            "{}, {:?}",
            &file.file_name().to_string_lossy(),
            errors
        );

        let (graph, output) = compile(&module, &str_interner)?;

        fs::create_dir_all("../target/renders/")?;
        let mut renderer = Engine::new(graph, output, str_interner);
        let fps = renderer.output_fps()?.unwrap();
        renderer.render_video(
            &format!(
                "../target/renders/{}.mp4",
                file.file_name().to_str().unwrap()
            ),
            fps,
            ResolutionRequest::auto(),
        )?;
    }

    assert_ne!(count, 0, "No files were found");

    Ok(())
}
