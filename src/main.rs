mod app;
mod engine;
mod game;
mod study;
mod ui;

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Stockfish Chess");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([600.0, 500.0])
            .with_title("Stockfish Chess"),
        ..Default::default()
    };

    eframe::run_native(
        "Stockfish Chess",
        native_options,
        Box::new(|cc| Ok(Box::new(app::ChessApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}
