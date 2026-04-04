use egui::{
    Align2, Color32, CornerRadius, FontId, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind,
    Ui, Vec2,
    epaint::{CubicBezierShape, PathStroke},
};
use gazpacho_core::graph::{InputPort, NodeRef, OutputPort, PortType};

use crate::view::graph::GraphView;

impl GraphView<'_> {
    pub(super) fn render_node(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        let rect = self.node_rect(ui, node_ref);
        let response = ui.allocate_rect(rect, Sense::all());
        let selected = Some(node_ref) == *self.selection;

        let node = self.graph.get(node_ref);
        if !ui.is_rect_visible(rect) {
            return;
        }

        let hovered = response.hovered();
        let pressed = response.is_pointer_button_down_on();

        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(3),
            Color32::from_rgb(40, 50, 60),
            Stroke::new(
                if selected {
                    2.0
                } else if hovered {
                    1.0
                } else {
                    0.0
                },
                Color32::from_gray(if selected || pressed { 180 } else { 100 }),
            ),
            StrokeKind::Middle,
        );

        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            node.spec().id(),
            FontId::proportional(14.0),
            Color32::from_gray(200),
        );

        // Interactions
        *self.state.node_positions.get_mut(&node_ref).unwrap() = self
            .state
            .to_world_space(ui, rect.center() + response.drag_delta());

        if response.double_clicked_by(PointerButton::Primary) {
            *self.selection = Some(node_ref);
        }

        // Ports
        self.render_node_connections(ui, node_ref);
        self.render_node_ports::<InputPort>(ui, node_ref);
        self.render_node_ports::<OutputPort>(ui, node_ref);
    }

    pub(super) fn node_rect(&self, ui: &Ui, node_ref: NodeRef) -> Rect {
        let world_size = Vec2::new(120.0, 80.0);
        let screen_pos = self
            .state
            .to_screen_space(ui, *self.state.node_positions.get(&node_ref).unwrap());

        Rect::from_center_size(screen_pos, world_size * self.state.zoom())
    }

    fn render_node_connections(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        for (in_port, out_port) in self.graph.get(node_ref).inputs() {
            let Some(out_port) = out_port else {
                continue;
            };

            ui.painter().add(connection_bezier(
                self.port_position(ui, in_port),
                self.port_position(ui, out_port),
            ));
        }
    }

    fn render_node_ports<T: PortType>(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        let refs = self.graph.port_refs::<T>(node_ref);
        for port_ref in refs.collect::<Box<[_]>>() {
            self.render_port(ui, port_ref);
        }
    }
}

pub(super) fn connection_bezier(start: Pos2, end: Pos2) -> CubicBezierShape {
    let h = (end.y - start.y) / 2.0;
    CubicBezierShape {
        points: [start, start + Vec2::Y * h, end - Vec2::Y * h, end],
        closed: false,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::new(5.0, Color32::from_rgb(100, 130, 120)),
    }
}
