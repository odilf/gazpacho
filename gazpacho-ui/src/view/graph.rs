use egui::{
    Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
    Widget,
    epaint::{CircleShape, CubicBezierShape, PathStroke},
};
use gazpacho_core::{
    data::{DataType, SimpleDataType},
    graph::{GenericPortRef, Graph, InputPort, InputValue, NodeRef, OutputPort, PortRef, PortType},
    node::{ALL, NodeDescriptor},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GraphViewState {
    graph: Graph<NodeMeta>,
    view_position: Pos2,
    scroll_velocity: Vec2,
    log_zoom: f32,
    zoom_speed: f32,
    show_picker: bool,
    picker_selection: Option<NodeDescriptor>,
    dragging_port: Option<GenericPortRef>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct NodeMeta {
    /// World position of node in graph.
    position: Pos2,
}

impl GraphViewState {
    fn zoom(&self) -> f32 {
        self.log_zoom.exp2()
    }

    fn to_screen_space(&self, ui: &Ui, world_pos: Pos2) -> Pos2 {
        let center = ui.max_rect().center() + self.view_position.to_vec2();
        world_pos * self.zoom() + center.to_vec2()
    }

    fn to_world_space(&self, ui: &Ui, screen_pos: Pos2) -> Pos2 {
        let center = ui.max_rect().center() + self.view_position.to_vec2();
        (screen_pos - center.to_vec2()) / self.zoom()
    }

    pub fn insert(&mut self, node: &'static NodeDescriptor) -> NodeRef {
        self.graph.insert_node_with_meta(
            node,
            NodeMeta {
                // TODO: Automatically find good place to place node
                position: Pos2::ZERO,
            },
        )
    }
}

impl Widget for &mut GraphViewState {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.render_grid(ui);
        self.render_picker(ui);
        self.render_nodes(ui);

        let response = ui.response().interact(Sense::all());
        response.context_menu(|ui| {
            ui.menu_button("Add node", |ui| {
                for node in ALL.values() {
                    if ui.button(node.id().to_string()).clicked() {
                        self.insert(node);
                    }
                }
            });
        });

        self.navigate(ui);

        response
    }
}

impl GraphViewState {
    fn node_rect(&self, node_ref: NodeRef, ui: &Ui) -> Rect {
        let world_size = Vec2::new(120.0, 80.0);
        let node = self.graph.get(node_ref);
        let screen_pos = self.to_screen_space(ui, node.metadata.position);
        Rect::from_center_size(screen_pos, world_size * self.zoom())
    }

    fn render_node(&mut self, node_ref: NodeRef, ui: &mut egui::Ui) {
        let node = self.graph.get(node_ref);
        let rect = self.node_rect(node_ref, ui);
        if !ui.is_rect_visible(rect) {
            return;
        }

        let response = ui.allocate_rect(rect, Sense::all());
        let hovered = response.hovered();
        let pressed = response.is_pointer_button_down_on();

        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(3),
            Color32::from_rgb(40, 50, 60),
            Stroke::new(
                if hovered { 1.0 } else { 0.0 },
                Color32::from_gray(if pressed { 180 } else { 100 }),
            ),
            StrokeKind::Middle,
        );

        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            node.descriptor().id(),
            FontId::proportional(14.0),
            Color32::from_gray(200),
        );

        self.render_ports::<InputPort>(node_ref, ui);
        self.render_ports::<OutputPort>(node_ref, ui);

        // Interactions
        self.graph.get_meta_mut(node_ref).position =
            self.to_world_space(ui, rect.center() + response.drag_delta());

        // Re-borrow after mutating.
        let node = self.graph.get(node_ref);
        response.on_hover_ui_at_pointer(|ui| {
            ui.label(node.descriptor().id().to_string());
        });
    }

    fn port_position<T: PortType>(&self, ui: &Ui, port: PortRef<T>) -> Pos2 {
        let rect = self.node_rect(port.node(), ui);
        let spacing =
            rect.width() / (self.graph.port_refs::<T>(port.node()).len() + 1) as f32 * Vec2::X;
        let start = if T::IS_INPUT {
            rect.left_top()
        } else {
            rect.left_bottom()
        };

        start + (port.port_index() + 1) as f32 * spacing
    }

    fn render_ports<T: PortType>(&mut self, node_ref: NodeRef, ui: &mut Ui) {
        let refs = self.graph.port_refs::<T>(node_ref);

        let painter = ui.painter();
        let bezier = |start: Pos2, end: Pos2| {
            let h = (end.y - start.y) / 2.0;
            CubicBezierShape {
                points: [start, start + Vec2::Y * h, end - Vec2::Y * h, end],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: PathStroke::new(5.0, Color32::from_rgb(100, 130, 120)),
            }
        };

        if !T::IS_INPUT {
            for (in_port, value) in self.graph.get(node_ref).inputs() {
                match value {
                    None => (),
                    Some(InputValue::Port(out_port)) => {
                        painter.add(bezier(
                            self.port_position(ui, in_port),
                            self.port_position(ui, out_port),
                        ));
                    }
                    Some(InputValue::Const(port)) => todo!(),
                }
            }
        }

        for port_ref in refs.collect::<Box<[_]>>() {
            let port = self.graph.get_port(port_ref);
            let r = 8.0;
            let color = Self::type_color(port.typ());
            let pos = self.port_position(ui, port_ref);
            let mut circle = CircleShape::filled(pos, r, color);

            let response = ui.allocate_rect(circle.visual_bounding_rect(), Sense::click_and_drag());

            // `contains_pointer` instead of `hovered` because of drag and drop.
            if response.contains_pointer() {
                circle.stroke = Stroke {
                    width: 1.0,
                    color: color.gamma_multiply(2.0),
                };

                if let Some(dragged_port) = self.dragging_port
                    && let Some((input, output)) =
                        GenericPortRef::input_output(port_ref, dragged_port)
                {
                    self.graph.connect(output, input);
                }
            } else if let Some(dragged_port) = self.dragging_port
                && let Some((input, output)) = GenericPortRef::input_output(port_ref, dragged_port)
                && self.graph.is_connected(output, input)
            {
                self.graph.disconnect(output, input);
            }

            let painter = ui.painter();
            if self.dragging_port == Some(port_ref.as_generic()) {
                if let Some(mouse_pos) = ui.input(|i| i.pointer.latest_pos()) {
                    painter.add(bezier(pos, mouse_pos));
                }
            }

            painter.add(circle);

            if response.dragged() {
                self.dragging_port = Some(port_ref.as_generic());
            }

            if response.drag_stopped() {
                self.dragging_port = None;
            }

            if let Some(port) = response.dnd_hover_payload::<GenericPortRef>() {
                panic!("{port:?}")
            }
        }
    }

    fn type_color(typ: DataType) -> Color32 {
        match typ {
            DataType::Simple(SimpleDataType::Int) => Color32::GREEN,
            DataType::Simple(SimpleDataType::Float) => Color32::DARK_GREEN,
            DataType::Simple(SimpleDataType::String) => Color32::BLUE,
            DataType::Simple(SimpleDataType::VideoFrame) => Color32::RED,
            DataType::Track(_) => Color32::DARK_RED,
        }
    }

    fn render_nodes(&mut self, ui: &mut Ui) {
        for node_ref in self.graph.node_refs() {
            self.render_node(node_ref, ui);
        }
    }

    fn render_grid(&mut self, ui: &mut Ui) {
        let painter = ui.painter();
        let spacing = 40.0;
        let bounds = ui.max_rect();
        let min = self.to_world_space(ui, bounds.left_top());
        let max = self.to_world_space(ui, bounds.right_bottom());

        let min = (min / spacing).floor() * spacing;
        let max = (max / spacing).ceil() * spacing;

        // Casts guaranteed to be exact becase of the rounding above.
        let range = |s, e| (0..((e - s) / spacing) as i32).map(move |i| s + i as f32 * spacing);

        for x in range(min.x, max.x) {
            for y in range(min.y, max.y) {
                let pos = self.to_screen_space(ui, Pos2::new(x, y));
                painter.circle_filled(pos, 1.0, Color32::from_white_alpha(60));
            }
        }
    }

    fn render_picker(&mut self, ui: &mut Ui) {
        if ui.input(|state| state.modifiers.shift && state.key_pressed(Key::A)) {
            self.show_picker = !self.show_picker;
        }
        if ui.input(|state| state.key_pressed(Key::Escape)) {
            self.show_picker = false;
        }

        if self.show_picker {
            let before = self.picker_selection;
            egui::ComboBox::from_label("Select one!")
                .selected_text(format!("{:?}", self.picker_selection))
                .show_ui(ui, |ui| {
                    for node in ALL.values() {
                        ui.selectable_value(
                            &mut self.picker_selection,
                            Some(*node),
                            node.id().to_string(),
                        );
                    }
                });

            if self.picker_selection != before {
                dbg!(self.picker_selection);
            }
        }
    }

    fn navigate(&mut self, ui: &mut Ui) {
        if ui.rect_contains_pointer(ui.max_rect()) {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta());
            let zoom_delta = ui.input(|i| i.zoom_delta()).log2();
            self.scroll_velocity += scroll_delta;
            self.scroll_velocity *= 0.8;
            self.view_position += self.scroll_velocity;

            self.zoom_speed += zoom_delta;
            self.zoom_speed *= 0.6;
            self.log_zoom += self.zoom_speed;

            if self.zoom_speed.abs() > 1e-3 {
                ui.request_repaint();
            } else {
                self.zoom_speed = 0.0;
            }

            if self.scroll_velocity.length_sq() > 1e-6 {
                ui.request_repaint();
            } else {
                self.scroll_velocity = Vec2::ZERO;
            }
        }
    }
}
