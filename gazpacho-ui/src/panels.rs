use egui::{Button, Color32, Panel, RichText, Vec2};

use crate::command::{Command, Pane};
use crate::project::FileData;
use crate::{App, Project, Sidebar, is_gzp};

pub fn show_top_bar(ui: &mut egui::Ui, state: &mut App) -> Option<Command> {
    let mut cmd = None;
    Panel::top("top_bar").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open Project...").clicked() {
                    cmd = Some(Command::OpenFolder);
                }
                ui.separator();
                if ui.button("Reload .gzp").clicked() {
                    cmd = Some(Command::ReloadActiveGzp);
                }
                if ui.button("Reveal in Finder").clicked() {
                    cmd = Some(Command::RevealActiveFileManager);
                }
                ui.separator();
                if ui.button("Reset All State").clicked() {
                    cmd = Some(Command::ResetAllState);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    cmd = Some(Command::Quit);
                }
            });

            ui.menu_button("View", |ui| {
                // XXX: Check this
                if ui.checkbox(&mut false, "Toggle Inspector").clicked() {
                    cmd = Some(Command::ToggleInspector);
                }
                if ui.button("Toggle Left Sidebar").clicked() {
                    cmd = Some(Command::ToggleLeftSidebar);
                }
                ui.separator();
                if ui.button("Reset Layout").clicked() {
                    cmd = Some(Command::ResetLayout);
                }
            });

            ui.menu_button("Focus", |ui| {
                for &pane in Pane::ALL {
                    if ui.button(pane.label()).clicked() {
                        cmd = Some(Command::FocusPane(pane));
                    }
                }
            });

            ui.add_space(16.0);

            // Project status
            if let Some(proj) = &state.project
                && let Some(selected) = proj.selected_file
            {
                let (path, _data) = &proj.files[selected];
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                ui.label(
                    RichText::new(name)
                        .color(ui.visuals().weak_text_color())
                        .small(),
                );
            }

            // Render status indicator
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Here goes status text")
                        .color(ui.visuals().warn_fg_color)
                        .small(),
                );
            });
        });
    });
    cmd
}

pub fn show_left_sidebar(ui: &mut egui::Ui, state: &mut App) -> Option<Command> {
    let width = state.layout.left_sidebar_width;
    let panel = if width < 10.0 {
        Panel::left("left_sidebar").exact_size(1.0).resizable(false)
    } else {
        Panel::left("left_sidebar")
            .default_size(width)
            .min_size(160.0)
    }
    .frame(egui::Frame::default().inner_margin(0));

    panel
        .show(ui, |ui| {
            // Tab bar
            ui.horizontal(|ui| ui.label("TODO: Sidebar buttons"));
            ui.separator();

            match state.layout.active_sidebar {
                Sidebar::Files => show_file_sidebar(ui, state.project.as_ref()),
                Sidebar::Sources => show_sources_sidebar(ui, state.project.as_ref()),
                Sidebar::Media => show_media_sidebar(ui, state.project.as_ref()),
            }
        })
        .inner
}

fn show_file_sidebar(ui: &mut egui::Ui, project: Option<&Project>) -> Option<Command> {
    let Some(proj) = &project else {
        ui.label(
            RichText::new("No project open")
                .color(ui.visuals().weak_text_color())
                .italics(),
        );

        ui.add_space(4.0);

        if ui.button("Open Folder...").clicked() {
            return Some(Command::OpenFolder);
        }

        return None;
    };

    ui.label(
        RichText::new(proj.root.to_string_lossy())
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(4.0);

    show_filetree(ui, proj)
}

fn show_filetree(ui: &mut egui::Ui, project: &Project) -> Option<Command> {
    let mut cmd = None;

    ui.scope_builder(egui::UiBuilder::new(), |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        for (index, (path, data)) in project.files.iter().enumerate() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let mut text = RichText::new(&name);

            let is_active = project.selected_file.is_some_and(|sel| sel == index);
            if is_active {
                // TODO(theme):
                text = text.strong().background_color(Color32::DARK_RED)
            }

            match data {
                None => text = text.weak(),
                // TODO(theme):
                Some(FileData::Media(_)) => text = text.background_color(Color32::DARK_BLUE),
                _ => (),
            }

            let response = ui.add(
                Button::selectable(is_active, text)
                    .min_size(Vec2::new(
                        ui.available_width(),
                        ui.spacing().interact_size.y,
                    ))
                    .corner_radius(egui::CornerRadius::ZERO),
            );

            // let response = ui.selectable_label(is_active, text);
            if response.clicked() {
                if is_gzp(path) && ui.input(|input| input.modifiers.shift) {
                    debug_assert!(cmd.is_none());
                    cmd = Some(Command::LoadGzpFile(index));
                } else {
                    debug_assert!(cmd.is_none());
                    cmd = Some(Command::SelectFile(index));
                }
            }

            // Context menu
            response.context_menu(|ui| {
                // TODO: Say finder/file manager depending on os?
                if ui.button("Reveal in file manager").clicked() {
                    debug_assert!(cmd.is_none());
                    cmd = Some(Command::RevealActiveFileManager);
                }
                if ui.button("Copy Path").clicked() {
                    debug_assert!(cmd.is_none());
                    cmd = Some(Command::CopyActiveFilePath);
                }
            });
        }

        cmd
    })
    .inner
}

fn show_media_sidebar(ui: &mut egui::Ui, project: Option<&Project>) -> Option<Command> {
    let Some(project) = project else {
        ui.label("TODO: media sidebar for empty project");
        return None;
    };

    ui.label(format!(
        "TODO: media sidebar for {}",
        project.root.display()
    ));
    None
}

fn show_sources_sidebar(ui: &mut egui::Ui, project: Option<&Project>) -> Option<Command> {
    let Some(project) = project else {
        ui.label("TODO: sources sidebar for empty project");
        return None;
    };

    ui.label(format!(
        "TODO: sources sidebar for {}",
        project.root.display()
    ));
    None
}

pub fn show_right_inspector(ui: &mut egui::Ui, state: &mut App) -> Option<Command> {
    if !state.layout.inspector_visible {
        return None;
    }

    let width = state.layout.right_sidebar_width;
    Panel::right("right_inspector")
        .default_size(width)
        .min_size(200.0)
        .show(ui, |ui| {
            ui.heading("Inspector");
            ui.separator();

            let Some(proj) = &state.project else { return };

            proj.show_inspector(ui);
        });

    None
}
