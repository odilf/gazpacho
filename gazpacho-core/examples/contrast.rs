use color_eyre::eyre;
use gazpacho_core::{
    graph::Graph,
    node::{basic::VIDEO_SOURCE, color::CONTRAST},
};

fn main() -> eyre::Result<()> {
    let mut graph = Graph::new();

    let video_source = graph.insert_node(&VIDEO_SOURCE);
    let video_source = graph.get(video_source).io();
    graph.set_const_input(video_source.port("path"), String::from("./sample.mp4"));

    let contrast = graph.insert_node(&CONTRAST);
    let contrast = graph.get(contrast).io();
    graph.set_const_input(contrast.port("amount"), 50.0);
    graph.connect(contrast.port("frame"), video_source.port("output"));

    graph.render_video(contrast.port("output"), "./output.mp4")?;

    Ok(())
}
