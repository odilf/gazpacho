mod node;
mod port;

use egui::{Color32, Key, Pos2, Response, Sense, Ui, Vec2, Widget, ahash::HashMap};
use gazpacho_core::{
    graph::{GenericPortRef, Graph, NodeRef},
    node::{ALL, NodeSpec},
};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GraphViewState {
    node_positions: HashMap<NodeRef, Pos2>,
    view_position: Pos2,
    scroll_velocity: Vec2,
    log_zoom: f32,
    zoom_speed: f32,
    show_picker: bool,
    picker_selection: Option<NodeSpec>,
    dragging_port: Option<GenericPortRef>,
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

    pub fn focus(&mut self, node_ref: NodeRef) {
        self.view_position = *self.node_positions.get(&node_ref).unwrap();
    }
}

#[derive(Debug)]
pub struct GraphView<'a> {
    state: &'a mut GraphViewState,
    graph: &'a mut Graph,
    selection: &'a mut Option<NodeRef>,
}

impl AppState {
    pub fn graph_view(&mut self) -> GraphView<'_> {
        GraphView {
            state: &mut self.graph_view,
            graph: &mut self.graph,
            selection: &mut self.node_view.selection,
        }
    }
}

impl<'a> Widget for GraphView<'a> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        self.render_grid(ui);
        self.render_picker(ui);
        self.render_nodes(ui);

        let response = ui.response().interact(Sense::all());
        response.context_menu(|ui| {
            ui.menu_button("Add node", |ui| {
                for node in ALL.values() {
                    if ui.button(node.id().to_string()).clicked() {
                        let node_ref = self.graph.insert_node(node);
                        // TODO: Auto find good position to place node
                        self.state.node_positions.insert(node_ref, Pos2::ZERO);
                    }
                }
            });
        });

        if response.double_clicked_by(egui::PointerButton::Primary) {
            *self.selection = None
        }

        self.state.navigate(ui);

        response
    }
}

impl GraphView<'_> {
    fn render_grid(&mut self, ui: &mut Ui) {
        let painter = ui.painter();
        let spacing = 40.0;
        let bounds = ui.max_rect();
        let min = self.state.to_world_space(ui, bounds.left_top());
        let max = self.state.to_world_space(ui, bounds.right_bottom());

        let min = (min / spacing).floor() * spacing;
        let max = (max / spacing).ceil() * spacing;

        // Casts guaranteed to be exact becase of the rounding above.
        let range = |s, e| (0..((e - s) / spacing) as i32).map(move |i| s + i as f32 * spacing);

        for x in range(min.x, max.x) {
            for y in range(min.y, max.y) {
                let pos = self.state.to_screen_space(ui, Pos2::new(x, y));
                painter.circle_filled(pos, 1.0, Color32::from_white_alpha(60));
            }
        }
    }

    fn render_picker(&mut self, ui: &mut Ui) {
        let state = &mut self.state;
        if ui.input(|state| state.modifiers.shift && state.key_pressed(Key::A)) {
            state.show_picker = !state.show_picker;
        }
        if ui.input(|state| state.key_pressed(Key::Escape)) {
            state.show_picker = false;
        }

        if state.show_picker {
            let before = state.picker_selection;
            egui::ComboBox::from_label("Select one!")
                .selected_text(format!("{:?}", state.picker_selection))
                .show_ui(ui, |ui| {
                    for node in ALL.values() {
                        ui.selectable_value(
                            &mut state.picker_selection,
                            Some(*node),
                            node.id().to_string(),
                        );
                    }
                });

            if state.picker_selection != before {
                dbg!(state.picker_selection);
            }
        }
    }

    fn render_nodes(&mut self, ui: &mut Ui) {
        for node_ref in self.graph.node_refs() {
            self.render_node(ui, node_ref);
        }
    }
}

impl GraphViewState {
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
