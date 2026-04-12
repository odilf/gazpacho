use egui::{Color32, Pos2, Response, Sense, Stroke, Ui, Vec2, epaint::CircleShape};
use gazpacho_core::{
    data::{DataType, SimpleDataType},
    graph::{GenericPortRef, ImmutableGraph as _, PortRef, PortType},
};

use crate::view::graph::{GraphView, node::connection_bezier};

impl GraphView<'_> {
    pub(super) fn render_port<T: PortType>(
        &mut self,
        ui: &mut Ui,
        port_ref: PortRef<T>,
    ) -> Response {
        let pos = self.port_position(port_ref);

        let port = self.graph.get_port(port_ref);
        let r = 8.0;
        let color = datatype_color(port.typ());
        let mut circle = CircleShape::filled(pos, r, color);

        let response = ui.allocate_rect(circle.visual_bounding_rect(), Sense::click_and_drag());

        // `contains_pointer` instead of `hovered` because of drag and drop.
        if response.contains_pointer() {
            circle.stroke = Stroke {
                width: 1.0,
                color: color.gamma_multiply(2.0),
            };

            if let Some(dragged_port) = self.state.dragging_port
                && let Some((input, output)) = GenericPortRef::input_output(port_ref, dragged_port)
            {
                self.graph.connect(output, input);
            }
        } else if let Some(dragged_port) = self.state.dragging_port
            && let Some((input, output)) = GenericPortRef::input_output(port_ref, dragged_port)
            && self.graph.is_connected(output, input)
        {
            self.graph.disconnect(output, input);
        }

        let painter = ui.painter();
        if self.state.dragging_port == Some(port_ref.as_generic())
            && let Some(mouse_pos) = ui.input(|i| i.pointer.latest_pos())
        {
            painter.add(connection_bezier(pos, mouse_pos));
        }

        painter.add(circle);

        if response.dragged() {
            self.state.dragging_port = Some(port_ref.as_generic());
        }

        if response.drag_stopped() {
            self.state.dragging_port = None;
        }

        response
    }

    pub(super) fn port_position<T: PortType>(&self, port_ref: PortRef<T>) -> Pos2 {
        let rect = self.node_rect(port_ref.node());
        let spacing =
            rect.width() / (self.graph.port_refs::<T>(port_ref.node()).len() + 1) as f32 * Vec2::X;
        let start = if T::IS_INPUT {
            rect.left_top()
        } else {
            rect.left_bottom()
        };

        start + (port_ref.port_index() + 1) as f32 * spacing
    }
}

fn datatype_color(typ: DataType) -> Color32 {
    match typ {
        DataType::Simple(SimpleDataType::Int) => Color32::GREEN,
        DataType::Simple(SimpleDataType::Float) => Color32::DARK_GREEN,
        DataType::Simple(SimpleDataType::String) => Color32::BLUE,
        DataType::Simple(SimpleDataType::VideoFrame) => Color32::RED,
        DataType::Simple(SimpleDataType::Any) => Color32::GRAY,
        DataType::Node => Color32::DARK_RED,
    }
}
