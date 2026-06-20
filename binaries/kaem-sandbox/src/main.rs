mod app;
mod crypto_adapter;
mod field;
mod frame_info;
mod sandbox;
mod sim_adapter;
mod theme;
mod ui;

use app::SandboxApp;

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("assets/icon.png must be a valid png");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 720.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "kaem sandbox",
        native_options,
        Box::new(|cc| Ok(Box::new(SandboxApp::new(cc)))),
    )
}
