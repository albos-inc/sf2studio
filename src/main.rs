//! sf2studio: make, tune and compare SF2 instruments.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis;
mod app;
mod audio;
mod mac_sampler;
mod patterns;
mod render;
mod tuning;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("sf2studio")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("sf2studio", options, Box::new(|cc| Ok(Box::new(app::StudioApp::new(cc)))))
}
