use crate::engine::DifficultyLevel;
use crate::game::{GameOutcome, PlayerColor};
use crate::ui::Theme;
use egui::Ui;

pub struct ControlPanel;

/// Game and engine state the control panel displays.
pub struct GameStatus<'a> {
    pub outcome: GameOutcome,
    pub engine_thinking: bool,
    pub engine_ready: bool,
    pub can_undo: bool,
    /// Feedback for the player's last action, such as a declined draw offer.
    pub message: Option<&'a str>,
}

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
        player_color: PlayerColor,
        status: &GameStatus,
    ) -> Option<ControlAction> {
        let mut action = None;
        let outcome = status.outcome;

        ui.vertical(|ui| {
            ui.heading("Stockfish Chess");
            ui.separator();

            // Game status
            match outcome {
                GameOutcome::InProgress => {
                    // The spinner also keeps egui repainting, which is what
                    // delivers Stockfish's reply without the mouse moving.
                    if status.engine_thinking {
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

            if let Some(message) = status.message {
                ui.label(message);
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
                for (color, label) in [(PlayerColor::White, "White"), (PlayerColor::Black, "Black")]
                {
                    // Re-clicking the current color must not restart the game.
                    if ui.selectable_label(player_color == color, label).clicked()
                        && player_color != color
                    {
                        action = Some(ControlAction::SetPlayerColor(color));
                    }
                }
            });

            ui.add_space(10.0);

            // Difficulty selection
            ui.label("Difficulty:");
            egui::ComboBox::from_id_salt("difficulty")
                .selected_text(difficulty.label())
                .show_ui(ui, |ui| {
                    for level in DifficultyLevel::all() {
                        if ui
                            .selectable_value(difficulty, *level, level.label())
                            .clicked()
                        {
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
                    let draw_button = ui
                        .add_enabled(
                            status.engine_ready && !status.engine_thinking,
                            egui::Button::new("🤝 Offer Draw"),
                        )
                        .on_hover_text("Stockfish must be ready to evaluate a draw offer");
                    if draw_button.clicked() {
                        action = Some(ControlAction::OfferDraw);
                    }
                });
            }

            // Undo stays available after the game ends, to retry a lost position.
            if status.can_undo && ui.button("↩ Undo Move").clicked() {
                action = Some(ControlAction::Undo);
            }
        });

        action
    }
}
