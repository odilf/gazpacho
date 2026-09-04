fn main() -> eyre::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "gazpacho",
        native_options,
        Box::new(|cc| Ok(Box::new(gazpacho_ui::App::new(cc)?))),
    )?;

    Ok(())
}
