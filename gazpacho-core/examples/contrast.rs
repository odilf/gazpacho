use color_eyre::eyre;
use gazpacho_core::{
    graph::{Graph, ImmutableGraph as _},
    node::{basic::VIDEO_SOURCE, color::CONTRAST},
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

    let contrast = graph.insert_node(&CONTRAST);
    let contrast = graph.get(contrast).io();
    graph.set_const_input(contrast.port("factor"), 2.5);
    graph.connect(video_source.port("output"), contrast.port("frame"));

    graph.render_as_video(
        contrast.port("output"),
        video_source.port("len"),
        video_source.port("fps"),
        "./output.mp4",
    )?;

    Ok(())
}
