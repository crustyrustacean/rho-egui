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
        Box::new(|cc| {
            // Fonts must be registered before the first frame; egui resolves
            // glyphs lazily but the family list is read at layout time.
            ui::fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc.egui_ctx.clone())))
        }),
    )
}
