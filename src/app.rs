use crate::engine::{discover_stockfish, DifficultyLevel, EngineActor, EngineCommand, EngineEvent};
use crate::game::{GameOutcome, GameState, MoveRecord, PlayerColor};
use crate::hero::HeroShot;
use crate::study::Study;
use crate::ui::{
    AnalysisPanel, ChessBoard, ControlAction, ControlPanel, MoveList, PieceRenderer,
    StudyNavAction, StudyPanel, Theme,
};
use serde::{Deserialize, Serialize};
use shakmaty::{Color, Move, Role, Square};
use std::path::PathBuf;
use std::sync::mpsc;

const DRAW_ACCEPTANCE_THRESHOLD_CP: i32 = 50;

fn score_for_color(score: i32, score_perspective: PlayerColor, color: PlayerColor) -> i32 {
    if score_perspective == color {
        score
    } else {
        score.saturating_neg()
    }
}

fn should_accept_draw(engine_score_cp: Option<i32>) -> bool {
    engine_score_cp.is_some_and(|score| score <= DRAW_ACCEPTANCE_THRESHOLD_CP)
}

fn promotion_label(role: Role, color: Color) -> &'static str {
    match (color, role) {
        (Color::White, Role::Queen) => "♕ Queen",
        (Color::White, Role::Rook) => "♖ Rook",
        (Color::White, Role::Bishop) => "♗ Bishop",
        (Color::White, Role::Knight) => "♘ Knight",
        (Color::Black, Role::Queen) => "♛ Queen",
        (Color::Black, Role::Rook) => "♜ Rook",
        (Color::Black, Role::Bishop) => "♝ Bishop",
        (Color::Black, Role::Knight) => "♞ Knight",
        (_, Role::Pawn | Role::King) => unreachable!("invalid promotion role"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineRequestKind {
    Game,
    Analysis,
    DrawOffer,
}

#[derive(Debug, Clone, Copy)]
struct ActiveEngineRequest {
    id: u64,
    kind: EngineRequestKind,
    score_side_to_move: PlayerColor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    #[default]
    Game,
    Analysis,
    Study,
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
    pending_promotion: Option<Vec<Move>>,

    // Engine state
    engine_cmd_tx: mpsc::Sender<EngineCommand>,
    engine_event_rx: mpsc::Receiver<EngineEvent>,
    engine_ready: bool,
    engine_error: Option<String>,
    engine_thinking: bool,
    engine_analyzing: bool,
    next_engine_request_id: u64,
    active_engine_request: Option<ActiveEngineRequest>,

    // Analysis
    analysis_panel: AnalysisPanel,

    draw_offer_score: Option<i32>,

    // Study
    study: Study,
    study_panel: StudyPanel,
    hero: Option<HeroShot>,
}

impl ChessApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load persisted state
        let state: AppState = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();

        let (stockfish_path, engine_error) = match discover_stockfish() {
            Ok(path) => {
                tracing::info!("Discovered Stockfish at {}", path.display());
                (Some(path), None)
            }
            Err(error) => {
                tracing::error!("{error}");
                (None, Some(error.to_string()))
            }
        };
        let should_initialize_engine = stockfish_path.is_some();
        let (engine_cmd_tx, engine_event_rx) =
            EngineActor::spawn(stockfish_path.unwrap_or_else(|| PathBuf::from("stockfish")));

        // Send init command
        if should_initialize_engine {
            let _ = engine_cmd_tx.send(EngineCommand::Init);
        }

        let mut app = Self {
            game: GameState::new(),
            state,
            piece_renderer: PieceRenderer::new(),
            selected_square: None,
            legal_moves_for_selected: Vec::new(),
            pending_promotion: None,
            engine_cmd_tx,
            engine_event_rx,
            engine_ready: false,
            engine_error,
            engine_thinking: false,
            engine_analyzing: false,
            next_engine_request_id: 1,
            active_engine_request: None,
            analysis_panel: AnalysisPanel::default(),
            draw_offer_score: None,
            study: Study::new("Untitled Study".to_string()),
            study_panel: StudyPanel::default(),
            hero: None,
        };

        app.clear_selection();
        if let Some(hero) = HeroShot::from_env() {
            app.state.theme = Theme::ChessCom;
            app.state.mode = AppMode::Analysis;
            app.game = HeroShot::opening_position();
            app.select_square(HeroShot::selected_square());
            app.hero = Some(hero);
        }
        app
    }

    fn clear_selection(&mut self) {
        self.selected_square = None;
        self.legal_moves_for_selected.clear();
        self.pending_promotion = None;
    }

    fn activate_engine_request(&mut self, kind: EngineRequestKind) -> u64 {
        let id = self.next_engine_request_id;
        self.next_engine_request_id = self.next_engine_request_id.wrapping_add(1).max(1);
        self.active_engine_request = Some(ActiveEngineRequest {
            id,
            kind,
            score_side_to_move: self.game.turn(),
        });
        id
    }

    fn cancel_active_engine_search(&mut self) {
        if self.active_engine_request.take().is_some() {
            let _ = self.engine_cmd_tx.send(EngineCommand::Stop);
        }
        self.engine_thinking = false;
        self.engine_analyzing = false;
        self.analysis_panel.is_analyzing = false;
        self.draw_offer_score = None;
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

    fn handle_move_candidates(&mut self, moves: Vec<Move>) {
        match moves.as_slice() {
            [] => {}
            [m] => {
                self.make_move(*m);
            }
            _ if moves.iter().all(|m| m.is_promotion()) => {
                self.pending_promotion = Some(moves);
            }
            _ => {
                tracing::error!(
                    "Multiple non-promotion moves shared one destination: {:?}",
                    moves
                );
            }
        }
    }

    fn show_promotion_picker(&mut self, ctx: &egui::Context) {
        let Some(options) = self.pending_promotion.clone() else {
            return;
        };
        let promoting_color = options
            .first()
            .and_then(|m| m.from())
            .and_then(|square| self.game.piece_at(square))
            .map(|(_, color)| color)
            .unwrap_or(Color::White);

        let (selected, cancel) = egui::Modal::new(egui::Id::new("promotion_picker"))
            .show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.heading("Promote pawn");
                ui.label("Choose the piece for this promotion.");
                ui.add_space(8.0);

                let mut selected = None;
                ui.horizontal_wrapped(|ui| {
                    for role in [Role::Queen, Role::Rook, Role::Bishop, Role::Knight] {
                        if let Some(m) = options.iter().find(|m| m.promotion() == Some(role)) {
                            if ui.button(promotion_label(role, promoting_color)).clicked() {
                                selected = Some(*m);
                            }
                        }
                    }
                });

                ui.add_space(8.0);
                let cancel = ui.button("Cancel").clicked();
                (selected, cancel)
            })
            .inner;

        let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if let Some(m) = selected {
            self.pending_promotion = None;
            self.make_move(m);
        } else if cancel || escape_pressed {
            self.clear_selection();
        }
    }

    fn make_move(&mut self, m: Move) -> Option<MoveRecord> {
        if let Ok(record) = self.game.make_move(m) {
            self.clear_selection();

            // In study mode, add to study tree
            if self.state.mode == AppMode::Study {
                self.study
                    .current_chapter_mut()
                    .add_move(record.clone(), self.game.fen());
                self.study.update_timestamp();
            }

            if self.engine_analyzing {
                self.restart_analysis();
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
        let request_id = self.activate_engine_request(EngineRequestKind::Game);
        let fen = self.game.fen();
        let moves: Vec<String> = Vec::new();

        let _ = self.engine_cmd_tx.send(EngineCommand::SetMultiPV(1));
        let _ = self.engine_cmd_tx.send(EngineCommand::Go {
            request_id,
            fen,
            moves,
            movetime_ms: Some(1000),
        });
    }

    fn start_analysis(&mut self) {
        if !self.engine_ready || self.engine_analyzing {
            return;
        }

        self.engine_analyzing = true;
        let request_id = self.activate_engine_request(EngineRequestKind::Analysis);
        self.analysis_panel.is_analyzing = true;
        self.analysis_panel.clear();
        // Store the base position where analysis started - all engine lines are relative to this
        self.analysis_panel.base_fen = Some(self.game.fen());

        let fen = self.game.fen();
        let moves: Vec<String> = Vec::new();
        // Always calculate max (5) lines, just display fewer
        let max_lines = 5;

        let _ = self
            .engine_cmd_tx
            .send(EngineCommand::SetMultiPV(max_lines));
        let _ = self.engine_cmd_tx.send(EngineCommand::Analyze {
            request_id,
            fen,
            moves,
        });
    }

    fn stop_analysis(&mut self) {
        if self.engine_analyzing {
            self.engine_analyzing = false;
            self.analysis_panel.is_analyzing = false;
            if self
                .active_engine_request
                .is_some_and(|request| request.kind == EngineRequestKind::Analysis)
            {
                self.active_engine_request = None;
            }
            let _ = self.engine_cmd_tx.send(EngineCommand::Stop);
        }
    }

    fn restart_analysis(&mut self) {
        if self.engine_analyzing {
            self.stop_analysis();
            self.start_analysis();
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
                    self.engine_error = None;

                    let difficulty = self.state.difficulty;
                    let _ = self
                        .engine_cmd_tx
                        .send(EngineCommand::SetDifficulty(difficulty));

                    if self.state.mode == AppMode::Game {
                        self.check_engine_turn();
                    }
                }
                EngineEvent::BestMove {
                    request_id,
                    best_move,
                    ponder,
                } => {
                    let Some(request) = self
                        .active_engine_request
                        .filter(|request| request.id == request_id)
                    else {
                        tracing::debug!("Ignoring stale best move for engine request {request_id}");
                        continue;
                    };
                    self.active_engine_request = None;
                    tracing::info!("Engine best move for request {request_id}: {best_move}");
                    if let Some(ponder) = ponder {
                        tracing::debug!("Engine ponder move: {ponder}");
                    }

                    match request.kind {
                        EngineRequestKind::Game => {
                            self.engine_thinking = false;
                            if self.game.outcome() == GameOutcome::InProgress {
                                if let Err(error) = self.game.make_move_uci(&best_move) {
                                    tracing::error!("Failed to apply engine move: {error}");
                                    self.engine_error = Some(format!(
                                        "Stockfish returned an invalid move: {best_move}"
                                    ));
                                }
                            } else {
                                tracing::debug!("Ignoring engine move after game ended");
                            }
                        }
                        EngineRequestKind::DrawOffer => {
                            self.engine_thinking = false;
                            if self.game.outcome() == GameOutcome::InProgress {
                                let accept_draw = should_accept_draw(self.draw_offer_score);
                                if accept_draw {
                                    self.game.agree_to_draw();
                                    tracing::info!(
                                        "Draw accepted at engine-relative score {:?} cp",
                                        self.draw_offer_score
                                    );
                                } else {
                                    tracing::info!(
                                        "Draw declined at engine-relative score {:?} cp",
                                        self.draw_offer_score
                                    );
                                }
                            } else {
                                tracing::debug!("Ignoring draw result after game ended");
                            }
                            self.draw_offer_score = None;
                        }
                        EngineRequestKind::Analysis => {
                            self.engine_analyzing = false;
                            self.analysis_panel.is_analyzing = false;
                        }
                    }

                    ctx.request_repaint();
                }
                EngineEvent::Info {
                    request_id,
                    depth,
                    score_cp,
                    score_mate,
                    pv,
                    nodes,
                    time_ms,
                    multipv,
                } => {
                    let Some(request) = self
                        .active_engine_request
                        .filter(|request| request.id == request_id)
                    else {
                        tracing::trace!("Ignoring stale info for engine request {request_id}");
                        continue;
                    };
                    tracing::trace!("Engine search time: {:?} ms", time_ms);

                    match request.kind {
                        EngineRequestKind::Analysis => {
                            let line_id = multipv.unwrap_or(1);
                            let white_score_cp = score_cp.map(|score| {
                                score_for_color(
                                    score,
                                    request.score_side_to_move,
                                    PlayerColor::White,
                                )
                            });
                            let white_score_mate = score_mate.map(|score| {
                                score_for_color(
                                    score,
                                    request.score_side_to_move,
                                    PlayerColor::White,
                                )
                            });
                            self.analysis_panel.update_line(
                                line_id,
                                white_score_cp,
                                white_score_mate,
                                depth,
                                pv,
                            );
                            if let Some(nodes) = nodes {
                                self.analysis_panel.total_nodes = nodes;
                            }
                        }
                        EngineRequestKind::DrawOffer if multipv.unwrap_or(1) == 1 => {
                            let engine_color = self.state.player_color.opposite();
                            let raw_score =
                                score_mate.map(|mate| mate.signum() * 10_000).or(score_cp);
                            self.draw_offer_score = raw_score.map(|score| {
                                score_for_color(score, request.score_side_to_move, engine_color)
                            });
                        }
                        EngineRequestKind::Game | EngineRequestKind::DrawOffer => {}
                    }
                }
                EngineEvent::Error(e) => {
                    tracing::error!("Engine error: {}", e);
                    self.engine_ready = false;
                    self.engine_error = Some(e);
                    self.engine_thinking = false;
                    self.engine_analyzing = false;
                    self.active_engine_request = None;
                    self.draw_offer_score = None;
                    self.analysis_panel.is_analyzing = false;
                    ctx.request_repaint();
                }
                EngineEvent::Terminated => {
                    tracing::warn!("Engine terminated");
                    self.engine_ready = false;
                    self.engine_error
                        .get_or_insert_with(|| "Stockfish stopped unexpectedly".to_string());
                    self.engine_thinking = false;
                    self.engine_analyzing = false;
                    self.active_engine_request = None;
                    self.draw_offer_score = None;
                    self.analysis_panel.is_analyzing = false;
                    ctx.request_repaint();
                }
            }
        }
    }

    fn new_game(&mut self) {
        let restart_analysis = self.engine_analyzing;
        self.cancel_active_engine_search();
        self.game.reset();
        self.clear_selection();

        let _ = self.engine_cmd_tx.send(EngineCommand::NewGame);

        if self.state.mode == AppMode::Game && self.state.player_color == PlayerColor::Black {
            self.check_engine_turn();
        }

        if matches!(self.state.mode, AppMode::Analysis | AppMode::Study) && restart_analysis {
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
                let _ = self.engine_cmd_tx.send(EngineCommand::SetDifficulty(level));
            }
            ControlAction::SetTheme(theme) => {
                tracing::info!("Setting theme to: {:?}", theme);
                self.state.theme = theme;
            }
            ControlAction::SetPlayerColor(color) => {
                self.state.player_color = color;
                self.new_game();
            }
            ControlAction::Resign => {
                self.cancel_active_engine_search();
                self.game.resign(self.state.player_color);
            }
            ControlAction::OfferDraw => {
                self.check_draw_offer();
            }
            ControlAction::Undo => {
                // Undo last two moves (player and engine)
                self.undo_last_moves();
            }
        }
    }

    fn check_draw_offer(&mut self) {
        if self.engine_ready && !self.engine_thinking && !self.engine_analyzing {
            let fen = self.game.fen();
            self.engine_thinking = true;
            self.draw_offer_score = None;
            let request_id = self.activate_engine_request(EngineRequestKind::DrawOffer);

            // Request a quick evaluation
            let _ = self.engine_cmd_tx.send(EngineCommand::SetMultiPV(1));
            let _ = self.engine_cmd_tx.send(EngineCommand::Go {
                request_id,
                fen,
                moves: Vec::new(),
                movetime_ms: Some(500), // 500ms quick eval
            });
        }
    }

    fn show_engine_status(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.engine_error {
            ui.group(|ui| {
                ui.colored_label(egui::Color32::RED, "Stockfish unavailable");
                ui.small(error);
                ui.small("Set STOCKFISH_PATH to a valid engine executable, then restart.");
            });
            ui.separator();
        } else if !self.engine_ready {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Starting Stockfish...");
            });
            ui.separator();
        }
    }

    fn undo_last_moves(&mut self) {
        // Undo the last two moves (player's move and engine's response)
        // First, if engine is thinking, stop it
        if self.active_engine_request.is_some() {
            self.cancel_active_engine_search();
        }

        // Undo moves until it's the player's turn again
        let target_turn = self.state.player_color;
        let mut undone = 0;

        while self.game.turn() != target_turn && self.game.can_go_back() {
            if self.game.go_back().is_ok() {
                undone += 1;
            } else {
                break;
            }
        }

        // Also remove the moves from history if we're at the end
        if !self.game.can_go_forward() && undone > 0 {
            // Truncate history
            for _ in 0..undone {
                self.game.undo_last_move();
            }
        }

        self.clear_selection();
        tracing::info!("Undid {} moves", undone);
    }

    fn handle_study_nav_action(&mut self, action: StudyNavAction) {
        match action {
            StudyNavAction::GoToPosition(path) => {
                // Navigate study chapter to the specified path
                let chapter = self.study.current_chapter_mut();
                chapter.current_path = path.clone();

                // Update the game to match the new position
                let fen = chapter.current_fen().to_string();
                if let Ok(new_game) = GameState::from_fen(&fen) {
                    self.game = new_game;
                    self.clear_selection();
                    tracing::info!("Navigated to study position: {:?}", path);
                }

                // Restart analysis if active
                if self.engine_analyzing {
                    self.restart_analysis();
                }
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

            if self.engine_analyzing {
                self.restart_analysis();
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

            if self.engine_analyzing {
                self.restart_analysis();
            }
        }
    }

    fn go_to_start(&mut self) {
        self.clear_selection();
        self.game.go_to_start();

        if self.state.mode == AppMode::Study {
            self.study.current_chapter_mut().go_to_start();
        }

        if self.engine_analyzing {
            self.restart_analysis();
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

        if self.engine_analyzing {
            self.restart_analysis();
        }
    }

    fn set_mode(&mut self, mode: AppMode) {
        if self.state.mode != mode {
            self.state.mode = mode;

            self.cancel_active_engine_search();

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
                        return true;
                    }
                }
            }
        }
        false
    }

    fn apply_engine_path(&mut self, base_fen: &str, path: Vec<String>) {
        let restart_analysis = self.engine_analyzing;
        if restart_analysis {
            self.stop_analysis();
        }

        if !base_fen.is_empty() {
            match GameState::from_fen(base_fen) {
                Ok(new_game) => {
                    self.game = new_game;
                    tracing::info!("Reset to base position for analysis line");
                }
                Err(error) => {
                    tracing::error!("Invalid analysis base position: {error}");
                    if restart_analysis {
                        self.start_analysis();
                    }
                    return;
                }
            }
        }

        tracing::info!("Playing engine path: {:?}", path);
        for uci_move in path {
            if !self.apply_engine_move(&uci_move) {
                break;
            }
        }

        if restart_analysis {
            self.start_analysis();
        }
    }

    /// Export current game as PGN
    fn export_game_pgn(&self) -> String {
        use chrono::Local;

        let mut pgn = String::new();

        // Headers
        pgn.push_str("[Event \"Stockfish Chess Game\"]\n");
        pgn.push_str("[Site \"Local\"]\n");
        pgn.push_str(&format!("[Date \"{}\"]\n", Local::now().format("%Y.%m.%d")));
        pgn.push_str("[Round \"-\"]\n");
        pgn.push_str("[White \"Player\"]\n");
        pgn.push_str("[Black \"Stockfish\"]\n");

        // Result
        let result = match self.game.outcome() {
            GameOutcome::Checkmate(PlayerColor::White)
            | GameOutcome::Resignation(PlayerColor::White) => "1-0",
            GameOutcome::Checkmate(PlayerColor::Black)
            | GameOutcome::Resignation(PlayerColor::Black) => "0-1",
            GameOutcome::Stalemate
            | GameOutcome::InsufficientMaterial
            | GameOutcome::ThreefoldRepetition
            | GameOutcome::FiftyMoveRule
            | GameOutcome::DrawByAgreement => "1/2-1/2",
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
        let mut new_study = Study::new(format!(
            "Game {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M")
        ));

        // Replay all moves into the study
        let moves = self.game.move_history().to_vec();
        for record in moves {
            let resulting_fen = record.resulting_fen.clone();
            new_study
                .current_chapter_mut()
                .add_move(record, resulting_fen);
        }

        self.study = new_study;
        self.state.mode = AppMode::Study;
        tracing::info!("Game saved to new study");
    }
}

impl eframe::App for ChessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_engine_events(ctx);

        if let Some(hero) = self.hero.as_mut() {
            hero.tick(
                ctx,
                self.analysis_panel.all_lines.len(),
                self.analysis_panel.current_depth,
            );
        }
        if self.hero.is_some() && self.engine_ready && !self.engine_analyzing {
            if self
                .hero
                .as_mut()
                .is_some_and(HeroShot::should_start_analysis)
            {
                self.start_analysis();
            }
        }

        if self.engine_analyzing {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.hero.is_some() {
            ctx.request_repaint();
        }

        // Side panel for controls, analysis, or study
        egui::SidePanel::left("sidebar")
            .default_width(240.0)
            .show(ctx, |ui| {
                // Mode selector
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    if ui
                        .selectable_label(self.state.mode == AppMode::Game, "🎮")
                        .clicked()
                    {
                        self.set_mode(AppMode::Game);
                    }
                    if ui
                        .selectable_label(self.state.mode == AppMode::Analysis, "📊")
                        .clicked()
                    {
                        self.set_mode(AppMode::Analysis);
                    }
                    if ui
                        .selectable_label(self.state.mode == AppMode::Study, "📚")
                        .clicked()
                    {
                        self.set_mode(AppMode::Study);
                    }
                });
                ui.separator();
                self.show_engine_status(ui);

                // Navigation controls
                if self.state.mode != AppMode::Game
                    || self.game.can_go_back()
                    || self.game.can_go_forward()
                {
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

                    ui.label(format!(
                        "Move: {} / {}",
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
                            let analyze_button = ui
                                .add_enabled(
                                    self.engine_ready,
                                    egui::Button::new(if self.engine_analyzing {
                                        "⏹ Stop"
                                    } else {
                                        "▶ Analyze"
                                    }),
                                )
                                .on_hover_text("Stockfish must be available for analysis");
                            if analyze_button.clicked() {
                                self.toggle_analysis();
                            }
                        });
                        ui.separator();

                        // Show analysis panel and handle clicked moves
                        if let Some((base_fen, path)) = self.analysis_panel.show(ui) {
                            self.apply_engine_path(&base_fen, path);
                        }

                        ui.separator();

                        // Also show study panel
                        if self.state.mode == AppMode::Study {
                            if let Some(nav_action) = self.study_panel.show(ui, &mut self.study) {
                                self.handle_study_nav_action(nav_action);
                            }
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
                            self.engine_ready,
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

            if can_interact {
                if response.move_candidates.is_empty() {
                    if let Some(square) = response.square_clicked {
                        self.select_square(square);
                    }
                } else {
                    self.handle_move_candidates(response.move_candidates);
                }
            }
        });

        self.show_promotion_picker(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.hero.is_some() {
            return;
        }
        eframe::set_value(storage, eframe::APP_KEY, &self.state);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_analysis();

        let cmd_tx = self.engine_cmd_tx.clone();
        let _ = cmd_tx.send(EngineCommand::Quit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_oriented_to_requested_color() {
        assert_eq!(
            score_for_color(125, PlayerColor::White, PlayerColor::White),
            125
        );
        assert_eq!(
            score_for_color(125, PlayerColor::Black, PlayerColor::White),
            -125
        );
        assert_eq!(
            score_for_color(-80, PlayerColor::White, PlayerColor::Black),
            80
        );
    }

    #[test]
    fn draw_acceptance_uses_engine_relative_score() {
        assert!(should_accept_draw(Some(-200)));
        assert!(should_accept_draw(Some(0)));
        assert!(should_accept_draw(Some(DRAW_ACCEPTANCE_THRESHOLD_CP)));
        assert!(!should_accept_draw(Some(DRAW_ACCEPTANCE_THRESHOLD_CP + 1)));
        assert!(!should_accept_draw(None));
    }

    #[test]
    fn promotion_labels_match_the_pawn_color() {
        assert_eq!(promotion_label(Role::Queen, Color::White), "♕ Queen");
        assert_eq!(promotion_label(Role::Queen, Color::Black), "♛ Queen");
        assert_eq!(promotion_label(Role::Knight, Color::Black), "♞ Knight");
    }
}
