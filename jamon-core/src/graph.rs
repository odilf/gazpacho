use std::{collections::HashMap, io::Write as _, process::ChildStdin};

use color_eyre::eyre::{self, ContextCompat as _};
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};
use serde::{Deserialize, Serialize};

use crate::{
    data::{DataValue, Frame, SimpleDataType, SimpleDataValue},
    node::{Effect, Node, const_node},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef(usize);

impl NodeRef {
    // TODO: This is convinient, but it allows to freely create invalid references.
    // I think it should get axed.
    pub fn port(self, index: usize) -> PortRef {
        PortRef {
            node: self,
            port_index: index,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortRef {
    node: NodeRef,
    port_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GNode {
    value: Node,
    /// Invariant: `self.inputs` has exactly the same length as the inputs of the node it refers to.
    inputs: Box<[Option<PortRef>]>,
    self_ref: NodeRef,
}

impl GNode {
    pub fn get_named_input_port(&self, name: &'static str) -> Option<PortRef> {
        self.inputs[self.value.get_named_input_port_position(name)?]
    }

    pub fn get_named_output_port(&self, name: &'static str) -> Option<PortRef> {
        Some(PortRef {
            node: self.self_ref,
            port_index: self.value.get_named_output_port_position(name)?,
        })
    }

    fn realized_inputs(&self) -> Option<Box<[PortRef]>> {
        self.inputs.iter().copied().collect()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    nodes: Vec<GNode>,
    output: Option<PortRef>,
    // Map from output ports to values
    #[serde(skip)]
    output_cache: HashMap<PortRef, DataValue>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            output: None,
            output_cache: HashMap::new(),
        }
    }

    pub fn insert_node(&mut self, node: Node) -> NodeRef {
        let node_ref = NodeRef(self.nodes.len());
        let inputs = node.inputs().iter().map(|_| None).collect();
        self.nodes.push(GNode {
            value: node,
            inputs,
            self_ref: node_ref,
        });
        node_ref
    }

    // NOTE: No `get_mut` exists, because then you could change const values without properly
    // invalidating the cache!
    pub fn get(&self, node_ref: NodeRef) -> &GNode {
        self.nodes
            .get(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    pub fn set_input(&mut self, dest_ref: PortRef, origin: impl IntoBindRef) {
        let origin = origin.into_bind_ref(self);
        let dest = self
            .nodes
            .get_mut(dest_ref.node.0)
            .expect("`NodeRef`s point to valid nodes.");

        dest.inputs[dest_ref.port_index] = Some(origin);
    }

    // Guaranteed to cache the output value (if not exiting with errors).
    pub fn render_from(&mut self, output_port: PortRef) -> eyre::Result<&mut DataValue> {
        if !self.output_cache.contains_key(&output_port) {
            // TODO: This does an allocation which I think is unecessary
            let Some(input_ports) = self.get(output_port.node).realized_inputs() else {
                eyre::bail!("Some input is unset")
            };

            // We first generate the output for every input, then we collect
            // them. It would be nice to do it "on the fly", but the HashMap
            // might get reallocated in the middle of the process. But I feel
            // it is possible to do something smarter than what I'm doing now
            for &input_port in &input_ports {
                self.render_from(input_port)?;
            }

            let input_values = input_ports
                .into_iter()
                // Safe to unwrap, since we just generated the outputs.
                .map(|port| self.output_cache.get(&port).unwrap())
                .collect::<Box<[_]>>();

            let output_node = self.get(output_port.node);
            let effect = output_node
                .value
                .effect(output_port.port_index)
                .wrap_err("Invalid port")?;

            let output_value = match effect {
                Effect::Fn(f) => f(&input_values)?,
            };

            self.output_cache.insert(output_port, output_value);
        }

        Ok(self.output_cache.get_mut(&output_port).unwrap())
    }

    pub fn render_to(&mut self, path: impl AsRef<str>) -> eyre::Result<()> {
        let Some(output_port) = self.output else {
            eyre::bail!("No output set!");
        };

        let fps_port = self
            .get(output_port.node)
            .get_named_output_port("fps")
            .wrap_err("Couldn't find `fps` output port on output node.")?;
        let fps: &f64 = (&*self.render_from(fps_port)?).try_into()?;
        let fps = *fps;

        let DataValue::Track(track) = self.render_from(output_port)? else {
            let output = self.get(output_port.node);
            let output_type = output
                .value
                .port_by_index(output_port.port_index)
                .unwrap()
                .typ();
            eyre::bail!("Don't know how to render {:?}", output_type);
        };
        if track.typ() != SimpleDataType::vframe() {
            eyre::bail!("Don't know how to render track of {:?}", track.typ());
        }

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

    pub fn set_output(&mut self, port: PortRef) {
        self.output = Some(port);
    }
}

impl Clone for Graph {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            output: self.output,
            output_cache: HashMap::new(),
        }
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
