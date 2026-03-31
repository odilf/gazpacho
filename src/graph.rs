use std::{io::Write as _, process::ChildStdin};

use color_eyre::eyre;
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};

use crate::{
    data::{DataType, DataValue, Frame, SimpleDataType, SimpleDataValue},
    node::{Node, const_node},
};

#[derive(Debug, Clone, Copy)]
pub struct NodeRef(usize);

impl NodeRef {
    pub fn bind(self, index: usize) -> BindRef {
        BindRef {
            node: self,
            bind_index: index,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BindRef {
    node: NodeRef,
    bind_index: usize,
}

#[derive(Debug, Clone)]
pub struct GNode {
    value: Node,
    /// Invariant: `self.inputs` has exactly the same length as the inputs of the node it refers to.
    inputs: Box<[Option<BindRef>]>,
}

impl GNode {
    fn get_named_bind_position(&self, name: &'static str) -> Option<usize> {
        todo!()
    }
}

impl From<Node> for GNode {
    fn from(node: Node) -> Self {
        let inputs = node.inputs().iter().map(|_| None).collect();

        GNode {
            value: node,
            inputs,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Graph {
    nodes: Vec<GNode>,
    output: BindRef,
}

impl Graph {
    pub fn new(root: Node) -> (Self, NodeRef) {
        (
            Graph {
                nodes: vec![root.into()],
                output: BindRef {
                    node: NodeRef(0),
                    bind_index: 0,
                },
            },
            NodeRef(0),
        )
    }

    pub fn insert_node(&mut self, node: Node) -> NodeRef {
        self.nodes.push(node.into());
        NodeRef(self.nodes.len() - 1)
    }

    pub fn get(&self, node_ref: NodeRef) -> &GNode {
        self.nodes
            .get(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    pub fn get_mut(&mut self, node_ref: NodeRef) -> &mut GNode {
        self.nodes
            .get_mut(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    pub fn named_bind(&self, node_ref: NodeRef, name: &'static str) -> Option<BindRef> {
        let node = self.get(node_ref);
        let index = node.get_named_bind_position(name)?;
        Some(BindRef {
            node: node_ref,
            bind_index: index,
        })
    }

    pub fn set_input(&mut self, dest_ref: BindRef, origin: impl IntoBindRef) {
        let origin = origin.into_bind_ref(self);
        let dest = self.get_mut(dest_ref.node);
        dest.inputs[dest_ref.bind_index] = Some(origin);
    }

    pub fn render_from(&self, output_ref: BindRef) -> eyre::Result<DataValue> {
        let output = self.get(output_ref.node);
        let inputs = output
            .inputs
            .iter()
            .map(|&input| {
                let Some(input) = input else {
                    eyre::bail!("Input is unset")
                };

                self.render_from(input)
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        let effect = self
            .get(output_ref.node)
            .value
            .effect(output_ref.bind_index);
        Ok(effect.apply(inputs)?)
    }

    pub fn render_to(&self, path: impl AsRef<str>) -> eyre::Result<()> {
        let output = self.get(self.output.node);
        let output_type = output.value.outputs()[self.output.bind_index].0.typ();
        if output_type != DataType::video_track() {
            eyre::bail!("Don't know how to render {output_type:?}");
        }

        let DataValue::Track {
            length,
            renderer,
            typ: SimpleDataType::VideoFrame,
        } = self.render_from(self.output)?
        else {
            panic!("Shouldn't be possible to get non-track here")
        };

        let mut process = None::<(FfmpegChild, ChildStdin)>;

        for i in 0..length {
            eprintln!("Frame {i}/{length}");
            let output: Frame = (renderer)(i)
                .try_into()
                .expect("Can't get non-video frames here");

            if let Some((_ffmpeg, stdin)) = process.as_mut() {
                stdin.write_all(&output.data())?;
            } else {
                let mut ffmpeg = FfmpegCommand::new()
                    .format("rawvideo")
                    .pix_fmt("rgb24") // or whatever pixel format your frames use
                    .size(output.width(), output.height()) // Get from first frame
                    .rate(30.0) // fps - adjust as needed
                    .input("pipe:0") // Read from stdin
                    .output(&path)
                    .codec_video("libx264") // or "libx265", "vp9", etc.
                    .overwrite() // Overwrite output file if it exists
                    .spawn()?;

                let stdin = ffmpeg.take_stdin().expect("Failed to open stdin");

                process = Some((ffmpeg, stdin))
            }
        }

        let (mut ffmpeg, stdin) = process.unwrap();
        drop(stdin);

        let output = ffmpeg.wait()?;
        if !output.success() {
            eyre::bail!("FFmpeg encoding failed");
        }

        Ok(())
    }

    pub fn set_output(&mut self, bind: BindRef) {
        self.output = bind;
    }
}

pub trait IntoBindRef {
    fn into_bind_ref(self, graph: &mut Graph) -> BindRef;
}

impl IntoBindRef for BindRef {
    fn into_bind_ref(self, _graph: &mut Graph) -> BindRef {
        self
    }
}

impl<T: Into<SimpleDataValue>> IntoBindRef for T {
    fn into_bind_ref(self, graph: &mut Graph) -> BindRef {
        let const_node = const_node(self.into());
        let const_node = graph.insert_node(const_node);
        const_node.bind(0)
    }
}
