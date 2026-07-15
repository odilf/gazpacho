use color_eyre::eyre;
use gazpacho_core::{
    graph::{Graph, ImmutableGraph as _},
    node::{basic::CONCAT, basic::VIDEO_SOURCE},
};
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
    graph.set_const_input(video_source.port("path"), String::from("./sample.mp4"));

    let concat = graph.insert_node(&CONCAT);
    let concat = graph.get(concat).io();
    graph.connect(video_source.port("output"), concat.port("frame-a"));
    graph.connect(video_source.port("len"), concat.port("len-a"));
    graph.connect(video_source.port("output"), concat.port("frame-b"));
    graph.connect(video_source.port("len"), concat.port("len-b"));

    graph.render_as_video(
        concat.port("output"),
        concat.port("len"),
        video_source.port("fps"),
        "./output.mp4",
    )?;

    Ok(())
}
