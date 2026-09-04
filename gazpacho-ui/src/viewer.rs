use crate::{
    App,
    command::{Command, Pane},
};

// TODO: I don't like this.
pub fn show_viewer(ui: &mut egui::Ui, state: &mut App) -> Option<Command> {
    egui::CentralPanel::default().show(ui, |ui| {
        // Focus indicator
        if state.focused_pane == Pane::Viewer {
            let rect = ui.max_rect();
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }

        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, egui::Sense::click());

        // Background
        painter.rect_filled(response.rect, 0.0, egui::Color32::from_rgb(20, 20, 22));

        // TODO: Display rendered frame if available

        // Overlay: timecode, resolution, render state
        let overlay_rect = egui::Rect::from_min_size(
            response.rect.left_top() + egui::vec2(8.0, 8.0),
            egui::vec2(220.0, 50.0),
        );

        painter.rect_filled(overlay_rect, 4.0, egui::Color32::from_black_alpha(128));

        // let time = state.playhead.time;
        // let frame_idx = state.playhead.frame_index();

        // painter.text(
        //     overlay_rect.left_top() + egui::vec2(8.0, 8.0),
        //     egui::Align2::LEFT_TOP,
        //     format!("{time} | frame {frame_idx}"),
        //     egui::FontId::monospace(11.0),
        //     egui::Color32::from_gray(200),
        // );

        // // Click to focus viewer
        // if response.clicked() {
        //     cmd = Some(Command::FocusPane(Pane::Viewer));
        // }
    });

    None
}
