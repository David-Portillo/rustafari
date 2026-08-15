// Release builds on Windows must not spawn a console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod settings;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("rustafari")
            .with_inner_size([1024.0, 700.0])
            .with_min_inner_size([680.0, 460.0])
            .with_app_id("dev.rustafari.app"),
        ..Default::default()
    };

    eframe::run_native(
        "rustafari",
        options,
        Box::new(|cc| Ok(Box::new(app::Rustafari::new(cc)))),
    )
}
