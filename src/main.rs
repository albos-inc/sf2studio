//! sf2studio: make, tune and compare SF2 instruments.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod abx;
mod analysis;
mod app;
mod audio;
mod create;
mod gm;
mod keyboard;
mod mac_sampler;
mod measure;
mod midi_in;
mod null_test;
mod patterns;
mod render;
mod theme;
mod tuning;

fn main() -> eframe::Result {
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("sf2studio")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0])
            .with_icon(std::sync::Arc::new(theme::app_icon())),
        ..Default::default()
    };
    // The guide's screenshots: always the same size, in front, and with
    // nothing read from or written to the user's settings.
    #[cfg(feature = "capture")]
    if std::env::args().any(|a| a == "--capture") {
        let scratch = std::env::temp_dir().join("sf2studio-capture-settings");
        let _ = std::fs::remove_dir_all(&scratch);
        options.persistence_path = Some(scratch);
        options.persist_window = false;
        options.viewport = options
            .viewport
            .with_inner_size([1280.0, 820.0])
            .with_resizable(false)
            .with_window_level(eframe::egui::WindowLevel::AlwaysOnTop);
    }
    eframe::run_native("sf2studio", options, Box::new(|cc| Ok(Box::new(app::StudioApp::new(cc)))))
}
