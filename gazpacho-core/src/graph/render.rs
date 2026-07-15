use std::{any::Any, io::Write as _, process::ChildStdin};

use color_eyre::eyre::{self, OptionExt as _};
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};

use crate::{
    data::{DataValue, Frame},
    graph::{Graph, ImmutableGraph, PortOutRef, SimpleGraph, map::NodeMap},
    node::{Ctx, Inputs},
};

impl Graph {
    /// Render a single port at the given [`Ctx`].
    pub fn render(&mut self, port: PortOutRef, ctx: Ctx) -> eyre::Result<DataValue> {
        let graph = SimpleGraph { nodes: &self.nodes };
        render_port(port, ctx, graph, &mut self.node_data)
    }
}

/// Evaluate `port` at `ctx`, recursing lazily into its inputs.
///
/// The current node's `data` slot is temporarily swapped out with a dummy so
/// the input resolver can mutably borrow the rest of `node_data` while the
/// effect mutably holds its own `data`.
pub(crate) fn render_port<G: ImmutableGraph + Copy>(
    port: PortOutRef,
    ctx: Ctx,
    graph: G,
    node_data: &mut NodeMap<Box<dyn Any>>,
) -> eyre::Result<DataValue> {
    let node = port.node();
    let spec = graph.get(node).spec();
    let (_, effect) = spec.outputs()[port.port_index()];

    let mut data = std::mem::replace(&mut node_data[node], Box::new(()));

    let result = {
        let mut resolve = |index: usize, child_ctx: Ctx| -> eyre::Result<DataValue> {
            let input_port = graph.get(node).inputs[index]
                .ok_or_else(|| eyre::eyre!("Input {index} of {node:?} is unset"))?;
            render_port(input_port, child_ctx, graph, node_data)
        };
        let inputs = Inputs::new(&mut resolve);
        effect(inputs, ctx, &mut *data)
    };

    node_data[node] = data;
    result
}

impl Graph {
    /// Render the pipeline at `vframe` for every frame in `[0, len)`, encoding
    /// to `dest_path`.
    ///
    /// `len` and `fps` are queried once with a default [`Ctx`] (they're
    /// expected to be time-invariant).
    pub fn render_as_video(
        &mut self,
        vframe: PortOutRef,
        len: PortOutRef,
        fps: PortOutRef,
        dest_path: &str,
    ) -> eyre::Result<()> {
        let len = i64::try_from(self.render(len, Ctx::default())?)?;
        let fps = f64::try_from(self.render(fps, Ctx::default())?)?;

        let mut process = None::<(FfmpegChild, ChildStdin)>;

        for i in 0..len {
            tracing::info!(len, frame = i, "Rendering");
            let ctx = Ctx {
                frame_index: i as u64,
            };
            let output: Frame = self.render(vframe, ctx)?.try_into()?;

            if let Some((_ffmpeg, stdin)) = process.as_mut() {
                stdin.write_all(output.data())?;
            } else {
                let mut ffmpeg = FfmpegCommand::new()
                    .format("rawvideo")
                    .pix_fmt("rgb24")
                    .size(output.width(), output.height())
                    .rate(fps as f32)
                    .input("pipe:0")
                    .output(dest_path)
                    .codec_video("libx264")
                    .overwrite()
                    .spawn()?;

                let stdin = ffmpeg.take_stdin().ok_or_eyre("Failed to open stdin")?;

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
}
