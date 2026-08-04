# Stockfish Chess

A cross-platform desktop chess application built with Rust and egui. Chess rules
are handled by `shakmaty`, while Stockfish runs as a separate UCI engine process.

## Features

- Play either color against Stockfish with seven difficulty levels
- Game, multi-line analysis, and study modes
- Legal move, last move, and check highlighting
- Queen, rook, bishop, and knight promotion selection
- Move history, undo, board flipping, and persistent preferences
- Four board themes
- Study chapters, comments, variations, JSON persistence, and PGN export
- User-visible engine startup and runtime errors

## Prerequisites

### Rust

Install Rust 1.75 or newer:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Stockfish

Download Stockfish from the
[official downloads page](https://stockfishchess.org/download/). The engine
binary is intentionally not included in this repository.

The app searches for an executable in this order:

1. The path in the `STOCKFISH_PATH` environment variable
2. Next to the application executable
3. The current working directory
4. `~/bin`
5. Common Homebrew and Unix binary directories
6. Every directory on the system `PATH`

Official filenames beginning with `stockfish`, such as
`stockfish-macos-m1-apple-silicon`, are recognized without being renamed.

To use a custom location:

```bash
STOCKFISH_PATH="/path/to/stockfish" cargo run
```

On macOS, a browser download may need executable permission and quarantine
removal:

```bash
chmod +x /path/to/stockfish
xattr -d com.apple.quarantine /path/to/stockfish
```

If discovery or startup fails, the full error and setup hint are displayed in
the application sidebar.

## Build and Run

```bash
cargo run
```

For an optimized build:

```bash
cargo build --release
./target/release/stockfish-chess
```

Enable application logs when troubleshooting:

```bash
RUST_LOG=info cargo run
```

## Playing

1. Choose White or Black under **Play as**.
2. Select a piece to display its legal destinations.
3. Select a destination to move.
4. When a pawn reaches the back rank, choose its promotion piece.
5. Stockfish responds automatically on its turn.

Draw offers are evaluated from Stockfish's perspective. Stockfish accepts when
it does not evaluate its own advantage above 0.50 pawns.

## Modes

- **Game** — play against Stockfish, adjust difficulty, resign, offer a draw,
  undo moves, and export completed games.
- **Analysis** — run continuous five-line Stockfish analysis and play moves
  from principal variations.
- **Study** — organize positions into chapters and variations, add comments,
  save studies locally, and export PGN.

## Difficulty Levels

- Novice (~1100)
- Beginner (~1350)
- Casual (~1500)
- Intermediate (~1800)
- Advanced (~2100)
- Expert (~2500)
- Maximum strength

## Architecture

```text
src/
├── main.rs              Application entry point
├── app.rs               State, modes, engine coordination, and dialogs
├── engine/
│   ├── actor.rs         Background UCI process actor
│   ├── discovery.rs     Cross-platform Stockfish discovery
│   └── difficulty.rs    Strength presets
├── game/
│   └── state.rs         Rules, history, outcomes, FEN, SAN, and UCI
├── study/
│   └── mod.rs           Study tree and persistence
└── ui/
    ├── analysis.rs      Evaluation bar and principal variations
    ├── board.rs         Board rendering and interaction
    ├── controls.rs      Game controls
    ├── move_list.rs     Move history
    ├── pieces.rs        Embedded SVG rendering
    ├── study_panel.rs   Study controls
    └── theme.rs         Board themes
```

The egui thread sends commands over an `mpsc` channel to a dedicated engine
thread. That thread owns the Stockfish child process and communicates through
UCI over stdin/stdout. The UI remains responsive while Stockfish searches.

## Development

Run the quality checks:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
```

Tests cover game state, promotion choices, UCI command construction and parsing,
engine discovery, score orientation, and draw acceptance.

## Troubleshooting

### Stockfish unavailable

- Confirm `STOCKFISH_PATH` points to a file, not a directory.
- Confirm the binary is executable.
- Confirm the binary matches the machine architecture.
- Read the detailed startup error in the sidebar or run with `RUST_LOG=info`.

### Pieces do not render

Ensure all SVG files are present under `src/assets/pieces/`.

### Slow debug performance

Use `cargo run --release`; release builds enable LTO.

## Roadmap

- PGN import
- Opening-book support
- Time controls
- Online multiplayer
- Move sounds

## License

MIT. See [LICENSE](LICENSE).

## Acknowledgments

- [Stockfish](https://stockfishchess.org/)
- [shakmaty](https://github.com/niklasf/shakmaty)
- [egui](https://github.com/emilk/egui)
- Piece SVGs derived from [lichess-org/lila](https://github.com/lichess-org/lila)
  (CC0)
