mod app;
mod controller;
mod detail_view;
mod session;
mod state;
mod view;

use app::DesignApp;

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
