// Release builds on Windows must not spawn a console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod folding;
mod fonts;
mod icons;
mod settings;
mod theme;
mod widgets;
mod worker;

fn main() -> eframe::Result<()> {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("rustafari")
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([680.0, 460.0])
        .with_app_id("dev.rustafari.app");

    // Extend content under a transparent title bar, the way native macOS
    // apps do. Only macOS honours these; the sidebar pads itself clear of the
    // traffic lights (see `TITLEBAR_INSET`). Despite the name,
    // `with_titlebar_shown(false)` makes the bar transparent rather than
    // removing it — the traffic lights stay.
    if cfg!(target_os = "macos") {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "rustafari",
        options,
        Box::new(|cc| Ok(Box::new(app::Rustafari::new(cc)))),
    )
}
