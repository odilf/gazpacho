//! Round-trip tests for serializing the app state.

use gazpacho_ast::parse;
use gazpacho_compile::compile;
use gazpacho_ui::state::ProjectState;

/// Serialize to JSON and back.
fn round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&json).expect("deserialize")
}

#[test]
fn project_state_round_trips() {
    let src = "load(\"hello.mp4\")\n";
    let (module, errors) = parse(src);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");

    let (graph, output) = compile(&module).expect("compile");

    let state = ProjectState {
        root: None,
        active_gzp: Some("/tmp/x.gzp".into()),
        source_text: Some(src.to_string()),
        module: Some(module),
        render_graph: Some((graph, output)),
        renderer: None,
        parse_errors: Vec::new(),
        compile_error: None,
        diagnostics: Vec::new(),
        media_items: Vec::new(),
    };

    let restored = round_trip(&state);

    // The module survives the round trip, including its interned strings.
    let original_module = state.module.as_ref().expect("module");
    let restored_module = restored.module.as_ref().expect("module");
    assert_eq!(
        gazpacho_ast::print(original_module),
        gazpacho_ast::print(restored_module)
    );

    // The render graph survives: same output node, same node contents.
    let (orig_graph, orig_output) = state.render_graph.as_ref().expect("graph");
    let (restored_graph, restored_output) = restored.render_graph.as_ref().expect("graph");
    assert_eq!(orig_output, restored_output);
    assert_eq!(
        orig_graph.get(*orig_output).op(),
        restored_graph.get(*restored_output).op(),
        "node op should survive the round trip"
    );
}

#[test]
fn app_round_trips() {
    let src = "load(\"hello.mp4\")\n";
    let (module, errors) = parse(src);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    let (graph, output) = compile(&module).expect("compile");

    let app = gazpacho_ui::App {
        recent_project_roots: vec!["/tmp/projects".into()],
        layout: gazpacho_ui::state::LayoutState {
            left_sidebar_width: 200.0,
            ..Default::default()
        },
        focused_pane: gazpacho_ui::command::Pane::Inspector,
        selection: Some(gazpacho_ui::state::Selection::MediaItem(3)),
        project: ProjectState {
            root: None,
            active_gzp: Some("/tmp/x.gzp".into()),
            source_text: Some(src.to_string()),
            module: Some(module),
            render_graph: Some((graph, output)),
            renderer: None,
            parse_errors: Vec::new(),
            compile_error: None,
            diagnostics: Vec::new(),
            media_items: Vec::new(),
        },
        media_items: vec![gazpacho_ui::state::MediaItem {
            path: "hello.mp4".into(),
            resolution: None,
            duration: None,
            fps: None,
            available: true,
            error: None,
        }],
        command_palette: Default::default(),
    };

    let restored = round_trip(&app);

    assert_eq!(restored.recent_project_roots, app.recent_project_roots);
    assert_eq!(restored.focused_pane, app.focused_pane);
    assert_eq!(restored.selection, app.selection);
    assert_eq!(
        restored.project.active_gzp,
        app.project.active_gzp
    );
    assert_eq!(
        gazpacho_ast::print(restored.project.module.as_ref().expect("module")),
        gazpacho_ast::print(app.project.module.as_ref().expect("module"))
    );
    assert_eq!(restored.media_items[0].path, app.media_items[0].path);
    // The renderer is runtime state and must not be persisted.
    assert!(restored.project.renderer.is_none());
}