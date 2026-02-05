# Stockfish Chess

A beautiful, cross-platform chess game with Stockfish AI integration, built with Rust and egui.

## Features

- 🎨 **Beautiful GUI** - Clean, modern interface built with egui
- 🤖 **Stockfish Integration** - Play against the world-class Stockfish engine
- 🎯 **Adjustable Difficulty** - Choose from multiple skill levels
- 🎭 **Themes** - Classic, Lichess, Chess.com, and Dark themes
- ♟️ **Legal Move Highlighting** - Visual indicators for valid moves
- 📋 **Move History** - Track all moves in standard algebraic notation
- 🔄 **Board Flip** - View the board from either side
- 💾 **Persistent Settings** - Your preferences are saved automatically

## Prerequisites

### 1. Rust Toolchain

Install Rust (1.75.0 or later):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Stockfish Engine

This application requires the Stockfish chess engine binary. **The binary is NOT included** in this repository.

#### Download Stockfish

1. Visit [Stockfish Downloads](https://stockfishchess.org/download/)
2. Download the appropriate binary for your platform:
   - **macOS (Apple Silicon)**: `stockfish-macos-m1-apple-silicon`
   - **macOS (Intel)**: `stockfish-macos-x86-64`
   - **Linux**: `stockfish-ubuntu-x86-64`
   - **Windows**: `stockfish-windows-x86-64.exe`

#### Setup

Place the Stockfish binary in one of these locations (searched in order):

1. `./stockfish` (same directory as the executable)
2. `~/bin/stockfish`
3. `/usr/local/bin/stockfish`
4. `/opt/homebrew/bin/stockfish` (macOS Homebrew)
5. System PATH (as `stockfish`)

Or set a custom path by modifying `src/app.rs`:

```rust
let stockfish_path = [
    "/path/to/your/stockfish",
    // ... other paths
];
```

#### macOS Quarantine Notice

If you downloaded Stockfish via browser, macOS may have quarantined it. Remove the quarantine attribute:

```bash
xattr -d com.apple.quarantine /path/to/stockfish
```

## Building

### Debug Build

```bash
cargo build
```

### Release Build (Optimized)

```bash
cargo build --release
```

The executable will be at:
- Debug: `target/debug/stockfish-chess`
- Release: `target/release/stockfish-chess`

## Running

### From Cargo

```bash
cargo run
```

### Direct Execution

```bash
# Debug build
./target/debug/stockfish-chess

# Release build
./target/release/stockfish-chess
```

## How to Play

1. **Select a piece** - Click on any of your pieces (White starts)
2. **View legal moves** - Valid destinations are shown with dots
3. **Make a move** - Click on a highlighted square to move
4. **Engine responds** - Stockfish will automatically play as Black

### Controls

- **New Game** - Start a fresh game
- **Flip Board** - Rotate the board 180°
- **Play as** - Switch between White and Black (starts new game)
- **Difficulty** - Adjust Stockfish strength:
  - Beginner (800 Elo)
  - Casual (1200 Elo)
  - Intermediate (1600 Elo)
  - Advanced (2000 Elo)
  - Expert (2400 Elo)
  - Grandmaster (2800 Elo)
  - Maximum (3200+ Elo)
- **Theme** - Change board appearance

## Architecture

```
src/
├── main.rs          # Application entry point
├── app.rs           # Main app state and UI coordination
├── engine/          # Stockfish engine integration
│   ├── mod.rs
│   ├── actor.rs     # Engine process communication
│   └── difficulty.rs # Difficulty level definitions
├── game/            # Chess game logic
│   ├── mod.rs
│   └── state.rs     # Game state management
└── ui/              # User interface components
    ├── mod.rs
    ├── board.rs     # Chess board rendering & interaction
    ├── controls.rs  # Control panel (buttons, dropdowns)
    ├── move_list.rs # Move history display
    ├── pieces.rs    # SVG piece rendering
    └── theme.rs     # Color themes
```

### Key Dependencies

- **egui/eframe** - Immediate-mode GUI framework
- **shakmaty** - Chess move generation and validation
- **resvg/tiny-skia** - SVG rendering for chess pieces
- **tokio** - Async runtime (for UI responsiveness)
- **serde** - Settings serialization

## Development

### Project Structure

- `src/assets/pieces/` - SVG chess piece graphics (embedded at compile time)
- `src/engine/` - UCI protocol implementation for Stockfish communication
- `src/game/` - Chess rules and state management
- `src/ui/` - egui components for the interface

### Engine Communication

The app communicates with Stockfish via the [UCI (Universal Chess Interface)](https://wbec-ridderkerk.nl/html/UCIProtocol.html) protocol:

1. Spawns Stockfish as a child process
2. Sends commands via stdin (`uci`, `position`, `go`, etc.)
3. Parses responses from stdout (`uciok`, `bestmove`, `info`, etc.)
4. Runs in a dedicated thread to avoid blocking the UI

### Adding Features

The modular architecture makes it easy to extend:

- **New themes**: Add variants to `src/ui/theme.rs`
- **New difficulties**: Modify `src/engine/difficulty.rs`
- **UI components**: Add modules in `src/ui/`

## Troubleshooting

### "Engine not found" error

- Ensure Stockfish binary is in one of the expected locations
- Check that the binary has execute permissions: `chmod +x stockfish`
- On macOS, remove quarantine: `xattr -d com.apple.quarantine stockfish`

### "Engine closed stdout unexpectedly"

- The Stockfish binary may be incompatible with your system
- Try downloading a different version from the official site
- Check system architecture matches (ARM vs x86_64)

### Pieces not rendering

- Ensure SVG assets are present at `src/assets/pieces/`
- Check for console errors about missing textures

### Slow performance

- Build in release mode: `cargo build --release`
- Reduce Stockfish thinking time in `src/app.rs` (default: 1000ms)

## License

MIT License - See [LICENSE](LICENSE) for details.

## Acknowledgments

- [Stockfish](https://stockfishchess.org/) - The powerful open-source chess engine
- [shakmaty](https://github.com/niklasf/shakmaty) - Rust chess library
- [egui](https://github.com/emilk/egui) - Immediate-mode GUI library
- Chess piece SVGs derived from [lichess-org/lila](https://github.com/lichess-org/lila) (CC0)

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.

### TODO / Future Features

- [ ] PGN import/export
- [ ] Opening book integration
- [ ] Analysis mode with engine evaluation
- [ ] Time controls (blitz, rapid, classical)
- [ ] Online multiplayer
- [ ] Custom board themes
- [ ] Move sound effects
