use crate::engine::{DifficultyLevel, EngineActor, EngineCommand, EngineEvent};
use crate::game::{GameOutcome, GameState, PlayerColor};
use crate::ui::{ChessBoard, ControlPanel, ControlAction, MoveList, PieceRenderer, Theme};
use shakmaty::{Move, Square};
use serde::{Deserialize, Serialize};
use std::sync::mpsc;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    difficulty: DifficultyLevel,
    theme: Theme,
    player_color: PlayerColor,
    flipped: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            difficulty: DifficultyLevel::Casual,
            theme: Theme::Classic,
            player_color: PlayerColor::White,
            flipped: false,
        }
    }
}

pub struct ChessApp {
    game: GameState,
    state: AppState,
    piece_renderer: PieceRenderer,

    // Selection state
    selected_square: Option<Square>,
    legal_moves_for_selected: Vec<Move>,

    // Engine state
    engine_cmd_tx: mpsc::Sender<EngineCommand>,
    engine_event_rx: mpsc::Receiver<EngineEvent>,
    engine_ready: bool,
    engine_thinking: bool,
}

impl ChessApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load persisted state
        let state: AppState = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();

        // Spawn engine actor - try common stockfish locations
        let stockfish_path = [
            "/Users/rj/Desktop/stockfish/stockfish-macos-m1-apple-silicon",
            "/Users/rj/bin/stockfish",
            "/usr/local/bin/stockfish",
            "/opt/homebrew/bin/stockfish",
            "stockfish",
        ]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|s| s.to_string());

        let (engine_cmd_tx, engine_event_rx) = EngineActor::spawn(stockfish_path);

        // Send init command
        let cmd_tx = engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx.send(EngineCommand::Init);
        });

        Self {
            game: GameState::new(),
            state,
            piece_renderer: PieceRenderer::new(),
            selected_square: None,
            legal_moves_for_selected: Vec::new(),
            engine_cmd_tx,
            engine_event_rx,
            engine_ready: false,
            engine_thinking: false,
        }
    }

    fn clear_selection(&mut self) {
        self.selected_square = None;
        self.legal_moves_for_selected.clear();
    }

    fn select_square(&mut self, square: Square) {
        tracing::debug!("select_square called: {:?}", square);
        // Check if there's a piece of the player's color on this square
        if let Some((role, color)) = self.game.piece_at(square) {
            let turn_color: shakmaty::Color = self.game.turn().into();
            tracing::debug!("  Piece at square: {:?} {:?}, turn={:?}", color, role, turn_color);
            if color == turn_color {
                self.selected_square = Some(square);
                self.legal_moves_for_selected = self.game.legal_moves_for_square(square);
                tracing::info!("  Selected square {:?} with {} legal moves", square, self.legal_moves_for_selected.len());
                return;
            } else {
                tracing::debug!("  Wrong color piece, not selecting");
            }
        } else {
            tracing::debug!("  No piece at square, clearing selection");
        }
        self.clear_selection();
    }

    fn make_move(&mut self, m: Move) {
        if let Ok(_record) = self.game.make_move(m) {
            self.clear_selection();
            self.check_engine_turn();
        }
    }

    fn check_engine_turn(&mut self) {
        // If game is over, don't start engine
        if self.game.outcome() != GameOutcome::InProgress {
            return;
        }

        // If it's the engine's turn, start thinking
        let engine_color = match self.state.player_color {
            PlayerColor::White => PlayerColor::Black,
            PlayerColor::Black => PlayerColor::White,
        };

        if self.game.turn() == engine_color && self.engine_ready && !self.engine_thinking {
            self.start_engine_search();
        }
    }

    fn start_engine_search(&mut self) {
        self.engine_thinking = true;

        let fen = self.game.fen();
        let moves: Vec<String> = Vec::new(); // We send position as FEN, not startpos + moves

        let cmd_tx = self.engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx
                .send(EngineCommand::Go {
                    fen,
                    moves,
                    movetime_ms: Some(1000), // 1 second per move
                });
        });
    }

    fn process_engine_events(&mut self, ctx: &egui::Context) {
        // Non-blocking receive of engine events
        while let Ok(event) = self.engine_event_rx.try_recv() {
            match event {
                EngineEvent::Ready => {
                    tracing::info!("Engine is ready");
                    self.engine_ready = true;

                    // Apply initial difficulty
                    let cmd_tx = self.engine_cmd_tx.clone();
                    let difficulty = self.state.difficulty;
                    std::thread::spawn(move || {
                        let _ = cmd_tx.send(EngineCommand::SetDifficulty(difficulty));
                    });

                    // Check if engine should move first
                    self.check_engine_turn();
                }
                EngineEvent::BestMove { best_move, .. } => {
                    tracing::info!("Engine best move: {}", best_move);
                    self.engine_thinking = false;

                    // Apply the move
                    if let Err(e) = self.game.make_move_uci(&best_move) {
                        tracing::error!("Failed to apply engine move: {}", e);
                    }

                    ctx.request_repaint();
                }
                EngineEvent::Info { depth, score_cp, .. } => {
                    if let (Some(d), Some(s)) = (depth, score_cp) {
                        tracing::debug!("Engine: depth={}, score={}", d, s);
                    }
                }
                EngineEvent::Error(e) => {
                    tracing::error!("Engine error: {}", e);
                    self.engine_thinking = false;
                }
                EngineEvent::Terminated => {
                    tracing::warn!("Engine terminated");
                    self.engine_ready = false;
                    self.engine_thinking = false;
                }
            }
        }
    }

    fn new_game(&mut self) {
        self.game.reset();
        self.clear_selection();
        self.engine_thinking = false;

        // Tell engine about new game
        let cmd_tx = self.engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx.send(EngineCommand::NewGame);
        });

        // Check if engine should move first (player is black)
        if self.state.player_color == PlayerColor::Black {
            self.check_engine_turn();
        }
    }

    fn handle_control_action(&mut self, action: ControlAction) {
        match action {
            ControlAction::NewGame => {
                self.new_game();
            }
            ControlAction::FlipBoard => {
                self.state.flipped = !self.state.flipped;
            }
            ControlAction::SetDifficulty(level) => {
                self.state.difficulty = level;
                let cmd_tx = self.engine_cmd_tx.clone();
                std::thread::spawn(move || {
                    let _ = cmd_tx.send(EngineCommand::SetDifficulty(level));
                });
            }
            ControlAction::SetTheme(theme) => {
                tracing::info!("Setting theme to: {:?}", theme);
                self.state.theme = theme;
            }
            ControlAction::SetPlayerColor(color) => {
                self.state.player_color = color;
                // Start new game when changing color
                self.new_game();
            }
        }
    }
}

impl eframe::App for ChessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process engine events
        self.process_engine_events(ctx);

        // Request repaint while engine is thinking (for spinner)
        if self.engine_thinking {
            ctx.request_repaint();
        }

        // Side panel for controls
        egui::SidePanel::left("controls")
            .default_width(200.0)
            .show(ctx, |ui| {
                if let Some(action) = ControlPanel::show(
                    ui,
                    &mut self.state.difficulty,
                    &mut self.state.theme,
                    &mut self.state.player_color,
                    self.game.outcome(),
                    self.engine_thinking,
                ) {
                    self.handle_control_action(action);
                }
            });

        // Bottom panel for move list
        egui::TopBottomPanel::bottom("moves")
            .default_height(150.0)
            .show(ctx, |ui| {
                MoveList::show(ui, self.game.move_history());
            });

        // Central panel for the board
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut board = ChessBoard::new(
                &self.game,
                self.state.theme,
                self.state.flipped,
                &mut self.piece_renderer,
            );

            let response = board.show(
                ui,
                &mut self.selected_square,
                &self.legal_moves_for_selected,
            );

            // Determine if player can make moves
            let game_turn = self.game.turn();
            let player_color = self.state.player_color;
            let can_move = self.game.outcome() == GameOutcome::InProgress
                && !self.engine_thinking
                && game_turn == player_color;
            tracing::debug!("can_move: {}, game_turn: {:?}, player_color: {:?}, thinking: {}", 
                can_move, game_turn, player_color, self.engine_thinking);

            // Always allow selection (to see legal moves), but only allow actual moves when it's player's turn
            if let Some(square) = response.square_clicked {
                tracing::info!("Square clicked: {:?}", square);
                // Always select the square to show legal moves, regardless of whose turn it is
                self.select_square(square);
            }
            
            if let Some(m) = response.move_made {
                tracing::info!("Move attempted: {:?}", m);
                if can_move {
                    self.make_move(m);
                } else {
                    tracing::debug!("Cannot move: not player's turn or game over");
                }
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.state);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Send quit command to engine
        let cmd_tx = self.engine_cmd_tx.clone();
        let _ = cmd_tx.send(EngineCommand::Quit);
    }
}
