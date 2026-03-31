use std::{io::Write as _, process::ChildStdin};

use color_eyre::eyre::{self, ContextCompat as _};
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};

use crate::{
    data::{DataType, DataValue, Frame, SimpleDataType, SimpleDataValue, track::Track},
    node::{Node, const_node},
};

#[derive(Debug, Clone, Copy)]
pub struct NodeRef(usize);

impl NodeRef {
    pub fn port(self, index: usize) -> PortRef {
        PortRef {
            node: self,
            port_index: index,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PortRef {
    node: NodeRef,
    port_index: usize,
}

#[derive(Debug, Clone)]
pub struct GNode {
    value: Node,
    /// Invariant: `self.inputs` has exactly the same length as the inputs of the node it refers to.
    inputs: Box<[Option<PortRef>]>,
    self_ref: NodeRef,
}

impl GNode {
    fn get_named_input_port(&self, name: &'static str) -> Option<PortRef> {
        self.inputs[self.value.get_named_input_port_position(name)?]
    }

    fn get_named_output_port(&self, name: &'static str) -> Option<PortRef> {
        Some(PortRef {
            node: self.self_ref,
            port_index: self.value.get_named_output_port_position(name)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Graph {
    nodes: Vec<GNode>,
    output: PortRef,
}

impl Graph {
    pub fn new(root: Node) -> (Self, NodeRef) {
        let node_ref = NodeRef(0);
        let inputs = root.inputs().iter().map(|_| None).collect();
        let node = GNode {
            value: root,
            inputs,
            self_ref: node_ref,
        };

        (
            Graph {
                nodes: vec![node],
                output: PortRef {
                    node: node_ref,
                    port_index: 0,
                },
            },
            node_ref,
        )
    }

    pub fn insert_node(&mut self, node: Node) -> NodeRef {
        // TODO: This is copy pasted from [`Graph::new`], yucky.
        let node_ref = NodeRef(self.nodes.len());
        let inputs = node.inputs().iter().map(|_| None).collect();
        self.nodes.push(GNode {
            value: node,
            inputs,
            self_ref: node_ref,
        });
        node_ref
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

    pub fn set_input(&mut self, dest_ref: PortRef, origin: impl IntoBindRef) {
        let origin = origin.into_bind_ref(self);
        let dest = self.get_mut(dest_ref.node);
        dest.inputs[dest_ref.port_index] = Some(origin);
    }

    pub fn render_from(&self, output_port: PortRef) -> eyre::Result<DataValue> {
        let output_node = self.get(output_port.node);
        let input_values = output_node
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
            .get(output_port.node)
            .value
            .effect(output_port.port_index)
            .wrap_err("Invalid port")?;

        effect.apply(input_values)
    }

    pub fn render_to(&self, path: impl AsRef<str>) -> eyre::Result<()> {
        let output = self.get(self.output.node);
        let output_type = || output.value.outputs()[self.output.port_index].0.typ();
        // if output_type != DataType::video_track() {
        //     eyre::bail!("Don't know how to render {output_type:?}");
        // }

        let DataValue::Track(mut track) = self.render_from(self.output)? else {
            eyre::bail!("Don't know how to render {:?}", output_type());
        };
        if track.typ() != SimpleDataType::vframe() {
            eyre::bail!("Don't know how to render track of {:?}", track.typ());
        }

        let fps: f64 = self
            .render_from(
                output
                    .get_named_output_port("fps")
                    .wrap_err("Couldn't find `fps` output port on output node.")?,
            )?
            .try_into()?;

        // let DataValue::Track(mut track) = self.render_from(self.output)? else {
        //     panic!("Shouldn't be possible to get non-track here")
        // };

        // if track.typ() != SimpleDataType::vframe() {
        //     panic!("Shouldn't be possible to get non-video track here")
        // }

        let mut process = None::<(FfmpegChild, ChildStdin)>;

        for i in 0..track.length() {
            let output: Frame = track
                .render(i)
                .try_into()
                .expect("Can't get non-video frames here");

            if let Some((_ffmpeg, stdin)) = process.as_mut() {
                stdin.write_all(output.data())?;
            } else {
                let mut ffmpeg = FfmpegCommand::new()
                    .format("rawvideo")
                    .pix_fmt("rgb24")
                    .size(output.width(), output.height())
                    .rate(fps as f32)
                    .input("pipe:0")
                    .output(&path)
                    .codec_video("libx264")
                    .overwrite()
                    .spawn()?;

                let stdin = ffmpeg.take_stdin().wrap_err("Failed to open stdin")?;

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

    pub fn set_output(&mut self, bind: PortRef) {
        self.output = bind;
    }
}

pub trait IntoBindRef {
    fn into_bind_ref(self, graph: &mut Graph) -> PortRef;
}

impl IntoBindRef for PortRef {
    fn into_bind_ref(self, _graph: &mut Graph) -> PortRef {
        self
    }
}

impl<T: Into<SimpleDataValue>> IntoBindRef for T {
    fn into_bind_ref(self, graph: &mut Graph) -> PortRef {
        let const_node = const_node(self.into());
        let const_node = graph.insert_node(const_node);
        const_node.port(0)
    }
}
