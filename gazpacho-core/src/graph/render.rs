use std::{any::Any, io::Write as _, process::ChildStdin};

use color_eyre::eyre::{self, Context as _, OptionExt as _};
use ffmpeg_sidecar::{child::FfmpegChild, command::FfmpegCommand};

use crate::{
    data::{DataType, DataValue, Frame},
    graph::{Graph, ImmutableGraph, PortOutRef, map::NodeMap, node_instance::NodeRef},
};

fn get_port_order(
    ports: impl IntoIterator<Item = PortOutRef>,
    graph: &impl ImmutableGraph,
) -> Vec<PortOutRef> {
    let mut port_order = Vec::new();

    for port in ports {
        populate_port_order(port, graph, &mut port_order);
    }

    port_order
}

fn populate_port_order(
    port: PortOutRef,
    graph: &impl ImmutableGraph,
    port_order: &mut Vec<PortOutRef>,
) {
    port_order.push(port);
    for &source in &graph.get(port.node()).inputs {
        let Some(source) = source else {
            continue;
        };

        populate_port_order(source, graph, port_order);
    }
}

impl Graph {
    pub fn render(&mut self, port: PortOutRef) -> eyre::Result<DataValue> {
        let [value] = self.render_many([port])?;
        Ok(value)
    }

    pub fn render_many<const N: usize>(
        &mut self,
        ports: [PortOutRef; N],
    ) -> eyre::Result<[DataValue; N]> {
        let mut port_order = get_port_order(ports, self);
        let (graph, computed, node_data) = self.split();

        while let Some(port) = port_order.pop() {
            if computed[port].is_none() {
                let computed_borrow = &*computed;
                let rendered = render_port(port, &graph, node_data, |port| {
                    computed_borrow[port]
                        .as_ref()
                        .expect("Values are cached because of `port_order`.")
                })
                .unwrap();

                computed[port] = Some(rendered)
            }
        }

        Ok(ports.map(|port| {
            self.computed_values[port]
                .take() // TODO: Maybe don't take?
                .unwrap()
        }))
    }
}

pub fn render_port<'a>(
    port: PortOutRef,
    graph: &impl ImmutableGraph,
    node_data: &'a mut NodeMap<Box<dyn Any>>,
    get_computed: impl Fn(PortOutRef) -> &'a DataValue,
) -> eyre::Result<DataValue> {
    let node = graph.get(port.node());
    let spec = node.spec();

    let len_ref = spec.inputs_ref().len();
    let mut input_values_ref = Vec::with_capacity(len_ref);
    let mut input_values_own = Vec::with_capacity(spec.inputs_own().len());

    for &input in &node.inputs[..len_ref] {
        let Some(input) = input else {
            eyre::bail!("Input was unset")
        };

        input_values_ref.push(get_computed(input));
    }

    for &input in &node.inputs[len_ref..] {
        let Some(input) = input else {
            eyre::bail!("Input was unset")
        };

        input_values_own.push(get_computed(input).clone());
    }

    let data: &mut dyn Any = node_data[port.node()].as_mut();

    let (_port, effect_fn) = spec.outputs()[port.port_index()];

    effect_fn(&input_values_ref, input_values_own.into_boxed_slice(), data)
}

impl Graph {
    pub fn render_as_video(&mut self, output: NodeRef, dest_path: &str) -> eyre::Result<()> {
        let fps_port = self
            .transitive_named_port_ref(output, "fps")
            .wrap_err("Couldn't find `fps` output port on output node.")?;
        let len_port = self
            .transitive_named_port_ref(output, "len")
            .wrap_err("Couldn't find `len` output port on output node.")?;
        let frame_index_port = self
            .transitive_named_port_ref(output, "frame-index")
            .wrap_err("Couldn't find frame index port")?;

        if self.connection(frame_index_port).is_some() {
            eyre::bail!("Frame index port is not disconnected");
        }

        let frame_index_node = self.set_const_input(frame_index_port, 0).node();

        let [fps, len] = self.render_many([fps_port, len_port])?;
        let len = i64::try_from(len)?;
        let fps = f64::try_from(fps)?;

        // TODO: Should this be transitive too?
        let output_port: PortOutRef = self
            .get(output)
            .typed_port_ref(DataType::vframe())
            .ok_or_eyre("Couldn't find output port of type `Frame`")?;

        let mut process = None::<(FfmpegChild, ChildStdin)>;

        for i in 0..len {
            tracing::info!(len, frame = i, "Rendering");
            self.set_const(frame_index_node, i.into())?;
            let output: Frame = self.render(output_port)?.clone().try_into().unwrap();

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
