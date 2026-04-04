use color_eyre::eyre;
use gazpacho_core::{graph::Graph, node::CONTRAST};

fn main() -> eyre::Result<()> {
    let mut graph = Graph::new();

    let video_source = graph.insert_node(&CONTRAST);
    let video_source = graph.get(video_source).io();
    graph.set_const_input(video_source.port("path"), "./sample.mp4".to_string());

    graph.render_video(video_source.port("output"), "./output.mp4")?;

    Ok(())
}
