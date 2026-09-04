use egui::{self, RichText};

use crate::command::{Command, all_commands};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CommandPalette {
    pub open: bool,
    pub query: String,
}

impl CommandPalette {
    /// Shows the command palette. Returns a command to dispatch when the user
    /// selects one, or when the palette is closed (via `Command::Escape`).
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Command> {
        if !self.open {
            return None;
        }

        let mut result = None;

        egui::Window::new("Command Palette")
            .open(&mut self.open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .fixed_size([420.0, 300.0])
            .show(ctx, |ui| {
                let response = ui.text_edit_singleline(&mut self.query);
                ui.memory_mut(|m| m.request_focus(response.id));

                ui.separator();

                let commands = all_commands();
                let filtered: Vec<_> = commands
                    .iter()
                    .filter(|cmd| {
                        self.query.is_empty()
                            || cmd
                                .label()
                                .to_lowercase()
                                .contains(&self.query.to_lowercase())
                    })
                    .collect();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for cmd in filtered {
                        let label = cmd.label();
                        let hint = cmd.shortcut_hint().unwrap_or("");
                        let text = if hint.is_empty() {
                            label
                        } else {
                            format!("{label}  ({hint})")
                        };

                        if ui
                            .selectable_label(false, RichText::new(text).monospace())
                            .clicked()
                        {
                            result = Some(cmd.clone());
                        }
                    }
                });
            });

        // The window was closed by the user (X button or Esc), so treat it as an
        // Escape command.
        if !self.open && result.is_none() {
            result = Some(Command::Escape);
        }

        result
    }
}
