use anyhow::Result;
use egui::ColorImage;
use shakmaty::Square;
use std::path::{Path, PathBuf};

use crate::game::GameState;

/// Capture a README hero PNG when `STOCKFISH_CHESS_HERO` is set to an output path.
pub struct HeroShot {
    pub path: PathBuf,
    frame: u32,
    requested: bool,
    analysis_started: bool,
    ready_frames: u32,
}

impl HeroShot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("STOCKFISH_CHESS_HERO")?;
        Some(Self {
            path: PathBuf::from(path),
            frame: 0,
            requested: false,
            analysis_started: false,
            ready_frames: 0,
        })
    }

    pub fn opening_position() -> GameState {
        let mut game = GameState::new();
        for uci in [
            "e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6", "b5a4", "g8f6", "e1g1", "f8e7",
        ] {
            game.make_move_uci(uci)
                .unwrap_or_else(|e| panic!("hero opening move {uci}: {e}"));
        }
        game
    }

    pub fn selected_square() -> Square {
        Square::F3
    }

    pub fn should_start_analysis(&mut self) -> bool {
        if self.analysis_started {
            return false;
        }
        self.analysis_started = true;
        true
    }

    pub fn tick(&mut self, ctx: &egui::Context, line_count: usize, depth: u32) {
        self.frame = self.frame.saturating_add(1);

        ctx.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    if let Err(err) = write_png(image, &self.path) {
                        tracing::error!("hero screenshot failed: {err:#}");
                        std::process::exit(1);
                    }
                    tracing::info!("Wrote hero shot to {}", self.path.display());
                    std::process::exit(0);
                }
            }
        });

        let rich = line_count >= 3 && depth >= 12;
        let usable = line_count >= 1 && depth >= 8;
        let timed_out = self.frame >= 900;

        if rich || usable {
            self.ready_frames = self.ready_frames.saturating_add(1);
        }

        let painted = self.ready_frames >= 12;
        let ready = (rich && painted) || (usable && painted && self.frame >= 180) || timed_out;
        if ready && !self.requested {
            if line_count == 0 {
                tracing::error!("hero shot timed out before Stockfish produced analysis lines");
                std::process::exit(1);
            }
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
    }
}

fn write_png(image: &ColorImage, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let width = image.width() as u32;
    let height = image.height() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("could not allocate {width}x{height} screenshot"))?;
    for (dst, src) in pixmap.pixels_mut().iter_mut().zip(image.pixels.iter()) {
        *dst = tiny_skia::ColorU8::from_rgba(src.r(), src.g(), src.b(), src.a()).premultiply();
    }
    pixmap.save_png(path)?;
    Ok(())
}
