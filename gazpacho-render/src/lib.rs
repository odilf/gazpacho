use std::collections::HashMap;

use eyre::{Context as _, OptionExt};
use gazpacho_compile::RenderGraph;
use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, Str};
use gazpacho_media::{
    MediaReader, MediaWriter,
    read::{AccessPattern, ResolutionRequest},
};
use gazpacho_operations::{NodeId, NodeInput, PartialRequest, Request, Value};

pub struct Renderer {
    graph: RenderGraph,
    output: NodeId,
    frame_cache: HashMap<(NodeId, PartialRequest), Frame>,
    media_reader: MediaReader,
    module: Module,
    // TODO: Consider nohash_hasher (certainly not SIP hash)
    extents: HashMap<NodeId, Extent>,
    resolutions: HashMap<NodeId, Resolution>,
    fps: HashMap<NodeId, Option<Fps>>,
}

impl Renderer {
    pub fn new(graph: RenderGraph, output: NodeId, module: Module) -> Self {
        Self {
            graph,
            output,
            module,
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
        // Copy-pasted from [`ResolutionRequest::resolve`],
        // except lazy and erroing native res.
        let resolution = match resolution {
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

        let mut writer = MediaWriter::new(output, fps, resolution)?;

        let extent = self
            .extent(self.output)
            .wrap_err("Output should be a video stream.")?;

        tracing::debug!(?extent);

        let mut t = extent.start;
        while t < extent.end {
            let frame = self.render_frame(Request {
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
            .to_frame()
            .wrap_err("Output was not a frame.")
    }

    pub fn extent(&mut self, node_id: NodeId) -> eyre::Result<Extent> {
        if let Some(extent) = self.extents.get(&node_id).copied() {
            return Ok(extent);
        }

        let node = self.graph.get(node_id);
        let extent = node.op().extent(self)?;
        self.extents.insert(node_id, extent);
        Ok(extent)
    }

    pub fn resolution(&mut self, node_id: NodeId) -> eyre::Result<Resolution> {
        if let Some(resolution) = self.resolutions.get(&node_id).copied() {
            return Ok(resolution);
        }

        let node = self.graph.get(node_id);
        let resolution = node.op().resolution(self)?;
        self.resolutions.insert(node_id, resolution);
        Ok(resolution)
    }

    pub fn fps(&mut self, node_id: NodeId) -> eyre::Result<Option<Fps>> {
        if let Some(fps) = self.fps.get(&node_id).copied() {
            return Ok(fps);
        }

        let node = self.graph.get(node_id);
        let fps = node.op().fps(self)?;
        self.fps.insert(node_id, fps);
        Ok(fps)
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
        let frame = node.op().frame(self, request)?;

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

impl gazpacho_operations::Renderer for Renderer {
    fn extent(&mut self, node: NodeInput) -> eyre::Result<Extent> {
        let NodeInput::Node(node) = node else {
            eyre::bail!("Extent needs node.");
        };

        self.extent(node)
    }

    fn resolution(&mut self, node: NodeInput) -> eyre::Result<Resolution> {
        let NodeInput::Node(node) = node else {
            eyre::bail!("Resolution needs node.");
        };

        self.resolution(node)
    }

    fn fps(&mut self, node: NodeInput) -> eyre::Result<Option<Fps>> {
        let NodeInput::Node(node) = node else {
            eyre::bail!("fps needs node.");
        };

        self.fps(node)
    }

    fn eval(&mut self, node: NodeInput, request: Request) -> eyre::Result<Value> {
        self.eval_input(node, request)
    }

    fn load_frame(&mut self, path: Str, request: Request) -> eyre::Result<Frame> {
        let path = self.module.str(path);
        self.media_reader.frame(
            path,
            request.time,
            ResolutionRequest::Manual(request.resolution),
            AccessPattern::Random,
        )
    }

    fn load_extent(&mut self, path: Str) -> eyre::Result<Extent> {
        let path = self.module.str(path);
        Ok(self
            .media_reader
            .metadata(path)?
            .video
            .first()
            .ok_or_eyre("No video available")?
            .extent)
    }
    fn load_resolution(&mut self, path: Str) -> eyre::Result<Resolution> {
        let path = self.module.str(path);
        Ok(self
            .media_reader
            .metadata(path)?
            .video
            .first()
            .ok_or_eyre("No video available")?
            .resolution)
    }
    fn load_fps(&mut self, path: Str) -> eyre::Result<Fps> {
        let path = self.module.str(path);
        self.media_reader
            .metadata(path)?
            .video
            .first()
            .ok_or_eyre("No video available")?
            .timing
            .as_constant()
            .ok_or_eyre("Variable fps not supported yet.")
    }
}
