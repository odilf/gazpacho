mod node;
mod port;

use egui::{Color32, Key, Pos2, Response, Sense, Ui, Widget, ahash::HashMap, lerp};
use gazpacho_core::{
    graph::{GenericPortRef, Graph, NodeRef},
    node::{ALL, NodeSpec},
};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GraphViewState {
    node_positions: HashMap<NodeRef, Pos2>,
    target_view_position: Pos2,
    view_position: Pos2,
    log_zoom: f32,
    target_log_zoom: f32,
    show_picker: bool,
    picker_selection: Option<NodeSpec>,
    dragging_port: Option<GenericPortRef>,
}

impl GraphViewState {
    fn zoom(&self) -> f32 {
        self.log_zoom.exp2()
    }

    pub fn focus(&mut self, node_ref: NodeRef) {
        self.target_view_position = *self.node_positions.get(&node_ref).unwrap();
    }
}

#[derive(Debug)]
pub struct GraphView<'a> {
    state: &'a mut GraphViewState,
    graph: &'a mut Graph,
    selection: &'a mut Option<NodeRef>,
    screen_center: Pos2,
}

impl AppState {
    pub fn graph_view(&mut self) -> GraphView<'_> {
        GraphView {
            state: &mut self.graph_view,
            graph: &mut self.graph,
            selection: &mut self.node_view.selection,
            // Yucky.
            screen_center: Pos2::ZERO,
        }
    }
}

impl<'a> Widget for GraphView<'a> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        self.screen_center = ui.content_rect().center();
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
    fn to_screen_space(&self, world_pos: Pos2) -> Pos2 {
        let center = self.screen_center - self.state.view_position.to_vec2();
        world_pos * self.state.zoom() + center.to_vec2()
    }

    fn to_world_space(&self, screen_pos: Pos2) -> Pos2 {
        let center = self.screen_center - self.state.view_position.to_vec2();
        (screen_pos - center.to_vec2()) / self.state.zoom()
    }

    fn render_grid(&mut self, ui: &mut Ui) {
        let painter = ui.painter();
        let spacing = 40.0;
        let bounds = ui.max_rect();
        let min = self.to_world_space(bounds.left_top());
        let max = self.to_world_space(bounds.right_bottom());

        let min = (min / spacing).floor() * spacing;
        let max = (max / spacing).ceil() * spacing;

        // Casts guaranteed to be exact becase of the rounding above.
        let range = |s, e| (0..((e - s) / spacing) as i32).map(move |i| s + i as f32 * spacing);

        for x in range(min.x, max.x) {
            for y in range(min.y, max.y) {
                let pos = self.to_screen_space(Pos2::new(x, y));
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
            self.target_view_position -= scroll_delta;
            self.target_log_zoom += zoom_delta;

            self.log_zoom = lerp(self.target_log_zoom..=self.log_zoom, 0.5);
            self.view_position = lerp(
                self.target_view_position.to_vec2()..=self.view_position.to_vec2(),
                0.5,
            )
            .to_pos2();
            
            if (self.log_zoom - self.target_log_zoom).abs() > 1e-3
                || self.target_view_position.distance_sq(self.view_position) > 1e-6
            {
                ui.request_repaint();
            }
        }
    }
}
