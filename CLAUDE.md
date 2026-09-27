# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Rust + egui (eframe, glow backend) desktop chess GUI. Chess rules come from `shakmaty`; Stockfish is an **external** UCI process the user installs separately (not vendored). This crate builds `stockfish-chess`; `stockfish` is the engine. Effective MSRV is 1.88 (set by the locked dependencies, not by this crate's code).

## Commands

```bash
cargo run --locked                          # debug build is slow to play; add --release
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --locked take_back               # tests whose name contains a substring
cargo test --locked engine::discovery       # one module
cargo test --locked -- --ignored            # real-Stockfish game test (engine must be discoverable)
```

Environment variables (`STOCKFISH_PATH`, `RUST_LOG`, `STOCKFISH_CHESS_HERO`) use `VAR=value cmd` in bash; in PowerShell use `$env:VAR = "value"; cmd`. In Git Bash, `PATH` entries must be POSIX-style (`/c/...`); a `C:/...` entry is split at the drive colon.

CI (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings`, and tests on ubuntu-latest, windows-latest, and macos-latest using the **stable** toolchain, so a new stable clippy lint can fail CI without any code change.

Platform differences in tests: `stop_interrupts_a_silent_analysis` and `stopped_search_has_a_deadline` are `#[cfg(unix)]` (they run a shell-script fake engine); discovery tests use platform-specific file names.

### README hero screenshot

`STOCKFISH_CHESS_HERO=<output.png>` launches a scripted mode (`src/hero.rs`): Ruy Lopez position, Chess.com theme, Analysis mode. It waits for enough analysis depth, writes an egui viewport screenshot, and exits the process; `save` is skipped. It needs a working Stockfish. The capture is at the display's scale factor: the committed `assets/hero.png` is 2562×1912 from a HiDPI screen, and a 1× screen produces a much smaller image.

## Architecture

### App and engine threads

- `ChessApp` (`src/app.rs`) is the eframe app and the coordinator: it owns `GameState`, `Study`, the UI panels, and the engine channels. Mode switching, engine turns, draw offers, undo, navigation, study sync, and PGN export all live here.
- `EngineActor::spawn` (`src/engine/actor.rs`) runs a thread that owns the Stockfish `Child`; a second thread reads stdout lines into it. UI → engine is `EngineCommand`, engine → UI is `EngineEvent`, drained without blocking each frame in `process_engine_events`.
- While a search is active the actor queues incoming commands (a newer search replaces a queued one) and sends `stop`; `SetDifficulty` during a search is deferred. Timeouts turn a hung engine into `EngineEvent::Error` instead of a frozen UI.
- **Request IDs**: each `Go`/`Analyze` gets an ID from `activate_engine_request`, tagged with an `EngineRequestKind` (Game, Analysis, DrawOffer) and the side to move. Events whose ID doesn't match `active_engine_request` are dropped as stale. New engine interactions must go through this.
- **Positions** are sent as `GameState::start_fen()` plus `uci_moves_to_current()`, never as a bare current FEN, so Stockfish sees the history and can recognize repetitions.
- **Strength**: engine options persist between searches. Game searches send `SetDifficulty(state.difficulty)` first; analysis and draw-offer evaluation send `DifficultyLevel::Maximum`. Every level sets `Skill Level` explicitly, because Novice lowers it (`UCI_Elo` bottoms out at 1320) and it would otherwise stick.
- **Scores** from UCI are side-to-move relative; `score_for_color` with `ActiveEngineRequest.score_side_to_move` converts them (analysis shows White-relative; draw acceptance uses engine-relative, accepting at ≤ 50 cp).
- **Repaint**: egui only repaints on input or request. Analysis requests repaints on a timer, but in Game mode the engine's reply and engine start-up are delivered only because a `ui.spinner()` is on screen ("Engine thinking...", "Starting Stockfish..."). Removing those spinners leaves moves unapplied until the mouse moves.
- An engine best move is applied after `game.go_to_end()`, because the user may have browsed back through history while it searched.

### Modes

- **Game**: leaving Game mode stores the game in `saved_game`, and returning restores it (and resumes the engine's turn); only a first visit starts a new game.
- **Study**: the board is always rebuilt from the chapter tree by `sync_game_to_study` (replaying the current path from the chapter root), so `GameState` history mirrors the path. `synced_study` holds the `(study id, chapter, path)` last synced; any change from study panel actions, loading, new studies, or chapters triggers a resync. In Study mode, navigation moves the chapter path, not the game index.
- **Analysis**: clicking a PV move plays the line up to that move only if the panel's `base_fen` still equals the current position.

### Other structure

- UI components in `src/ui/` are immediate-mode widgets whose `show(...)` returns an optional action (`ControlAction`, `StudyNavAction`, a PV path) that `ChessApp` applies; they don't mutate app or engine state themselves. `ControlPanel` receives a `GameStatus` snapshot.
- The board (`src/ui/board.rs`) supports click and drag. It reports `drag_started` and a targeted square; `ChessApp` decides selection and legality. `square_at` maps pointer positions to squares, including when flipped.
- `GameState` (`src/game/state.rs`) wraps shakmaty: positions plus `MoveRecord`s (SAN/UCI/FEN), a browsable `current_index`, automatic draw detection, and `take_back_to(color)` for undo.
- Engine discovery (`src/engine/discovery.rs`): `STOCKFISH_PATH` (hard error if set but unusable), then the app's directory, CWD, `~/bin`, Unix bin directories, and `PATH`. Any file named `stockfish*` counts, except this app's own outputs (`stockfish-chess*`, `stockfish_chess*`); on Windows it must also end in `.exe`, since there is no executable bit to filter `stockfish-chess.d` and `.pdb` build files. A discovery failure doesn't abort startup; the error is shown in the sidebar.
- `AppState` (difficulty, theme, color, flip, mode) is persisted by eframe under `APP_KEY` and is `#[serde(default)]`, so new fields need defaults for old saves to load. Studies are JSON files in `dirs::data_dir()/Stockfish-Chess/studies/`.
- Piece SVGs are embedded via `include_str!` and rasterized with resvg/tiny-skia.
