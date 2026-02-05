use crate::engine::DifficultyLevel;
use crate::game::{GameOutcome, PlayerColor};
use crate::ui::Theme;
use egui::Ui;

pub struct ControlPanel;

#[derive(Debug, Clone)]
pub enum ControlAction {
    NewGame,
    FlipBoard,
    SetDifficulty(DifficultyLevel),
    SetTheme(Theme),
    SetPlayerColor(PlayerColor),
    Resign,
    OfferDraw,
    Undo,
}

impl ControlPanel {
    pub fn show(
        ui: &mut Ui,
        difficulty: &mut DifficultyLevel,
        theme: &mut Theme,
        player_color: &mut PlayerColor,
        outcome: GameOutcome,
        is_engine_thinking: bool,
    ) -> Option<ControlAction> {
        let mut action = None;

        ui.vertical(|ui| {
            ui.heading("Stockfish Chess");
            ui.separator();

            // Game status
            match outcome {
                GameOutcome::InProgress => {
                    if is_engine_thinking {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Engine thinking...");
                        });
                    }
                }
                GameOutcome::Checkmate(winner) => {
                    let text = match winner {
                        PlayerColor::White => "White wins by checkmate!",
                        PlayerColor::Black => "Black wins by checkmate!",
                    };
                    ui.colored_label(egui::Color32::GREEN, text);
                }
                GameOutcome::Stalemate => {
                    ui.colored_label(egui::Color32::YELLOW, "Draw by stalemate");
                }
                GameOutcome::InsufficientMaterial => {
                    ui.colored_label(egui::Color32::YELLOW, "Draw by insufficient material");
                }
                GameOutcome::ThreefoldRepetition => {
                    ui.colored_label(egui::Color32::YELLOW, "Draw by threefold repetition");
                }
                GameOutcome::FiftyMoveRule => {
                    ui.colored_label(egui::Color32::YELLOW, "Draw by fifty-move rule");
                }
                GameOutcome::Resignation(winner) => {
                    let text = match winner {
                        PlayerColor::White => "White wins by resignation!",
                        PlayerColor::Black => "Black wins by resignation!",
                    };
                    ui.colored_label(egui::Color32::GREEN, text);
                }
                GameOutcome::DrawByAgreement => {
                    ui.colored_label(egui::Color32::YELLOW, "Draw by agreement");
                }
            }

            ui.add_space(10.0);

            // New Game button
            if ui.button("New Game").clicked() {
                action = Some(ControlAction::NewGame);
            }

            // Flip Board button
            if ui.button("Flip Board").clicked() {
                action = Some(ControlAction::FlipBoard);
            }

            ui.add_space(10.0);
            ui.separator();

            // Play as
            ui.label("Play as:");
            ui.horizontal(|ui| {
                if ui.selectable_label(*player_color == PlayerColor::White, "White").clicked() {
                    *player_color = PlayerColor::White;
                    action = Some(ControlAction::SetPlayerColor(PlayerColor::White));
                }
                if ui.selectable_label(*player_color == PlayerColor::Black, "Black").clicked() {
                    *player_color = PlayerColor::Black;
                    action = Some(ControlAction::SetPlayerColor(PlayerColor::Black));
                }
            });

            ui.add_space(10.0);

            // Difficulty selection
            ui.label("Difficulty:");
            egui::ComboBox::from_id_salt("difficulty")
                .selected_text(difficulty.label())
                .show_ui(ui, |ui| {
                    for level in DifficultyLevel::all() {
                        if ui.selectable_value(difficulty, *level, level.label()).clicked() {
                            action = Some(ControlAction::SetDifficulty(*level));
                        }
                    }
                });

            ui.add_space(10.0);

            // Theme selection
            ui.label("Theme:");
            egui::ComboBox::from_id_salt("theme")
                .selected_text(theme.label())
                .show_ui(ui, |ui| {
                    for t in Theme::all() {
                        if ui.selectable_value(theme, *t, t.label()).clicked() {
                            action = Some(ControlAction::SetTheme(*t));
                        }
                    }
                });

            // Game actions (only during active game)
            if outcome == GameOutcome::InProgress {
                ui.add_space(10.0);
                ui.separator();
                
                ui.horizontal(|ui| {
                    if ui.button("🏳 Resign").clicked() {
                        action = Some(ControlAction::Resign);
                    }
                    if ui.button("🤝 Offer Draw").clicked() {
                        action = Some(ControlAction::OfferDraw);
                    }
                });
                
                if ui.button("↩ Undo Move").clicked() {
                    action = Some(ControlAction::Undo);
                }
            }
        });

        action
    }
}
