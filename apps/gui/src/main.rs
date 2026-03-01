use design_gui::app::DesignApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DesignBrainModel GUI",
        native_options,
        Box::new(|cc| Ok(Box::new(DesignApp::new(cc)))),
    )
}
