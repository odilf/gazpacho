use std::{collections::HashMap, io::Write as _, process::ChildStdin};

use color_eyre::eyre::{self, ContextCompat as _};
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};
use serde::{Deserialize, Serialize};

use crate::{
    data::{DataValue, Frame, Port, SimpleDataType, SimpleDataValue},
    node::{Effect, Node, const_node},
};

/// A reference to a [`GNode`] in a particular instance of a [`Graph`].
///
/// It is guaranteed to point to a valid node of the graph it came from. At least, guaranteed until
/// I implement deleting nodes...
///
/// See also [`PortRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef(usize);

/// A reference to a [`Port`] in a particular instance of a [`Graph`].
///
/// See also [`NodeRef`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortRef {
    node: NodeRef,
    port_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInstance<M = ()> {
    value: Node,
    /// Invariant: `self.inputs` has exactly the same length as the inputs of the node it refers to.
    inputs: Box<[Option<PortRef>]>,
    self_ref: NodeRef,
    pub metadata: M,
}

impl<M> NodeInstance<M> {
    pub fn inner(&self) -> &Node {
        &self.value
    }

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
pub struct Graph<M = ()> {
    nodes: Vec<NodeInstance<M>>,
    output: Option<PortRef>,
    // Map from output ports to values
    #[serde(skip)]
    output_cache: HashMap<PortRef, DataValue>,
}

impl Graph {
    /// Constructs a new [`Graph`] without any metadata.
    ///
    /// See [`Graph::default`] to be able to use metadata.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            output: None,
            output_cache: HashMap::new(),
        }
    }
}

impl<M> Graph<M> {
    pub fn nodes(&self) -> &[NodeInstance<M>] {
        &self.nodes
    }

    pub fn node_refs(&self) -> impl Iterator<Item = NodeRef> + use<M> {
        // Internal assumption: `NodeRef`s are always in order, starting from `0`.
        (0..self.nodes.len()).map(NodeRef)
    }

    pub fn nodes_mut(&mut self) -> impl Iterator<Item = &mut NodeInstance<M>> {
        self.nodes.iter_mut()
    }

    pub fn get(&self, node_ref: NodeRef) -> &NodeInstance<M> {
        self.nodes
            .get(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    // NOTE: Not `pub`, because then you could change const values without properly
    // invalidating the cache!
    fn get_mut(&mut self, node_ref: NodeRef) -> &mut NodeInstance<M> {
        self.nodes
            .get_mut(node_ref.0)
            .expect("`NodeRef`s point to valid nodes.")
    }

    pub fn get_meta_mut(&mut self, node_ref: NodeRef) -> &mut M {
        &mut self.get_mut(node_ref).metadata
    }

    pub fn get_input_port(&self, port_ref: PortRef) -> Option<&Port> {
        self.get(port_ref.node)
            .inner()
            .inputs()
            .get(port_ref.port_index)
    }

    pub fn get_output_port(&self, port_ref: PortRef) -> Option<&Port> {
        self.get(port_ref.node)
            .inner()
            .outputs()?
            .get(port_ref.port_index)
            .map(|(port, _)| port)
    }

    pub fn insert_node(&mut self, node: Node) -> NodeRef
    where
        M: Default,
    {
        self.insert_node_with_meta(node, M::default())
    }

    pub fn insert_node_with_meta(&mut self, node: Node, metadata: M) -> NodeRef {
        let node_ref = NodeRef(self.nodes.len());
        let inputs = node.inputs().iter().map(|_| None).collect();
        self.nodes.push(NodeInstance {
            value: node,
            inputs,
            self_ref: node_ref,
            metadata,
        });
        node_ref
    }

    pub fn set_input(&mut self, dest_ref: PortRef, origin: impl IntoBindRef<M>) {
        let origin = origin.into_bind_ref(self);
        let dest = self.get_mut(dest_ref.node);

        dest.inputs[dest_ref.port_index] = Some(origin);
    }

    // Guaranteed to cache the output value (if not exiting with errors).
    //
    // The return is only `mut` because of tracks, but I feel now that tracks should just have interior mutability...
    pub fn render_from(&mut self, output_port: PortRef) -> eyre::Result<&mut DataValue> {
        if !self.output_cache.contains_key(&output_port) {
            let output_node = self.get(output_port.node);
            match output_node.inner() {
                Node::Const { value } if output_port.port_index == 0 => {
                    self.output_cache
                        .insert(output_port, DataValue::Simple(value.clone()));
                    return Ok(self.output_cache.get_mut(&output_port).unwrap());
                }
                _ => (),
            };
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

impl<M: Clone> Clone for Graph<M> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            output: self.output,
            output_cache: HashMap::new(),
        }
    }
}

pub trait IntoBindRef<M> {
    fn into_bind_ref(self, graph: &mut Graph<M>) -> PortRef;
}

impl<M> IntoBindRef<M> for PortRef {
    fn into_bind_ref(self, _graph: &mut Graph<M>) -> PortRef {
        self
    }
}

impl<M: Default, T: Into<SimpleDataValue>> IntoBindRef<M> for T {
    fn into_bind_ref(self, graph: &mut Graph<M>) -> PortRef {
        let const_node = const_node(self.into());
        let const_node = graph.insert_node(const_node);
        PortRef {
            node: const_node,
            port_index: 0,
        }
    }
}
