use crate::game::MoveRecord;
use egui::{ScrollArea, Ui};

pub struct MoveList;

impl MoveList {
    pub fn show(ui: &mut Ui, moves: &[MoveRecord]) {
        ui.vertical(|ui| {
            ui.heading("Moves");
            ui.separator();

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Display moves in pairs (white, black)
                    let mut move_pairs: Vec<(usize, &str, Option<&str>)> = Vec::new();

                    for (i, record) in moves.iter().enumerate() {
                        let move_number = i / 2 + 1;
                        if i % 2 == 0 {
                            // White's move
                            let black_move = moves.get(i + 1).map(|r| r.san.as_str());
                            move_pairs.push((move_number, &record.san, black_move));
                        }
                    }

                    for (num, white_move, black_move) in move_pairs {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}.", num));
                            ui.monospace(white_move);
                            if let Some(black) = black_move {
                                ui.monospace(black);
                            }
                        });
                    }

                    // Auto-scroll to bottom
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                });
        });
    }
}
