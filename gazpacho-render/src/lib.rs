use std::collections::HashMap;

use eyre::{Context as _, OptionExt as _};
use gazpacho_compile::{NodeId, NodeInput, Op, RenderGraph};
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, SimpleValue, Time};
use gazpacho_media::{
    MediaReader, MediaWriter,
    read::{AccessPattern, ResolutionRequest},
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    resolution: ResolutionRequest,
    time: Time,
}

pub fn render(graph: RenderGraph, output: NodeId, path: &str) -> eyre::Result<()> {
    let mut renderer = Renderer {
        graph,
        output,
        extents: HashMap::new(),
        frame_cache: HashMap::new(),
        media_reader: MediaReader::new(),
    };

    // TODO: Derive fps and resolution from the graph.
    let fps = Fps::THIRTY;
    let resolution = Resolution {
        width: 1080,
        height: 1920,
    };
    let mut writer = MediaWriter::new(path, fps, resolution);

    let extent = renderer
        .extent(output)
        .wrap_err("Output should be a video stream.")?;

    tracing::debug!(?extent);

    let mut t = extent.start;
    while t < extent.end {
        let frame = renderer.render(Request {
            resolution: ResolutionRequest::auto(),
            time: t,
        })?;

        writer.write_frame(frame)?;

        tracing::debug!("Rendering t={}", t.to_string());
        t = t.advance_secs(fps.frame_length());
    }

    Ok(())
}

pub struct Renderer {
    graph: RenderGraph,
    output: NodeId,
    extents: HashMap<NodeId, Extent>,
    frame_cache: HashMap<(NodeId, Request), Frame>,
    media_reader: MediaReader,
}

impl Renderer {
    pub fn render(&mut self, request: Request) -> eyre::Result<Frame> {
        self.render_node(self.output, Some(request))?
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
                let path = self.eval_input(*path, None)?;
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

    pub fn render_node(
        &mut self,
        node_id: NodeId,
        request: Option<Request>,
    ) -> eyre::Result<Value> {
        if let Some(request) = request
            && let Some(frame) = self.frame_cache.get(&(node_id, request)).cloned()
        {
            return Ok(Value::Frame(frame));
        }

        let node = self.graph.get(node_id);
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

                let request = request.ok_or_eyre("Need request to get video frame")?;
                self.media_reader.frame(
                    path,
                    request.time,
                    request.resolution,
                    AccessPattern::Random,
                )?
            }
            Op::Contrast => {
                let input = self
                    .eval_input(node.inputs()[0], request)?
                    .as_frame()
                    .ok_or_eyre("Expected frame")?;

                input.map(|p| p.wrapping_mul(2))
            }
            Op::Concat => {
                let NodeInput::Node(a) = node.inputs()[0] else {
                    eyre::bail!("Expected node input")
                };
                let NodeInput::Node(b) = node.inputs()[1] else {
                    eyre::bail!("Expected node input")
                };
                let Some(request) = request else {
                    eyre::bail!("Cannot auto request")
                };

                let ext_a = self.extent(a)?;
                let out = if ext_a.contains(&request.time) {
                    self.render_node(a, Some(request))
                } else {
                    self.render_node(
                        b,
                        Some(Request {
                            time: request.time - ext_a.duration(),
                            ..request
                        }),
                    )
                }?;

                match out {
                    Value::Frame(f) => f,
                    _ => return Ok(out),
                }
            }
        };

        self.frame_cache
            .insert((node_id, request.unwrap()), frame.clone());
        Ok(Value::Frame(frame))
    }

    fn eval_input(&mut self, input: NodeInput, request: Option<Request>) -> eyre::Result<Value> {
        match input {
            NodeInput::Constant(v) => Ok(Value::Simple(v)),
            NodeInput::Node(node) => self.render_node(node, request),
        }
    }
}
