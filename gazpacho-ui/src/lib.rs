use std::path::{Path, PathBuf};

use egui::{Color32, FontFamily};
use eyre::WrapErr as _;

pub mod command;
pub mod palette;
pub mod panels;
mod project;
pub mod viewer;

pub use project::Project;

use crate::{
    command::{Command, Pane},
    palette::CommandPalette,
};

pub fn is_gzp(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.to_str() == Some("gzp"))
}

/// The main application struct.
///
/// Persisted parts (layout preferences, recent roots, active file) are
/// serialized via eframe; everything else lives in runtime state.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct App {
    pub recent_project_roots: Vec<PathBuf>,
    pub layout: LayoutState,
    pub focused_pane: Pane,
    pub project: Option<Project>,
    // pub timeline: Timeline,
    pub command_palette: CommandPalette,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> eyre::Result<Self> {
        setup_fonts_and_styles(&cc.egui_ctx)?;

        let app = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();

        Ok(app)
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 1. Keyboard shortcuts
        let mut commands = self.collect_global_keyboard_commands(ui);

        // 2. Draw the shell; panels may emit commands
        commands.extend(panels::show_top_bar(ui, self));
        commands.extend(panels::show_left_sidebar(ui, self));
        commands.extend(panels::show_right_inspector(ui, self));
        // commands.extend(self.timeline.show(ui));
        // commands.extend(self.viewer.show(ui));

        // 3. Command palette (topmost layer)
        self.command_palette.show(&ctx);

        // 4. Dispatch commands
        for cmd in commands {
            self.dispatch(&ctx, cmd);
        }

        // 5. Background work: render preview frame if requested
        // TODO
    }
}

impl App {
    /// Collect keyboard shortcuts that should be handled globally.
    fn collect_global_keyboard_commands(&mut self, ui: &egui::Ui) -> Vec<Command> {
        use egui::Key;

        let mut commands = Vec::new();

        // Don't collect commands while command palette is open.
        if self.command_palette.open {
            return commands;
        }

        // Don't intercept input while typing in a text field.
        if ui.ctx().memory(|m| m.focused()).is_some() {
            return commands;
        }

        let input = ui.input(|i| i.clone());

        if input.modifiers.command && input.key_pressed(Key::P) {
            commands.push(Command::ToggleCommandPalette);
        }
        if input.modifiers.command && input.key_pressed(Key::R) {
            commands.push(Command::ReloadActiveGzp);
        }
        if input.modifiers.command && input.modifiers.shift && input.key_pressed(Key::O) {
            commands.push(Command::RevealActiveFileManager);
        }
        if input.modifiers.command && input.key_pressed(Key::Q) {
            commands.push(Command::Quit);
        }

        // Digit shortcuts for focus
        for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5]
            .into_iter()
            .enumerate()
        {
            if input.key_pressed(key)
                && !input.modifiers.any()
                && let Some(pane) = command::Pane::ALL.get(i)
            {
                commands.push(Command::FocusPane(*pane));
            }
        }

        if input.key_pressed(Key::Tab) {
            if input.modifiers.shift {
                commands.push(Command::FocusPrevPane);
            } else {
                commands.push(Command::FocusNextPane);
            }
        }

        if input.key_pressed(Key::Escape) {
            commands.push(Command::Escape);
        }

        if input.key_pressed(Key::I) && !input.modifiers.any() {
            commands.push(Command::ToggleInspector);
        }

        // Space for play/pause (only when no text editing focus)
        if input.key_pressed(Key::Space) && !input.modifiers.any() {
            commands.push(Command::PlayPause);
        }

        // Frame stepping
        if input.key_pressed(Key::Period) && !input.modifiers.any() {
            commands.push(Command::FrameForward);
        }
        if input.key_pressed(Key::Comma) && !input.modifiers.any() {
            commands.push(Command::FrameBackward);
        }

        // Seek
        if input.key_pressed(Key::Home) {
            commands.push(Command::SeekToStart);
        }
        if input.key_pressed(Key::End) {
            commands.push(Command::SeekToEnd);
        }

        // Zoom
        if input.key_pressed(Key::Equals) || input.key_pressed(Key::Plus) {
            commands.push(Command::TimelineZoomIn);
        }
        if input.key_pressed(Key::Minus) {
            commands.push(Command::TimelineZoomOut);
        }

        commands
    }

    fn dispatch(&mut self, ctx: &egui::Context, cmd: Command) {
        match cmd {
            Command::ToggleCommandPalette => {
                self.command_palette.open = !self.command_palette.open;
                // Query intentionally not cleared.
            }

            Command::FocusPane(pane) => {
                self.focused_pane = pane;
            }

            Command::FocusNextPane => {
                self.focused_pane = self.focused_pane.next();
            }

            Command::FocusPrevPane => {
                self.focused_pane = self.focused_pane.prev();
            }

            Command::Escape => {
                if self.command_palette.open {
                    self.command_palette.open = false;
                    // Query intentionally not cleared.
                }
            }

            Command::ToggleInspector => {
                self.layout.inspector_visible = !self.layout.inspector_visible;
            }

            Command::ToggleLeftSidebar => {
                if self.layout.left_sidebar_width > 10.0 {
                    self.layout.left_sidebar_width = 0.0;
                } else {
                    self.layout.left_sidebar_width = 240.0;
                }
            }

            Command::OpenFolder => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    match Project::new(path) {
                        Ok(proj) => self.project = Some(proj),
                        Err(err) => todo!("Handle errors {err}."),
                    }
                }
            }

            Command::LoadGzpFile(index) => {
                if let Some(proj) = &mut self.project {
                    proj.active = Some(index);
                    todo!("Do the work to activate")
                }
            }

            Command::SelectFile(index) => {
                if let Some(proj) = &mut self.project {
                    proj.selected_file = Some(index);
                }
            }

            Command::ReloadActiveGzp => {
                todo!("Reload active file")
            }

            Command::RevealActiveFileManager => {
                if let Some(proj) = &self.project
                    && let Some(selected) = proj.selected_file
                {
                    let result = open::that_detached(&proj.files[selected].0);
                    if let Err(err) = result {
                        todo!("Handle errors: {err}")
                    }
                }
            }

            Command::CopyActiveFilePath => {
                if let Some(proj) = &self.project
                    && let Some(selected) = proj.selected_file
                {
                    let path = &proj.files[selected].0;
                    ctx.send_cmd(egui::OutputCommand::CopyText(
                        path.to_string_lossy().to_string(),
                    ));
                }
            }

            Command::PlayPause => {
                todo!("play/pause")
            }

            Command::FrameForward => {
                todo!("Frame forward")
            }

            Command::FrameBackward => {
                todo!("Frame backward")
            }

            Command::SeekToStart => {
                todo!("Seek start")
            }

            Command::SeekToEnd => {
                todo!("Seek end")
            }

            Command::TimelineZoomIn => {
                // self.timeline.layout.zoom = (self.timeline.layout.zoom * 1.25).min(100.0);
            }

            Command::TimelineZoomOut => {
                // self.timeline.layout.zoom = (self.timeline.layout.zoom / 1.25).max(0.01);
            }

            Command::ResetLayout => {
                self.layout = Default::default();
            }

            Command::ResetAllState => {
                *self = Self::default();
                ctx.memory_mut(|mem| *mem = egui::Memory::default());
            }

            Command::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn setup_fonts_and_styles(ctx: &egui::Context) -> eyre::Result<()> {
    let mut fonts = egui::FontDefinitions::default();

    ctx.global_style_mut(|style| {
        style.compact_menu_style = true;
        let visuals = &mut style.visuals;
        visuals.extreme_bg_color = Color32::BLACK;
        visuals.window_fill = Color32::BLACK;
        visuals.panel_fill = Color32::BLACK;
        // TODO: Continue setting up styles
    });

    let variants: &[(&str, &str)] = &[
        ("light", "AtkinsonHyperlegibleNext-Light.otf"),
        ("regular", "AtkinsonHyperlegibleNext-Regular.otf"),
        ("medium", "AtkinsonHyperlegibleNext-Medium.otf"),
        ("semibold", "AtkinsonHyperlegibleNext-SemiBold.otf"),
        ("italic", "AtkinsonHyperlegibleNext-Italic.otf"),
    ];

    for (key, file) in variants {
        let font_path = std::env::var("GAZPACHO_FONTS").wrap_err("`GAZPACHO_FONTS` is not set.")?;
        let bytes = std::fs::read(format!("{font_path}/{file}"))
            .wrap_err_with(|| format!("failed to read font file at '{font_path}/{file}'"))?;
        fonts
            .font_data
            .insert((*key).to_owned(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .insert(FontFamily::Name((*key).into()), vec![(*key).to_owned()]);
    }

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "light".to_owned());

    ctx.set_fonts(fonts);

    Ok(())
}
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LayoutState {
    pub left_sidebar_width: f32,
    pub right_sidebar_width: f32,
    pub timeline_height: f32,
    pub inspector_visible: bool,
    pub active_sidebar: Sidebar,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            left_sidebar_width: 240.0,
            right_sidebar_width: 280.0,
            timeline_height: 180.0,
            inspector_visible: false,
            active_sidebar: Sidebar::default(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize,
)]
pub enum Sidebar {
    #[default]
    Files,
    Sources,
    Media,
}
