use egui::{FontFamily, FontId, TextStyle};
use eyre::WrapErr as _;
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct App {}

impl Default for App {
    fn default() -> Self {
        Self {}
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> eyre::Result<Self> {
        setup_fonts_and_styles(&cc.egui_ctx)?;

        let app = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };

        Ok(app)
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

fn setup_fonts_and_styles(ctx: &egui::Context) -> eyre::Result<()> {
    let mut fonts = egui::FontDefinitions::default();

    let variants: &[(&str, &str)] = &[
        ("iosevka-thin", "IosevkaTermNerdFontPropo-Thin.ttf"),
        ("iosevka-light", "IosevkaTermNerdFontPropo-Light.ttf"),
        ("iosevka-regular", "IosevkaTermNerdFontPropo-Regular.ttf"),
        ("iosevka-medium", "IosevkaTermNerdFontPropo-Medium.ttf"),
        ("iosevka-semibold", "IosevkaTermNerdFontPropo-SemiBold.ttf"),
        ("iosevka-italic", "IosevkaTermNerdFontPropo-LightItalic.ttf"),
    ];

    for (key, file) in variants {
        let font_path = std::env::var("GAZPACHO_FONTS").wrap_err("`GAZPACHO_FONTS` is not set.")?;
        let bytes =
            std::fs::read(format!("{font_path}/{file}")).wrap_err("failed to read font file")?;
        fonts
            .font_data
            .insert((*key).to_owned(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .insert(FontFamily::Name((*key).into()), vec![(*key).to_owned()]);
    }

    // Make regular weight the fallback for the built-in families too,
    // in case anything falls back to Proportional/Monospace directly.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "iosevka-regular".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "iosevka-regular".to_owned());

    ctx.set_fonts(fonts);

    // --- This is the part that makes it automatic ---
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = BTreeMap::from([
        (
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Name("iosevka-semibold".into())),
        ),
        (
            TextStyle::Body,
            FontId::new(14.0, FontFamily::Name("iosevka-regular".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Name("iosevka-regular".into())),
        ),
        (
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Name("iosevka-medium".into())),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Name("iosevka-light".into())),
        ),
    ]);
    ctx.set_global_style(style);

    Ok(())
}
