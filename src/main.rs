#![windows_subsystem = "windows"]

mod app;
mod credentials;
mod icon;
mod rdp;
mod store;
mod theme;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    let icon = egui::IconData {
        rgba: icon::render_icon(64),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RDP Manager")
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([820.0, 540.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "RDP Manager",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
