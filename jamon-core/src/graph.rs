use std::{collections::HashMap, fmt, io::Write as _, marker::PhantomData, process::ChildStdin};

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
pub struct PortRef<T: PortType> {
    node: NodeRef,
    port_index: usize,
    _marker: PhantomData<T>,
}

mod private {
    pub trait PortTypeSeal {}
}

pub trait PortType: private::PortTypeSeal {
    const IS_INPUT: bool;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct InputPort;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutputPort;

impl private::PortTypeSeal for InputPort {}
impl private::PortTypeSeal for OutputPort {}

impl PortType for InputPort {
    const IS_INPUT: bool = true;
}

impl PortType for OutputPort {
    const IS_INPUT: bool = false;
}

pub type PortInRef = PortRef<InputPort>;
pub type PortOutRef = PortRef<OutputPort>;

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
            _marker: PhantomData,
        })
    }
}

/// An instance of a [`Node`](`NodeDescriptor`) in a [`Graph`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInstance<M = ()> {
    descriptor: &'static NodeDescriptor,
    /// The ports that the input of this node connects to.
    ///
    /// Invariant: `self.inputs` has exactly the same length as the inputs of the node it refers to.
    inputs: Box<[Option<InputValue>]>,
    // TODO: This feels unecessary and ugly.
    self_ref: NodeRef,
    pub metadata: M,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

    pub fn output_port_refs(&self, node_ref: NodeRef) -> impl Iterator<Item = PortOutRef> {
        // Assuming internal invariant: Port indices are always in order, starting from `0`.
        (0..self.get(node_ref).descriptor().outputs().len()).map(move |i| PortRef {
            node: node_ref,
            port_index: i,
            _marker: PhantomData,
        })
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
        self.nodes.push(NodeInstance {
            descriptor: node,
            inputs,
            self_ref: node_ref,
            metadata,
        });
        node_ref
    }

    pub fn connect(&mut self, output: PortOutRef, input: PortInRef) {
        self.get_mut(input.node).inputs[input.port_index] = Some(InputValue::Port(output));
        todo!("Invalidate cache")
    }

    pub fn set_const_input(&mut self, input: PortInRef, value: impl Into<SimpleDataValue>) {
        self.get_mut(input.node).inputs[input.port_index] = Some(InputValue::Const(input));
        self.consts.insert(input, value.into());
        todo!("Invalidate cache")
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
            // .zip(self.get(output_port.node).descriptor().inputs())
            // .filter_map(|(val, _)| val.port())
            // .map(|(val, port)| Some((val?, port)))
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
            // let output = self.get(output_port.node);
            // let output_type = output
            //     .descriptor
            //     .port_by_index(output_port.port_index)
            //     .unwrap()
            //     .typ();
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

// impl<'de, M: Deserialize<'de>> Deserialize<'de> for Graph<M> {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         #[derive(Deserialize)]
//         #[serde(field_identifier, rename_all = "lowercase")]
//         enum Field {
//             Nodes,
//             Output,
//             Consts,
//         }

//         struct GraphVisitor;

//         impl<'de> Visitor<'de> for GraphVisitor {
//             type Value = Graph;

//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("struct Duration")
//             }

//             fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
//             where
//                 V: SeqAccess<'de>,
//             {
//                 let nodes = seq
//                     .next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(0, &self))?;
//                 let output = seq
//                     .next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(1, &self))?;
//                 let consts: HashMap<PortRef<InputPort>, SimpleDataValue> = seq
//                     .next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(2, &self))?;
//                 Ok(Graph {
//                     nodes,
//                     output,
//                     // TODO: Allocation is avoidable
//                     consts: consts
//                         .into_iter()
//                         .map(|(k, v)| (k, DataValue::Simple(v)))
//                         .collect(),
//                     output_cache: HashMap::new(),
//                 })
//             }

//             fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
//             where
//                 V: MapAccess<'de>,
//             {
//                 let mut nodes = None;
//                 let mut output = None;
//                 let mut consts = None::<HashMap<PortInRef, SimpleDataValue>>;
//                 while let Some(key) = map.next_key()? {
//                     match key {
//                         Field::Nodes => {
//                             if nodes.is_some() {
//                                 return Err(de::Error::duplicate_field("nodes"));
//                             }
//                             nodes = Some(map.next_value()?);
//                         }
//                         Field::Output => {
//                             if output.is_some() {
//                                 return Err(de::Error::duplicate_field("output"));
//                             }
//                             output = Some(map.next_value()?);
//                         }
//                         Field::Consts => {
//                             if consts.is_some() {
//                                 return Err(de::Error::duplicate_field("consts"));
//                             }
//                             consts = Some(map.next_value()?);
//                         }
//                     }
//                 }
//                 let nodes = nodes.ok_or_else(|| de::Error::missing_field("nodes"))?;
//                 let output = output.ok_or_else(|| de::Error::missing_field("output"))?;
//                 let consts = consts.ok_or_else(|| de::Error::missing_field("consts"))?;
//                 Ok(Graph {
//                     nodes,
//                     output,
//                     // TODO: Allocation is avoidable
//                     consts: consts
//                         .into_iter()
//                         .map(|(k, v)| (k, DataValue::Simple(v)))
//                         .collect(),
//                     output_cache: HashMap::new(),
//                 })
//             }
//         }

//         deserializer.deserialize_struct("Graph", &["nodes", "output", "consts"], GraphVisitor)
//     }
// }
