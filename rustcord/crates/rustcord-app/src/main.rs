//! Rustcord — a fast, native client for Discord.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod app;
mod backend;
mod config;
mod demo;
mod markdown;
mod streaming;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn,naga=warn,eframe=warn,egui_glow=warn".into()),
        )
        .init();

    let paths = config::Paths::resolve();
    let cfg = config::AppConfig::load(&paths);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rustcord")
            .with_app_id("rustcord")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([760.0, 480.0]),
        // Repaint only when something changes: an idle chat costs ~0% CPU.
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native(
        "Rustcord",
        options,
        Box::new(move |cc| Ok(Box::new(app::RustcordApp::new(cc, cfg, paths)))),
    )
}
