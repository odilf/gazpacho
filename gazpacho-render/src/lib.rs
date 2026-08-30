use std::collections::HashMap;

use eyre::{Context as _, OptionExt as _};
use gazpacho_compile::{NodeId, NodeInput, RenderGraph};
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, SimpleValue};
use gazpacho_media::{
    MediaReader, MediaWriter,
    read::{AccessPattern, ResolutionRequest},
};
use gazpacho_operations::{Op, color};

use crate::request::{PartialRequest, Request};

mod request;

pub enum Value {
    Simple(SimpleValue),
    Frame(Frame),
    Fps(Fps),
    Resolution(Resolution),
}

impl Value {
    #[must_use]
    pub fn as_frame(self) -> Option<Frame> {
        match self {
            Self::Frame(frame) => Some(frame),
            _ => None,
        }
    }
}

pub struct Renderer {
    graph: RenderGraph,
    output: NodeId,
    frame_cache: HashMap<(NodeId, PartialRequest), Frame>,
    media_reader: MediaReader,
    // TODO: Consider nohash_hasher (certainly not SIP hash)
    extents: HashMap<NodeId, Extent>,
    resolutions: HashMap<NodeId, Resolution>,
    fps: HashMap<NodeId, Option<Fps>>,
}

impl Renderer {
    pub fn new(graph: RenderGraph, output: NodeId) -> Self {
        Self {
            graph,
            output,
            extents: HashMap::new(),
            resolutions: HashMap::new(),
            fps: HashMap::new(),
            frame_cache: HashMap::new(),
            media_reader: MediaReader::new(),
        }
    }

    pub fn render_video(
        mut self,
        output: &str,
        fps: Fps,
        resolution: ResolutionRequest,
    ) -> eyre::Result<()> {
        // Copy-pasted from [`ResolutionRequest::resolve`], but lazy and erroing native res.
        let concrete_resolution = match resolution {
            ResolutionRequest::Auto { downsample } => {
                let downsample = u32::from(downsample.get());
                let native = self.resolution(self.output)?;
                Resolution {
                    width: (native.width / downsample).max(1),
                    height: (native.height / downsample).max(1),
                }
            }
            ResolutionRequest::Manual(resolution) => resolution,
        };

        let mut writer = MediaWriter::new(output, fps, concrete_resolution);

        let extent = self
            .extent(self.output)
            .wrap_err("Output should be a video stream.")?;

        tracing::debug!(?extent);

        let mut t = extent.start;
        while t < extent.end {
            let frame = self.render_frame(Request {
                // Does it make sense here that we're not passing in the concrete resolution?
                resolution,
                time: t,
            })?;

            writer.write_frame(frame)?;

            tracing::debug!("Rendering t={}", t.to_string());
            t = t.advance_secs(fps.frame_length());
        }

        Ok(())
    }

    pub fn render_frame(&mut self, request: Request) -> eyre::Result<Frame> {
        self.render_node(self.output, request)?
            .as_frame()
            .ok_or_eyre("Output was not a frame.")
    }

    pub fn extent(&mut self, node_id: NodeId) -> eyre::Result<Extent> {
        if let Some(extent) = self.extents.get(&node_id).copied() {
            return Ok(extent);
        }

        let node = self.graph.get(node_id);
        let extent = match node.op() {
            Op::Load => {
                let path = node.inputs().get(0).ok_or_eyre("Expected one input.")?;
                let path = self.eval_input(*path, Request::sentinel())?;
                let path = match path {
                    Value::Simple(SimpleValue::Str(str)) => {
                        self.graph.strings.resolve(str).unwrap()
                    }
                    _ => eyre::bail!("Expected string"),
                };

                self.media_reader.extent(path)?
            }
            Op::Contrast => self.extent(
                node.inputs()[0]
                    .as_node()
                    .ok_or_eyre("Expected node input")?,
            )?,
            Op::Concat => {
                let a = node.inputs()[0]
                    .as_node()
                    .ok_or_eyre("Expected node input")?;
                let b = node.inputs()[1]
                    .as_node()
                    .ok_or_eyre("Expected node input")?;

                let ext_a = self.extent(a)?;
                let ext_b = self.extent(b)?;

                Extent::new(ext_a.start, ext_a.end + ext_b.duration()).unwrap()
            }
        };

        self.extents.insert(node_id, extent.clone());
        Ok(extent)
    }

    pub fn resolution(&mut self, node_id: NodeId) -> eyre::Result<Resolution> {
        if let Some(resolution) = self.resolutions.get(&node_id).copied() {
            return Ok(resolution);
        }

        let node = self.graph.get(node_id);
        match node.op() {
            Op::Load => {
                let path = node.inputs().get(0).ok_or_eyre("Expected one input.")?;
                let path = self.eval_input(*path, Request::sentinel())?;
                let path = match path {
                    Value::Simple(SimpleValue::Str(str)) => {
                        self.graph.strings.resolve(str).unwrap()
                    }
                    _ => eyre::bail!("Expected string"),
                };

                Ok(self
                    .media_reader
                    .metadata(path)?
                    .video
                    .first()
                    .unwrap()
                    .resolution)
            }
            Op::Contrast => self.resolution(
                node.inputs()[0]
                    .as_node()
                    .ok_or_eyre("Expected node input.")?,
            ),
            Op::Concat => {
                let a = node.inputs()[0]
                    .as_node()
                    .ok_or_eyre("Expectd node input.")?;
                let b = node.inputs()[1]
                    .as_node()
                    .ok_or_eyre("Expectd node input.")?;

                let a = self.resolution(a)?;
                let b = self.resolution(b)?;

                Ok(if a == b {
                    a
                } else {
                    eyre::bail!("Concating two videos with different resolutions")
                })
            }
        }
    }

    pub fn fps(&mut self, node_id: NodeId) -> eyre::Result<Option<Fps>> {
        if let Some(fps) = self.fps.get(&node_id).copied() {
            return Ok(fps);
        }

        let node = self.graph.get(node_id);
        match node.op() {
            Op::Load => {
                let path = node.inputs().get(0).ok_or_eyre("Expected one input.")?;
                let path = self.eval_input(*path, Request::sentinel())?;
                let path = match path {
                    Value::Simple(SimpleValue::Str(str)) => {
                        self.graph.strings.resolve(str).unwrap()
                    }
                    _ => eyre::bail!("Expected string"),
                };

                Ok(self
                    .media_reader
                    .metadata(path)?
                    .video
                    .first()
                    .unwrap()
                    .timing
                    .as_constant())
            }
            Op::Contrast => self.fps(
                node.inputs()[0]
                    .as_node()
                    .ok_or_eyre("Expectd node input.")?,
            ),
            Op::Concat => {
                let a = node.inputs()[0]
                    .as_node()
                    .ok_or_eyre("Expectd node input.")?;
                let b = node.inputs()[1]
                    .as_node()
                    .ok_or_eyre("Expectd node input.")?;

                let a = self.fps(a)?;
                let b = self.fps(b)?;

                Ok(if a == b { a } else { None })
            }
        }
    }

    pub fn output_fps(&mut self) -> eyre::Result<Option<Fps>> {
        self.fps(self.output)
    }

    pub fn render_node(&mut self, node_id: NodeId, request: Request) -> eyre::Result<Value> {
        let deps = self.graph.get(node_id).deps();
        if let Some(frame) = self
            .frame_cache
            .get(&(node_id, request.select(deps)))
            .cloned()
        {
            return Ok(Value::Frame(frame));
        }

        let node = self.graph.get(node_id);

        // TODO: Define these in `gazpacho_operations`
        let frame = match node.op() {
            Op::Load => {
                let path = node.inputs().get(0).ok_or_eyre("Expected one input.")?;
                let path = self.eval_input(*path, request)?;
                let path = match path {
                    Value::Simple(SimpleValue::Str(str)) => {
                        self.graph.strings.resolve(str).unwrap()
                    }
                    _ => eyre::bail!("Expected string"),
                };

                self.media_reader.frame(
                    path,
                    request.time,
                    request.resolution,
                    AccessPattern::Random,
                )?
            }
            Op::Contrast => {
                let input = node.inputs()[0];
                let amount = node.inputs()[1];
                let input = self
                    .eval_input(input, request)?
                    .as_frame()
                    .ok_or_eyre("Expected frame")?;

                let Value::Simple(SimpleValue::Float(amount)) = self.eval_input(amount, request)?
                else {
                    eyre::bail!("Expected float.")
                };

                color::contrast(input, amount.into())
            }
            Op::Concat => {
                let NodeInput::Node(a) = node.inputs()[0] else {
                    eyre::bail!("Expected node input")
                };
                let NodeInput::Node(b) = node.inputs()[1] else {
                    eyre::bail!("Expected node input")
                };

                let ext_a = self.extent(a)?;
                let out = if ext_a.contains(&request.time) {
                    self.render_node(a, request)
                } else {
                    self.render_node(
                        b,
                        Request {
                            time: request.time - ext_a.duration(),
                            ..request
                        },
                    )
                }?;

                match out {
                    Value::Frame(f) => f,
                    _ => return Ok(out),
                }
            }
        };

        let deps = self.graph.get(node_id).deps();
        self.frame_cache
            .insert((node_id, request.select(deps)), frame.clone());
        Ok(Value::Frame(frame))
    }

    fn eval_input(&mut self, input: NodeInput, request: Request) -> eyre::Result<Value> {
        match input {
            NodeInput::Constant(v) => Ok(Value::Simple(v)),
            NodeInput::Node(node) => self.render_node(node, request),
        }
    }
}
