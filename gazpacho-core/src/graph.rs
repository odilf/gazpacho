use std::{
    collections::{HashMap, HashSet},
    fmt,
    io::Write as _,
    process::ChildStdin,
};

use color_eyre::eyre::{self, ContextCompat as _};
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};
use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, Visitor},
    ser::SerializeMap,
};

use crate::{
    data::{DataValue, Frame, Port, SimpleDataType, SimpleDataValue},
    node::NodeDescriptor,
};

/// A reference to a [`NodeInstance`] in a particular [`Graph`].
///
/// It is guaranteed to point to a valid node of the graph it came from. At least, guaranteed until
/// I implement deleting nodes...
///
/// See also [`PortRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef(usize);

/// A reference to a [`Port`] in a particular [`Graph`].
/// Can be either a [`PortInRef`] or [`PortOutRef`].
///
/// See also [`NodeRef`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortRef<T> {
    node: NodeRef,
    port_index: usize,
    meta: T,
}

impl<T> PortRef<T> {
    pub fn node(&self) -> NodeRef {
        self.node
    }

    pub fn port_index(&self) -> usize {
        self.port_index
    }
}

mod private {
    pub trait PortTypeSeal {}
}

pub trait PortType: private::PortTypeSeal + Copy + std::hash::Hash + Send + Sync {
    const TYPE: DynPortType;
    const IS_INPUT: bool = matches!(Self::TYPE, DynPortType::Input);
    const INSTANCE: Self;
    type Other: PortType;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DynPortType {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct InputPort;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutputPort;

impl private::PortTypeSeal for InputPort {}
impl private::PortTypeSeal for OutputPort {}

impl PortType for InputPort {
    const TYPE: DynPortType = DynPortType::Input;
    const INSTANCE: Self = Self;
    type Other = OutputPort;
}

impl PortType for OutputPort {
    const TYPE: DynPortType = DynPortType::Output;
    const INSTANCE: Self = Self;
    type Other = InputPort;
}

pub type PortInRef = PortRef<InputPort>;
pub type PortOutRef = PortRef<OutputPort>;
pub type GenericPortRef = PortRef<DynPortType>;

impl<T: PortType> PortRef<T> {
    pub fn as_generic(self) -> GenericPortRef {
        PortRef {
            node: self.node,
            port_index: self.port_index,
            meta: T::TYPE,
        }
    }
}

impl GenericPortRef {
    pub fn input_output<T: PortType>(
        a: PortRef<T>,
        b: GenericPortRef,
    ) -> Option<(PortInRef, PortOutRef)> {
        match (T::TYPE, b.meta) {
            (DynPortType::Input, DynPortType::Output) => Some((
                PortInRef {
                    node: a.node,
                    port_index: a.port_index,
                    meta: InputPort::INSTANCE,
                },
                PortOutRef {
                    node: b.node,
                    port_index: b.port_index,
                    meta: OutputPort::INSTANCE,
                },
            )),
            (DynPortType::Output, DynPortType::Input) => Some((
                PortInRef {
                    node: b.node,
                    port_index: b.port_index,
                    meta: InputPort::INSTANCE,
                },
                PortOutRef {
                    node: a.node,
                    port_index: a.port_index,
                    meta: OutputPort::INSTANCE,
                },
            )),
            _ => None,
        }
    }
}

impl NodeDescriptor {
    pub fn named_port_ref<T: PortType>(&self, node: NodeRef, name: &str) -> Option<PortRef<T>> {
        let port_index = if T::IS_INPUT {
            self.inputs().iter().position(|port| port.name() == name)?
        } else {
            self.outputs()
                .iter()
                .position(|(port, _)| port.name() == name)?
        };

        Some(PortRef {
            node,
            port_index,
            meta: T::INSTANCE,
        })
    }
}

/// An instance of a [`Node`](`NodeDescriptor`) in a [`Graph`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInstance<M = ()> {
    descriptor: &'static NodeDescriptor,

    /// The ports that the input of this node connects to.
    ///
    /// Invariant: `self.inputs` has exactly the same length as the number of input ports of the
    /// node it refers to.
    inputs: Box<[Option<InputValue>]>,

    /// The ports that the input of this node connects to.
    ///
    /// Invariant: `self.outputs` has exactly the same length as the number of output ports of the
    /// node it refers to.
    // TODO: Using a whole ass `HashSet` here is overkill. Maybe try a `VecSet`.
    outputs: Box<[HashSet<PortInRef>]>,

    // TODO: This feels unecessary and ugly. But it is practical...
    self_ref: NodeRef,
    pub metadata: M,
}

impl<M> NodeInstance<M> {
    pub fn port_refs<T: PortType>(&self) -> impl Iterator<Item = PortRef<T>> + ExactSizeIterator {
        let n = match T::TYPE {
            DynPortType::Input => self.descriptor().inputs().len(),
            DynPortType::Output => self.descriptor().outputs().len(),
        };

        // Assuming internal invariant: Port indices are always in order, starting from `0`.
        (0..n).map(move |i| PortRef {
            node: self.self_ref,
            port_index: i,
            meta: T::INSTANCE,
        })
    }

    pub fn inputs(
        &self,
    ) -> impl Iterator<Item = (PortInRef, Option<InputValue>)> + ExactSizeIterator {
        self.port_refs().zip(&self.inputs).map(|(p, &i)| (p, i))
    }

    pub fn outputs(&self) -> &[HashSet<PortInRef>] {
        &self.outputs
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InputValue {
    Port(PortOutRef),
    Const(PortInRef),
}

impl<M> NodeInstance<M> {
    pub fn descriptor(&self) -> &'static NodeDescriptor {
        self.descriptor
    }

    pub fn named_port_ref<T: PortType>(&self, name: &str) -> Option<PortRef<T>> {
        self.descriptor().named_port_ref(self.self_ref, name)
    }

    pub fn io(&self) -> NodeIo {
        NodeIo {
            node_ref: self.self_ref,
            descriptor: self.descriptor,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Graph<M = ()> {
    nodes: Vec<NodeInstance<M>>,
    output: Option<PortOutRef>,
    consts: GraphConsts,
    // Map from output ports to values
    #[serde(skip)]
    output_cache: HashMap<PortOutRef, DataValue>,
}

impl Graph {
    /// Constructs a new [`Graph`] without any metadata.
    ///
    /// See [`Graph::default`] to be able to use metadata.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            output: None,
            consts: GraphConsts::default(),
            output_cache: HashMap::new(),
        }
    }
}

impl<M> Graph<M> {
    pub fn nodes(&self) -> &[NodeInstance<M>] {
        &self.nodes
    }

    pub fn node_refs(&self) -> impl Iterator<Item = NodeRef> + use<M> {
        // Assuming internal invariant: `NodeId`s are always in order, starting from `0`.
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

    pub fn invalidate_cache(&mut self, node_ref: NodeRef) {
        let mut done = true;
        // TODO: Unnecessary allocation.
        for output in self.port_refs::<OutputPort>(node_ref).collect::<Box<[_]>>() {
            let removed = self.output_cache.remove(&output);
            if !done && removed.is_some() {
                done = false;
            }
        }

        if done {
            return;
        }

        for input in self
            .get(node_ref)
            .outputs()
            .iter()
            .flatten()
            .copied()
            // TODO: This clone is theoretically unecessary.
            .collect::<Box<[_]>>()
        {
            self.invalidate_cache(input.node);
        }
    }

    pub fn port_refs<T: PortType>(
        &self,
        node_ref: NodeRef,
    ) -> impl Iterator<Item = PortRef<T>> + ExactSizeIterator {
        self.get(node_ref).port_refs::<T>()
    }

    pub fn get_port<T: PortType>(&self, port_ref: PortRef<T>) -> Port {
        if T::IS_INPUT {
            *self
                .get(port_ref.node)
                .descriptor()
                .inputs()
                .get(port_ref.port_index)
                .unwrap()
        } else {
            self.get(port_ref.node)
                .descriptor()
                .outputs()
                .get(port_ref.port_index)
                .unwrap()
                .0
        }
    }

    pub fn insert_node(&mut self, node: &'static NodeDescriptor) -> NodeRef
    where
        M: Default,
    {
        self.insert_node_with_meta(node, M::default())
    }

    pub fn insert_node_with_meta(&mut self, node: &'static NodeDescriptor, metadata: M) -> NodeRef {
        let node_ref = NodeRef(self.nodes.len());
        let inputs = node.inputs().iter().map(|_| None).collect();
        let outputs = node.outputs().iter().map(|_| HashSet::new()).collect();
        self.nodes.push(NodeInstance {
            descriptor: node,
            inputs,
            outputs,
            self_ref: node_ref,
            metadata,
        });
        node_ref
    }

    pub fn connect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = Some(InputValue::Port(output));
        self.get_mut(output.node).outputs[output.port_index].insert(input);
        self.invalidate_cache(input.node);
    }

    pub fn is_connected(&self, output: PortOutRef, input: PortInRef) -> bool {
        self.get(input.node).inputs[input.port_index] == Some(InputValue::Port(output))
    }

    pub fn disconnect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = None;
        self.get_mut(output.node).outputs[output.port_index].remove(&input);
        self.invalidate_cache(input.node);
    }

    pub fn set_const_input(&mut self, input: PortInRef, value: impl Into<SimpleDataValue>) {
        self.consts.insert(input, value.into());
        self.get_mut(input.node).inputs[input.port_index] = Some(InputValue::Const(input));
        self.invalidate_cache(input.node);
    }
}

// Video rendering
impl<M> Graph<M> {
    // Guaranteed to cache the output value (if not exiting with errors).
    pub fn render_from(&mut self, output_port: PortOutRef) -> eyre::Result<&DataValue> {
        // Non-idiomatic rust because of borrowing.
        if self.output_cache.contains_key(&output_port) {
            return Ok(self.output_cache.get(&output_port).unwrap());
        }

        // Make sure all inputs are rendered
        let realized_input_ports = self
            .get(output_port.node)
            .inputs
            .iter()
            .copied()
            .collect::<Option<Box<[_]>>>()
            .wrap_err("Some inputs are unset")?;

        for &input in &realized_input_ports {
            if let InputValue::Port(port) = input {
                self.render_from(port)?;
            }
        }

        // then, render
        let inputs = realized_input_ports
            .iter()
            .map(|&input| match input {
                InputValue::Port(port) => self.output_cache.get(&port).unwrap(),
                InputValue::Const(port) => self.consts.get(port).unwrap(),
            })
            .collect::<Box<[_]>>();

        let effect = self.get(output_port.node).descriptor().outputs()[output_port.port_index].1;
        let output = effect(&inputs);

        self.output_cache.insert(output_port, output);
        Ok(self.output_cache.get(&output_port).unwrap())
    }

    pub fn render_to(&mut self, path: impl AsRef<str>) -> eyre::Result<()> {
        let Some(output_port) = self.output else {
            eyre::bail!("No output set!");
        };

        let fps_port = self
            .get(output_port.node)
            .named_port_ref("fps")
            .wrap_err("Couldn't find `fps` output port on output node.")?;
        let fps: &f64 = self.render_from(fps_port)?.try_into()?;
        let fps = *fps;

        let DataValue::Track(track) = self.render_from(output_port)? else {
            let output_type = self.get_port(output_port).typ();
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

    pub fn set_global_output(&mut self, port: PortOutRef) {
        self.output = Some(port);
    }
}

pub struct NodeIo {
    node_ref: NodeRef,
    descriptor: &'static NodeDescriptor,
}

impl NodeIo {
    pub fn get_named_port<T: PortType>(&self, name: &str) -> Option<PortRef<T>> {
        self.descriptor.named_port_ref(self.node_ref, name)
    }
    pub fn port<T: PortType>(&self, name: &str) -> PortRef<T> {
        self.get_named_port(name).unwrap()
    }
}

impl<M: Clone> Clone for Graph<M> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            output: self.output,
            consts: self.consts.clone(),
            output_cache: HashMap::new(),
        }
    }
}

impl Clone for GraphConsts {
    fn clone(&self) -> Self {
        Self(
            self.iter()
                .map(|(k, v)| (k, DataValue::Simple(v.clone())))
                .collect(),
        )
    }
}

#[derive(Debug, Default)]
struct GraphConsts(HashMap<PortInRef, DataValue>);

impl GraphConsts {
    fn get(&self, port: PortInRef) -> Option<&DataValue> {
        self.0.get(&port)
    }

    fn insert(&mut self, port: PortInRef, value: SimpleDataValue) -> Option<DataValue> {
        self.0.insert(port, DataValue::Simple(value))
    }

    fn iter(&self) -> impl Iterator<Item = (PortInRef, &SimpleDataValue)> {
        self.0.iter().map(|(k, v)| {
            let DataValue::Simple(v) = v else {
                unreachable!("Can only have `SimpleDataValue`s in consts");
            };
            (*k, v)
        })
    }
}

impl Serialize for GraphConsts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut g = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in self.iter() {
            g.serialize_entry(&k, &v)?;
        }
        g.end()
    }
}

struct GraphConstsVisitor {}

impl<'de> Visitor<'de> for GraphConstsVisitor {
    type Value = GraphConsts;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a `PortInRef` to `SimpleDataValue` map")
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut map = GraphConsts(HashMap::with_capacity(access.size_hint().unwrap_or(0)));
        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<'de> Deserialize<'de> for GraphConsts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(GraphConstsVisitor {})
    }
}
