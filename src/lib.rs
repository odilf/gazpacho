use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, ContextCompat};

pub struct Graph {
    inputs: Vec<NodeRef>,
    outputs: Vec<NodeRef>,
    static_outputs: Vec<NodeRef>,
    nodes: Vec<Node>,
}

impl Graph {
    pub fn empty() -> Self {
        Self {
            inputs: Vec::new(),
            outputs: Vec::new(),
            static_outputs: Vec::new(),
            nodes: Vec::new(),
        }
    }

    pub fn new() -> Self {
        let mut graph = Self::empty();
        let frame_index = graph.insert(NodeData::Index(0));
        let video_output = graph.insert(NodeData::Frame);

        graph.inputs = vec![frame_index];
        graph.outputs = vec![video_output];

        graph
    }

    pub fn insert(&mut self, node: NodeData) -> NodeRef {
        let r = NodeRef(self.nodes.len());
        self.nodes.push(Node {
            data: node,
            inputs: Vec::new(),
            outputs: Vec::new(),
            id: r,
        });

        r
    }

    fn get(&mut self, node: NodeRef) -> &Node {
        &self.nodes[node.0]
    }

    fn get_mut(&mut self, node: NodeRef) -> &mut Node {
        &mut self.nodes[node.0]
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Node> {
        self.inputs.iter().map(|&ni| &self.nodes[ni.0])
    }

    pub fn set_inputs(&mut self, node: NodeRef, inputs: Vec<NodeRef>) {
        let node = self.get_mut(node);
        node.inputs = inputs;
    }

    pub fn frame_index_input(&self) -> eyre::Result<NodeRef> {
        self.inputs
            .get(0)
            .wrap_err("Frame index input not available")
            .copied()
    }

    pub fn video_output(&self) -> eyre::Result<NodeRef> {
        self.outputs
            .get(0)
            .wrap_err("Video output not available")
            .copied()
    }

    pub fn render(&self, node: NodeRef) -> eyre::Result<Vec<NodeOutput>> {
        let node = self.get(node);
        let input_values = Vec::with_capacity(node.inputs.len());

        // TODO: This should be iterator
        for &input in &node.inputs {
            input_values.push(self.render(input)?);
        }

        node.data.render(input_values)
    }

    pub fn render_video_to(&self, path: impl AsRef<Path>) -> eyre::Result<()> {
        let path = path.as_ref();

        for frame_index in 0.. {
            let output = self.render(todo!())?;
            let Some([NodeOutput::Frame(frame)]) = output.as_array() else {
                eyre::bail!("Output wasn't a frame");
            };

            todo!("Write frame to file")
        }

        Ok(())
    }
}

pub struct Node {
    data: NodeData,
    inputs: Vec<NodeRef>,
    outputs: Vec<NodeRef>,
    static_outputs: Vec<NodeRef>,
    id: NodeRef,
}

#[derive(Debug, Clone)]
pub enum NodeData {
    Path(PathBuf),
    Index(usize),
    Frame,
    VideoSource,
    ImageSource,
}
impl NodeData {
    fn render(&self, input_values: &[NodeOutput]) -> Result<Vec<NodeOutput>, eyre::Error> {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef(usize);

pub enum NodeOutput {
    Index(usize),
    Path(PathBuf),
    Frame(Frame),
}

#[derive(Debug, Clone)]
pub struct Frame {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

pub fn black() -> Frame {
    Frame {
        width: 1,
        height: 1,
        data: vec![0, 0, 0],
    }
}
