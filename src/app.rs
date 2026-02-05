use crate::engine::{DifficultyLevel, EngineActor, EngineCommand, EngineEvent};
use crate::game::{GameOutcome, GameState, PlayerColor};
use crate::ui::{ChessBoard, ControlPanel, ControlAction, MoveList, PieceRenderer, Theme, AnalysisPanel};
use shakmaty::{Move, Square};
use serde::{Deserialize, Serialize};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    Game,
    Analysis,
}

impl Default for AppMode {
    fn default() -> Self {
        AppMode::Game
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    difficulty: DifficultyLevel,
    theme: Theme,
    player_color: PlayerColor,
    flipped: bool,
    mode: AppMode,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            difficulty: DifficultyLevel::Casual,
            theme: Theme::Classic,
            player_color: PlayerColor::White,
            flipped: false,
            mode: AppMode::Game,
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
    engine_analyzing: bool,

    // Analysis
    analysis_panel: AnalysisPanel,
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
            "./stockfish",
            "/Users/rj/Desktop/stockfish/stockfish-macos-m1-apple-silicon",
            "~/bin/stockfish",
            "/usr/local/bin/stockfish",
            "/opt/homebrew/bin/stockfish",
            "stockfish",
        ]
        .iter()
        .find(|p| {
            let expanded = shellexpand::tilde(p);
            std::path::Path::new(expanded.as_ref()).exists()
        })
        .map(|s| shellexpand::tilde(s).to_string());

        let (engine_cmd_tx, engine_event_rx) = EngineActor::spawn(stockfish_path);

        // Send init command
        let cmd_tx = engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx.send(EngineCommand::Init);
        });

        let mut app = Self {
            game: GameState::new(),
            state,
            piece_renderer: PieceRenderer::new(),
            selected_square: None,
            legal_moves_for_selected: Vec::new(),
            engine_cmd_tx,
            engine_event_rx,
            engine_ready: false,
            engine_thinking: false,
            engine_analyzing: false,
            analysis_panel: AnalysisPanel::default(),
        };

        // Clear selection when initializing
        app.clear_selection();
        app
    }

    fn clear_selection(&mut self) {
        self.selected_square = None;
        self.legal_moves_for_selected.clear();
    }

    fn select_square(&mut self, square: Square) {
        // Check if there's a piece of the current turn's color on this square
        if let Some((role, color)) = self.game.piece_at(square) {
            let turn_color: shakmaty::Color = self.game.turn().into();
            if color == turn_color {
                self.selected_square = Some(square);
                self.legal_moves_for_selected = self.game.legal_moves_for_square(square);
                return;
            }
        }
        self.clear_selection();
    }

    fn make_move(&mut self, m: Move) {
        if let Ok(_record) = self.game.make_move(m) {
            self.clear_selection();
            
            // In analysis mode, restart analysis on new position
            if self.state.mode == AppMode::Analysis && self.engine_analyzing {
                self.start_analysis();
            } else if self.state.mode == AppMode::Game {
                self.check_engine_turn();
            }
        }
    }

    fn check_engine_turn(&mut self) {
        // Only in game mode
        if self.state.mode != AppMode::Game {
            return;
        }

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
        let moves: Vec<String> = Vec::new();

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

    fn start_analysis(&mut self) {
        if !self.engine_ready || self.engine_analyzing {
            return;
        }

        self.engine_analyzing = true;
        self.analysis_panel.is_analyzing = true;

        let fen = self.game.fen();
        let moves: Vec<String> = Vec::new();

        let cmd_tx = self.engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx
                .send(EngineCommand::Analyze { fen, moves });
        });
    }

    fn stop_analysis(&mut self) {
        if self.engine_analyzing {
            self.engine_analyzing = false;
            self.analysis_panel.is_analyzing = false;
            
            let cmd_tx = self.engine_cmd_tx.clone();
            std::thread::spawn(move || {
                let _ = cmd_tx.send(EngineCommand::Stop);
            });
        }
    }

    fn toggle_analysis(&mut self) {
        if self.engine_analyzing {
            self.stop_analysis();
        } else {
            self.start_analysis();
        }
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

                    // Check if engine should move first (in game mode)
                    if self.state.mode == AppMode::Game {
                        self.check_engine_turn();
                    }
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
                EngineEvent::Info { depth, score_cp, score_mate, pv, nodes, .. } => {
                    // Update analysis panel
                    self.analysis_panel.update_evaluation(score_cp, score_mate, depth, pv, nodes);
                }
                EngineEvent::Error(e) => {
                    tracing::error!("Engine error: {}", e);
                    self.engine_thinking = false;
                    self.engine_analyzing = false;
                    self.analysis_panel.is_analyzing = false;
                }
                EngineEvent::Terminated => {
                    tracing::warn!("Engine terminated");
                    self.engine_ready = false;
                    self.engine_thinking = false;
                    self.engine_analyzing = false;
                    self.analysis_panel.is_analyzing = false;
                }
            }
        }
    }

    fn new_game(&mut self) {
        self.stop_analysis();
        self.game.reset();
        self.clear_selection();
        self.engine_thinking = false;

        // Tell engine about new game
        let cmd_tx = self.engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx.send(EngineCommand::NewGame);
        });

        // Check if engine should move first (player is black)
        if self.state.mode == AppMode::Game && self.state.player_color == PlayerColor::Black {
            self.check_engine_turn();
        }

        // Restart analysis if in analysis mode
        if self.state.mode == AppMode::Analysis && self.analysis_panel.is_analyzing {
            self.start_analysis();
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

    fn go_to_previous_position(&mut self) {
        if self.game.can_go_back() {
            self.clear_selection();
            let _ = self.game.go_back();
            
            // Restart analysis on new position
            if self.state.mode == AppMode::Analysis && self.engine_analyzing {
                self.start_analysis();
            }
        }
    }

    fn go_to_next_position(&mut self) {
        if self.game.can_go_forward() {
            self.clear_selection();
            let _ = self.game.go_forward();
            
            // Restart analysis on new position
            if self.state.mode == AppMode::Analysis && self.engine_analyzing {
                self.start_analysis();
            }
        }
    }

    fn go_to_start(&mut self) {
        self.clear_selection();
        self.game.go_to_start();
        
        if self.state.mode == AppMode::Analysis && self.engine_analyzing {
            self.start_analysis();
        }
    }

    fn go_to_end(&mut self) {
        self.clear_selection();
        self.game.go_to_end();
        
        if self.state.mode == AppMode::Analysis && self.engine_analyzing {
            self.start_analysis();
        }
    }

    fn set_mode(&mut self, mode: AppMode) {
        if self.state.mode != mode {
            self.state.mode = mode;
            
            // Stop any ongoing analysis/game activities
            self.stop_analysis();
            
            if mode == AppMode::Game {
                // Start fresh game
                self.new_game();
            } else {
                // In analysis mode, we keep the current position
                // Optionally start analysis immediately
            }
        }
    }
}

impl eframe::App for ChessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process engine events
        self.process_engine_events(ctx);

        // Request repaint while engine is analyzing (for live updates)
        if self.engine_analyzing {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Side panel for controls and analysis
        egui::SidePanel::left("controls")
            .default_width(220.0)
            .show(ctx, |ui| {
                // Mode selector
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    if ui.selectable_label(self.state.mode == AppMode::Game, "🎮 Game").clicked() {
                        self.set_mode(AppMode::Game);
                    }
                    if ui.selectable_label(self.state.mode == AppMode::Analysis, "📊 Analysis").clicked() {
                        self.set_mode(AppMode::Analysis);
                    }
                });
                ui.separator();

                // Navigation controls (mainly for analysis mode, but also useful in game)
                if self.state.mode == AppMode::Analysis || self.game.can_go_back() || self.game.can_go_forward() {
                    ui.label("Navigation:");
                    ui.horizontal(|ui| {
                        if ui.button("⏮ Start").clicked() {
                            self.go_to_start();
                        }
                        if ui.button("◀ Prev").clicked() {
                            self.go_to_previous_position();
                        }
                        if ui.button("▶ Next").clicked() {
                            self.go_to_next_position();
                        }
                        if ui.button("⏭ End").clicked() {
                            self.go_to_end();
                        }
                    });
                    
                    // Position indicator
                    ui.label(format!("Position: {} / {}", 
                        self.game.current_index(), 
                        self.game.position_count() - 1
                    ));
                    ui.separator();
                }

                // Analysis panel (only in analysis mode)
                if self.state.mode == AppMode::Analysis {
                    ui.horizontal(|ui| {
                        if ui.button(if self.engine_analyzing { "⏹ Stop" } else { "▶ Analyze" })
                            .clicked() {
                            self.toggle_analysis();
                        }
                    });
                    ui.separator();
                    
                    self.analysis_panel.show(ui);
                    ui.separator();
                }

                // Standard controls
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

            // Handle board interaction
            // In game mode, only allow moves when it's player's turn
            // In analysis mode, allow exploring moves but they create a new variation
            let can_interact = match self.state.mode {
                AppMode::Game => {
                    self.game.outcome() == GameOutcome::InProgress
                        && !self.engine_thinking
                        && self.game.turn() == self.state.player_color
                }
                AppMode::Analysis => {
                    // In analysis, always allow moves, but they'll truncate future if not at end
                    self.game.outcome() == GameOutcome::InProgress
                }
            };

            if let Some(square) = response.square_clicked {
                self.select_square(square);
            }
            
            if let Some(m) = response.move_made {
                if can_interact {
                    self.make_move(m);
                }
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.state);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Stop analysis before quitting
        self.stop_analysis();
        
        // Send quit command to engine
        let cmd_tx = self.engine_cmd_tx.clone();
        let _ = cmd_tx.send(EngineCommand::Quit);
    }
}
