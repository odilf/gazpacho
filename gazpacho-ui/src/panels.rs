use egui::{self, Panel, RichText};
use std::path::PathBuf;

use crate::App;
use crate::command::{Command, Pane};
use crate::state::{Selection, Sidebar};

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
            if let Some(ref gzp) = state.project.active_gzp {
                let name = gzp
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
    let mut cmd = None;
    let width = state.layout.left_sidebar_width;
    let panel = if width < 10.0 {
        Panel::left("left_sidebar").exact_size(1.0).resizable(false)
    } else {
        Panel::left("left_sidebar")
            .default_size(width)
            .min_size(160.0)
    };
    panel.show(ui, |ui| {
        // Tab bar
        ui.horizontal(|ui| {
            let project_selected = state.layout.active_sidebar == Sidebar::Project;
            if ui.selectable_label(project_selected, "Project").clicked() {
                state.layout.active_sidebar = Sidebar::Project;
            }
            let media_selected = state.layout.active_sidebar == Sidebar::Media;
            if ui.selectable_label(media_selected, "Media").clicked() {
                state.layout.active_sidebar = Sidebar::Media;
            }
        });
        ui.separator();

        match state.layout.active_sidebar {
            Sidebar::Project => {
                cmd = show_project_tab(ui, state);
            }
            Sidebar::Media => {
                show_media_tab(ui, state);
            }
        }
    });
    cmd
}

fn show_project_tab(ui: &mut egui::Ui, state: &mut App) -> Option<Command> {
    let mut cmd = None;

    if ui.button("Open Folder...").clicked() {
        cmd = Some(Command::OpenFolder);
    }
    ui.add_space(4.0);

    if let Some(ref root) = state.project.root.clone() {
        ui.label(
            RichText::new(root.display().to_string())
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(4.0);

        // File tree
        if let Ok(entries) = std::fs::read_dir(root) {
            let mut gzp_files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "gzp"))
                .collect();
            gzp_files.sort();

            for (i, path) in gzp_files.iter().enumerate() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let is_active = state.project.active_gzp.as_ref().is_some_and(|a| a == path);
                let is_selected = state.selection == Some(Selection::ProjectFile(i));

                let text = if is_active {
                    RichText::new(format!("* {name}")).strong()
                } else {
                    RichText::new(&name)
                };

                let response = ui.selectable_label(is_selected || is_active, text);
                if response.clicked() {
                    state.selection = Some(Selection::ProjectFile(i));
                    cmd = Some(Command::OpenGzpFile(path.clone()));
                }

                // Context menu
                response.context_menu(|ui| {
                    // TODO: Say finder/file manager depending on os?
                    if ui.button("Reveal in file manager").clicked() {
                        cmd = Some(Command::RevealActiveFileManager);
                    }
                    if ui.button("Copy Path").clicked() {
                        cmd = Some(Command::CopyActiveFilePath);
                    }
                });

                // Error badge
                if is_active && !state.project.diagnostics.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        let count = state.project.diagnostics.len();
                        ui.label(
                            RichText::new(format!("{count} error(s)"))
                                .color(ui.visuals().error_fg_color)
                                .small(),
                        );
                    });
                }
            }
        }
    } else {
        ui.label(
            RichText::new("No project open")
                .color(ui.visuals().weak_text_color())
                .italics(),
        );
    }

    cmd
}

fn show_media_tab(ui: &mut egui::Ui, state: &mut App) {
    if state.media_items.is_empty() {
        ui.label(
            RichText::new("No media loaded")
                .color(ui.visuals().weak_text_color())
                .italics(),
        );
        return;
    }

    for (i, item) in state.media_items.iter().enumerate() {
        let is_selected = state.selection == Some(Selection::MediaItem(i));
        let name = std::path::Path::new(&item.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| item.path.clone());

        let response = ui.selectable_label(is_selected, &name);
        if response.clicked() {
            state.selection = Some(Selection::MediaItem(i));
        }

        // Details on hover or when selected
        if response.hovered() || is_selected {
            ui.indent(format!("media_detail_{i}"), |ui| {
                if let Some(res) = item.resolution {
                    ui.label(RichText::new(format!("{}x{}", res.width, res.height)).small());
                }
                if let Some(dur) = item.duration {
                    ui.label(RichText::new(format!("Duration: {dur}")).small());
                }
                if let Some(fps) = item.fps {
                    ui.label(RichText::new(format!("FPS: {fps}")).small());
                }
                if !item.available {
                    ui.label(
                        RichText::new("Unavailable")
                            .color(ui.visuals().error_fg_color)
                            .small(),
                    );
                }
                if let Some(ref err) = item.error {
                    ui.label(
                        RichText::new(err)
                            .color(ui.visuals().error_fg_color)
                            .small(),
                    );
                }
            });
        }
    }
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

            match state.selection {
                None => {
                    show_project_inspector(ui, state);
                }
                Some(Selection::ProjectFile(i)) => {
                    show_file_inspector(ui, state, i);
                }
                Some(Selection::MediaItem(i)) => {
                    show_media_inspector(ui, state, i);
                }
            }
        });

    None
}

fn show_project_inspector(ui: &mut egui::Ui, state: &App) {
    if let Some(ref gzp) = state.project.active_gzp {
        ui.label(RichText::new("Active File").strong());
        ui.label(
            RichText::new(gzp.display().to_string())
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(8.0);
    }

    if let Some(ref module) = state.project.module {
        ui.label(RichText::new("Module Info").strong());
        ui.label(RichText::new(format!("Defs: {}", module.defs.len())).small());
        ui.label(RichText::new(format!("Imports: {}", module.imports.len())).small());
        ui.add_space(8.0);
    }

    if let Some((_, output)) = &state.project.render_graph {
        ui.label(RichText::new("Render Graph").strong());
        ui.label(RichText::new(format!("Output node: {output:?}")).small());
    }

    if !state.project.diagnostics.is_empty() {
        ui.add_space(8.0);
        ui.label(
            RichText::new("Diagnostics")
                .strong()
                .color(ui.visuals().error_fg_color),
        );
        for diag in &state.project.diagnostics {
            ui.label(
                RichText::new(&diag.message)
                    .small()
                    .color(ui.visuals().error_fg_color),
            );
        }
    }
}

fn show_file_inspector(ui: &mut egui::Ui, state: &App, index: usize) {
    let root = match state.project.root.as_ref() {
        Some(r) => r,
        None => return,
    };

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    let gzp_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "gzp"))
        .collect();

    let Some(path) = gzp_files.get(index) else {
        return;
    };

    ui.label(RichText::new("File").strong());
    ui.label(
        RichText::new(path.display().to_string())
            .small()
            .color(ui.visuals().weak_text_color()),
    );

    if let Ok(meta) = std::fs::metadata(path) {
        ui.label(RichText::new(format!("Size: {} bytes", meta.len())).small());
    }
}

fn show_media_inspector(ui: &mut egui::Ui, state: &App, index: usize) {
    let Some(item) = state.media_items.get(index) else {
        return;
    };

    ui.label(RichText::new("Media").strong());
    ui.label(
        RichText::new(&item.path)
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(4.0);

    if let Some(res) = item.resolution {
        ui.label(RichText::new(format!("Resolution: {res}")).small());
    }
    if let Some(dur) = item.duration {
        ui.label(RichText::new(format!("Duration: {dur}")).small());
    }
    if let Some(fps) = item.fps {
        ui.label(RichText::new(format!("FPS: {fps}")).small());
    }
    ui.label(
        RichText::new(if item.available {
            "Available"
        } else {
            "Unavailable"
        })
        .small(),
    );
    if let Some(ref err) = item.error {
        ui.label(
            RichText::new(err)
                .small()
                .color(ui.visuals().error_fg_color),
        );
    }
}
