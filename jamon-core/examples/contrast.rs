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

    let contrast = graph.insert_node(contrast_node());
    let contrast = graph.get_io_ref(contrast);
    graph.set_input(contrast["amount"], 50.0);
    graph.set_input(contrast["frame"], video_source["output"]);
    graph.set_output(contrast["output"]);

    graph.render_to("./output.mp4")?;

    Ok(())
}
