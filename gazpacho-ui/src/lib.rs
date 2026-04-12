mod view;

use egui::{
    Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Layout, Style, Visuals,
};
use gazpacho_core::graph::Graph;
use serde::{Deserialize, Serialize};

use crate::view::{GraphViewState, NodeViewState, TimelineViewState, View};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    view: View,
    graph: Graph,
    graph_view: GraphViewState,
    node_view: NodeViewState,
    timeline: TimelineViewState,
}

impl AppState {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_style_of(
            egui::Theme::Dark,
            Style {
                visuals: Visuals {
                    panel_fill: Color32::BLACK,
                    menu_corner_radius: CornerRadius::same(0),
                    ..Visuals::dark()
                },
                ..Default::default()
            },
        );

        cc.egui_ctx.set_style_of(
            egui::Theme::Light,
            Style {
                visuals: Visuals { ..Visuals::dark() },
                ..Default::default()
            },
        );

        // TODO: Use more fonts
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "main".to_owned(),
            std::sync::Arc::new(FontData::from_static(include_bytes!(env!(
                "JAMON_MAIN_FONT_PATH"
            )))),
        );

        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .insert(0, "main".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "main".to_owned());

        cc.egui_ctx.set_fonts(fonts);

        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

impl eframe::App for AppState {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let is_web = cfg!(target_arch = "wasm32");
                ui.menu_button("file", |ui| {
                    if !is_web && ui.button("quit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    if ui.button("new").clicked() {
                        *self = AppState::default()
                    }
                });

                ui.add_space(16.0);

                egui::widgets::global_theme_preference_switch(ui);

                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    for (view, name) in [
                        (View::Timeline, "timeline"),
                        (View::Node, "node"),
                        (View::Graph, "graph"),
                    ] {
                        if ui
                            .add_enabled(self.view != view, Button::new(name))
                            .clicked()
                        {
                            self.view = view
                        }
                    }
                })
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            match self.view {
                View::Graph => ui.add(self.graph_view()),
                View::Node => ui.add(self.node_view()),
                _ => todo!(),
            };

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });
        });
    }
}
