mod app;
mod crypto_adapter;
mod field;
mod sandbox;
mod theme;
mod ui;

use app::SandboxApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "kaem sandbox",
        native_options,
        Box::new(|cc| Ok(Box::new(SandboxApp::new(cc)))),
    )
}
