use crate::engine::{DifficultyLevel, EngineActor, EngineCommand, EngineEvent};
use crate::game::{GameOutcome, GameState, PlayerColor, MoveRecord};
use crate::study::{Study, StudyManager};
use crate::ui::{ChessBoard, ControlPanel, ControlAction, MoveList, PieceRenderer, Theme, AnalysisPanel, StudyPanel};
use shakmaty::{Move, Square};
use serde::{Deserialize, Serialize};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    Game,
    Analysis,
    Study,
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

    // Study
    study: Study,
    study_panel: StudyPanel,
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
            study: Study::new("Untitled Study".to_string()),
            study_panel: StudyPanel::default(),
        };

        app.clear_selection();
        app
    }

    fn clear_selection(&mut self) {
        self.selected_square = None;
        self.legal_moves_for_selected.clear();
    }

    fn select_square(&mut self, square: Square) {
        if let Some((_role, color)) = self.game.piece_at(square) {
            let turn_color: shakmaty::Color = self.game.turn().into();
            if color == turn_color {
                self.selected_square = Some(square);
                self.legal_moves_for_selected = self.game.legal_moves_for_square(square);
                return;
            }
        }
        self.clear_selection();
    }

    fn make_move(&mut self, m: Move) -> Option<MoveRecord> {
        if let Ok(record) = self.game.make_move(m) {
            self.clear_selection();
            
            // In study mode, add to study tree
            if self.state.mode == AppMode::Study {
                self.study.current_chapter_mut().add_move(record.clone(), self.game.fen());
                self.study.update_timestamp();
            }
            
            // In analysis mode, restart analysis on new position
            if self.state.mode == AppMode::Analysis && self.engine_analyzing {
                self.start_analysis();
            } else if self.state.mode == AppMode::Game {
                self.check_engine_turn();
            }
            
            Some(record)
        } else {
            None
        }
    }

    fn check_engine_turn(&mut self) {
        if self.state.mode != AppMode::Game {
            return;
        }

        if self.game.outcome() != GameOutcome::InProgress {
            return;
        }

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
                    movetime_ms: Some(1000),
                });
        });
    }

    fn start_analysis(&mut self) {
        if !self.engine_ready || self.engine_analyzing {
            return;
        }

        self.engine_analyzing = true;
        self.analysis_panel.is_analyzing = true;
        self.analysis_panel.clear();

        let fen = self.game.fen();
        let moves: Vec<String> = Vec::new();
        // Always calculate max (5) lines, just display fewer
        let max_lines = 5;

        let cmd_tx = self.engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx.send(EngineCommand::SetMultiPV(max_lines));
            let _ = cmd_tx.send(EngineCommand::Analyze { fen, moves });
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
        while let Ok(event) = self.engine_event_rx.try_recv() {
            match event {
                EngineEvent::Ready => {
                    tracing::info!("Engine is ready");
                    self.engine_ready = true;

                    let cmd_tx = self.engine_cmd_tx.clone();
                    let difficulty = self.state.difficulty;
                    std::thread::spawn(move || {
                        let _ = cmd_tx.send(EngineCommand::SetDifficulty(difficulty));
                    });

                    if self.state.mode == AppMode::Game {
                        self.check_engine_turn();
                    }
                }
                EngineEvent::BestMove { best_move, .. } => {
                    tracing::info!("Engine best move: {}", best_move);
                    self.engine_thinking = false;

                    if let Err(e) = self.game.make_move_uci(&best_move) {
                        tracing::error!("Failed to apply engine move: {}", e);
                    }

                    ctx.request_repaint();
                }
                EngineEvent::Info { depth, score_cp, score_mate, pv, nodes, multipv, .. } => {
                    let line_id = multipv.unwrap_or(1);
                    self.analysis_panel.update_line(line_id, score_cp, score_mate, depth, pv);
                    if let Some(n) = nodes {
                        self.analysis_panel.total_nodes = n;
                    }
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

        let cmd_tx = self.engine_cmd_tx.clone();
        std::thread::spawn(move || {
            let _ = cmd_tx.send(EngineCommand::NewGame);
        });

        if self.state.mode == AppMode::Game && self.state.player_color == PlayerColor::Black {
            self.check_engine_turn();
        }

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
                self.new_game();
            }
        }
    }

    fn go_to_previous_position(&mut self) {
        if self.game.can_go_back() {
            self.clear_selection();
            let _ = self.game.go_back();
            
            if self.state.mode == AppMode::Study {
                self.study.current_chapter_mut().go_back();
            }
            
            if self.state.mode == AppMode::Analysis && self.engine_analyzing {
                self.start_analysis();
            }
        }
    }

    fn go_to_next_position(&mut self) {
        if self.game.can_go_forward() {
            self.clear_selection();
            let _ = self.game.go_forward();
            
            if self.state.mode == AppMode::Study {
                // In study mode, try to follow the main line
                self.study.current_chapter_mut().go_to_child(0);
            }
            
            if self.state.mode == AppMode::Analysis && self.engine_analyzing {
                self.start_analysis();
            }
        }
    }

    fn go_to_start(&mut self) {
        self.clear_selection();
        self.game.go_to_start();
        
        if self.state.mode == AppMode::Study {
            self.study.current_chapter_mut().go_to_start();
        }
        
        if self.state.mode == AppMode::Analysis && self.engine_analyzing {
            self.start_analysis();
        }
    }

    fn go_to_end(&mut self) {
        self.clear_selection();
        self.game.go_to_end();
        
        if self.state.mode == AppMode::Study {
            // Go to end of main line
            while self.study.current_chapter().can_go_forward(0) {
                self.study.current_chapter_mut().go_to_child(0);
            }
        }
        
        if self.state.mode == AppMode::Analysis && self.engine_analyzing {
            self.start_analysis();
        }
    }

    fn set_mode(&mut self, mode: AppMode) {
        if self.state.mode != mode {
            self.state.mode = mode;
            
            self.stop_analysis();
            
            match mode {
                AppMode::Game => {
                    self.new_game();
                }
                AppMode::Analysis => {
                    // Keep current position
                }
                AppMode::Study => {
                    // Sync game with study position
                    let fen = self.study.current_chapter().current_fen().to_string();
                    if let Ok(new_game) = GameState::from_fen(&fen) {
                        self.game = new_game;
                    }
                }
            }
        }
    }

    /// Apply a move clicked from engine analysis (creates a fork/variation)
    /// Returns true if move was successfully applied
    fn apply_engine_move(&mut self, uci_move: &str) -> bool {
        use shakmaty::uci::UciMove;
        
        // Parse the UCI move
        if let Ok(uci) = uci_move.parse::<UciMove>() {
            // Convert to Move
            if let Ok(m) = uci.to_move(self.game.current_position()) {
                // Check if move is legal
                if self.game.legal_moves().contains(&m) {
                    // Apply the move
                    if let Some(record) = self.make_move(m) {
                        // In Analysis mode, this creates a variation/fork
                        tracing::info!("Applied engine move: {} (fork)", record.san);
                        
                        // Restart analysis at new position
                        if self.engine_analyzing {
                            self.stop_analysis();
                            self.start_analysis();
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Export current game as PGN
    fn export_game_pgn(&self) -> String {
        use chrono::Local;
        
        let mut pgn = String::new();
        
        // Headers
        pgn.push_str(&format!("[Event \"Stockfish Chess Game\"]\n"));
        pgn.push_str(&format!("[Site \"Local\"]\n"));
        pgn.push_str(&format!("[Date \"{}\"]\n", Local::now().format("%Y.%m.%d")));
        pgn.push_str(&format!("[Round \"-\"]\n"));
        pgn.push_str(&format!("[White \"Player\"]\n"));
        pgn.push_str(&format!("[Black \"Stockfish\"]\n"));
        
        // Result
        let result = match self.game.outcome() {
            GameOutcome::Checkmate(PlayerColor::White) => "1-0",
            GameOutcome::Checkmate(PlayerColor::Black) => "0-1",
            GameOutcome::Stalemate | GameOutcome::InsufficientMaterial | 
            GameOutcome::ThreefoldRepetition | GameOutcome::FiftyMoveRule => "1/2-1/2",
            GameOutcome::InProgress => "*",
        };
        pgn.push_str(&format!("[Result \"{}\"]\n", result));
        pgn.push('\n');
        
        // Moves
        for (i, record) in self.game.move_history().iter().enumerate() {
            if i % 2 == 0 {
                pgn.push_str(&format!("{}. ", i / 2 + 1));
            }
            pgn.push_str(&record.san);
            pgn.push(' ');
        }
        
        pgn.push_str(result);
        pgn.push('\n');
        
        pgn
    }

    /// Save current game to a new study
    fn save_game_to_study(&mut self) {
        let mut new_study = Study::new(format!("Game {}", chrono::Local::now().format("%Y-%m-%d %H:%M")));
        
        // Replay all moves into the study
        let moves: Vec<_> = self.game.move_history().iter().cloned().collect();
        for record in moves {
            new_study.current_chapter_mut().add_move(record, self.game.fen());
        }
        
        self.study = new_study;
        self.state.mode = AppMode::Study;
        tracing::info!("Game saved to new study");
    }
}

impl eframe::App for ChessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_engine_events(ctx);

        if self.engine_analyzing {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Side panel for controls, analysis, or study
        egui::SidePanel::left("sidebar")
            .default_width(240.0)
            .show(ctx, |ui| {
                // Mode selector
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    if ui.selectable_label(self.state.mode == AppMode::Game, "🎮").clicked() {
                        self.set_mode(AppMode::Game);
                    }
                    if ui.selectable_label(self.state.mode == AppMode::Analysis, "📊").clicked() {
                        self.set_mode(AppMode::Analysis);
                    }
                    if ui.selectable_label(self.state.mode == AppMode::Study, "📚").clicked() {
                        self.set_mode(AppMode::Study);
                    }
                });
                ui.separator();

                // Navigation controls
                if self.state.mode != AppMode::Game || self.game.can_go_back() || self.game.can_go_forward() {
                    ui.label("Navigation:");
                    ui.horizontal(|ui| {
                        if ui.button("⏮").on_hover_text("Go to start").clicked() {
                            self.go_to_start();
                        }
                        if ui.button("◀").on_hover_text("Previous move").clicked() {
                            self.go_to_previous_position();
                        }
                        if ui.button("▶").on_hover_text("Next move").clicked() {
                            self.go_to_next_position();
                        }
                        if ui.button("⏭").on_hover_text("Go to end").clicked() {
                            self.go_to_end();
                        }
                    });
                    
                    ui.label(format!("Move: {} / {}", 
                        self.game.current_index(), 
                        self.game.position_count() - 1
                    ));
                    ui.separator();
                }

                // Mode-specific panels
                match self.state.mode {
                    AppMode::Analysis | AppMode::Study => {
                        // Combined Analysis + Study mode
                        ui.horizontal(|ui| {
                            if ui.button(if self.engine_analyzing { "⏹ Stop" } else { "▶ Analyze" })
                                .clicked() {
                                self.toggle_analysis();
                            }
                        });
                        ui.separator();
                        
                        // Show analysis panel and handle clicked moves
                        let clicked_path = self.analysis_panel.show(ui);
                        
                        // If user clicked a move in an engine line, apply the full path
                        // clicked_path contains all moves from start to clicked move
                        if !clicked_path.is_empty() {
                            tracing::info!("Playing engine path: {:?}", clicked_path);
                            
                            // Play each move in the path sequentially
                            for uci_move in clicked_path {
                                if !self.apply_engine_move(&uci_move) {
                                    break; // Stop if a move couldn't be applied
                                }
                            }
                        }
                        
                        ui.separator();
                        
                        // Also show study panel
                        if self.state.mode == AppMode::Study {
                            self.study_panel.show(ui, &mut self.study);
                        }
                    }
                    AppMode::Game => {
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
                        
                        // Add PGN export button for finished games
                        if self.game.outcome() != GameOutcome::InProgress {
                            ui.separator();
                            if ui.button("📄 Export PGN").clicked() {
                                let pgn = self.export_game_pgn();
                                ui.ctx().copy_text(pgn);
                            }
                            if ui.button("📚 Save to Study").clicked() {
                                self.save_game_to_study();
                            }
                        }
                    }
                }
            });

        // Bottom panel for move list
        egui::TopBottomPanel::bottom("moves")
            .default_height(120.0)
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
            let can_interact = match self.state.mode {
                AppMode::Game => {
                    self.game.outcome() == GameOutcome::InProgress
                        && !self.engine_thinking
                        && self.game.turn() == self.state.player_color
                }
                AppMode::Analysis | AppMode::Study => {
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
        self.stop_analysis();
        
        let cmd_tx = self.engine_cmd_tx.clone();
        let _ = cmd_tx.send(EngineCommand::Quit);
    }
}
