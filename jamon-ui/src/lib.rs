mod view;

use egui::{FontData, FontDefinitions, FontFamily};
use serde::{Deserialize, Serialize};

use crate::view::{GraphViewState, NodeViewState, TimelineViewState, View};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    view: View,
    graph: GraphViewState,
    node: NodeViewState,
    timeline: TimelineViewState,
}

impl AppState {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        use egui::{Color32, CornerRadius, Shadow, Style, Visuals};

        // Visuals - Dark theme with blue accent
        let mut visuals = Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = Color32::from_gray(27);
        visuals.widgets.inactive.bg_fill = Color32::from_gray(40);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(70, 90, 120);
        visuals.widgets.active.bg_fill = Color32::from_rgb(80, 120, 180);
        visuals.selection.bg_fill = Color32::from_rgb(60, 100, 160).linear_multiply(0.4);
        visuals.window_fill = Color32::from_gray(32);
        visuals.panel_fill = Color32::from_gray(27);
        visuals.window_shadow = Shadow::NONE;
        visuals.menu_corner_radius = CornerRadius::ZERO;

        // Style - Rounded corners and spacing
        let mut style = Style::default();
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.window_margin = 8.0.into();

        cc.egui_ctx.set_global_style(style);

        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "main".to_owned(),
            std::sync::Arc::new(
                // .ttf and .otf supported
                FontData::from_static(include_bytes!(env!("JAMON_MAIN_FONT_PATH"))),
            ),
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
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ui.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            ui.horizontal(|ui| {
                ui.label("Write something: (not!!!!)");
                // ui.text_edit_singleline(&mut self.label);
            });

            // ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            // if ui.button("Increment").clicked() {
            //     self.value += 1.0;
            // }

            ui.separator();

            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/main/",
                "Source code."
            ));

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
