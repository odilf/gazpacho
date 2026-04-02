use color_eyre::eyre;
use jamon_core::{
    graph::Graph,
    node::{contrast_node, video_source_node},
};

fn main() -> eyre::Result<()> {
    let mut graph = Graph::new();

    let video_source = graph.insert_node(video_source_node());
    graph.set_output(video_source.port(0));
    graph.set_input(video_source.port(0), String::from("./sample.mp4"));

    let contrast = graph.insert_node(contrast_node());
    // graph.set_input(contrast.port(0), 50.0);
    // graph.set_input(contrast.port(1), video_source.port(0));
    // graph.set_output(contrast.bind(0));

    graph.render_to("./output.mp4")?;

    Ok(())
}
