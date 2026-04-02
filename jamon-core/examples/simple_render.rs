use color_eyre::eyre;
use jamon_core::{
    graph::Graph,
    node::{contrast_node, video_source_node},
};

fn main() -> eyre::Result<()> {
    let mut graph = Graph::new();

    let video_source = graph.insert_node(video_source_node());
    let video_source = graph.get_io_ref(video_source);

    graph.set_output(video_source["output"]);
    graph.set_input(video_source["path"], String::from("./sample.mp4"));

    graph.render_to("./output.mp4")?;

    Ok(())
}
