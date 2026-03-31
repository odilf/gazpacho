use std::path::PathBuf;

use color_eyre::eyre;
use jamon::{
    graph::Graph,
    node::{contrast_node, video_source_node},
};

fn main() -> eyre::Result<()> {
    let (mut graph, video_source) = Graph::new(video_source_node());
    dbg!(&graph);
    graph.set_input(video_source.bind(0), PathBuf::from("./sample.mp4"));
    dbg!(&graph);

    let contrast = graph.insert_node(contrast_node());
    graph.set_input(contrast.bind(0), 50.0);
    graph.set_input(contrast.bind(1), video_source.bind(0));

    // graph.set_output(contrast.bind(0));

    // dbg!(&graph);

    graph.render_to("./output.mp4")?;

    Ok(())
}
