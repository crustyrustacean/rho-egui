// src/main.rs

mod agent;
mod app;
mod chat;
mod ui;
mod util;

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("rho"),
        ..Default::default()
    };

    eframe::run_native(
        "rho",
        options,
        Box::new(|_cc| Ok(Box::new(app::App::default()))),
    )
}
