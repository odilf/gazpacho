use color_eyre::eyre;
use gazpacho_core::{graph::Graph, node::basic::VIDEO_SOURCE};
use tracing_subscriber::fmt::format::FmtSpan;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_span_events(FmtSpan::ACTIVE)
        .with_line_number(true)
        .init();

    let mut graph = Graph::new();

    let video_source = graph.insert_node(&VIDEO_SOURCE);
    let video_source = graph.get(video_source).io();
    graph.set_const_input(video_source.port("path"), "./sample.mp4".to_string());

    graph.render_video(video_source.port("video_source"), "./output.mp4")?;

    Ok(())
}
