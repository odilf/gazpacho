use std::path::PathBuf;

use color_eyre::eyre;
use jamon::{Graph, NodeData};

fn main() -> eyre::Result<()> {
    let mut graph = Graph::new();

    let path = graph.insert(NodeData::Path("./sample.mp4".into()));
    let video = graph.insert(NodeData::VideoSource);

    graph.set_inputs(video, vec![path, graph.frame_index_input()?]);
    graph.set_inputs(graph.video_output()?, vec![video]);

    graph.render_video_to("./output.mp4")?;

    Ok(())
}
