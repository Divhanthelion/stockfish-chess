use crate::engine::difficulty::DifficultyLevel;
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Clone)]
pub enum EngineCommand {
    Init,
    SetDifficulty(DifficultyLevel),
    NewGame,
    Go {
        fen: String,
        moves: Vec<String>,
        movetime_ms: Option<u64>,
    },
    Stop,
    Quit,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Ready,
    BestMove {
        best_move: String,
        ponder: Option<String>,
    },
    Info {
        depth: Option<u32>,
        score_cp: Option<i32>,
        score_mate: Option<i32>,
        pv: Vec<String>,
        nodes: Option<u64>,
        time_ms: Option<u64>,
    },
    Error(String),
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineState {
    Uninitialized,
    Initializing,
    Idle,
    Thinking,
    Terminated,
}

pub struct EngineActor {
    cmd_rx: mpsc::Receiver<EngineCommand>,
    event_tx: mpsc::Sender<EngineEvent>,
    state: EngineState,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: Option<BufReader<ChildStdout>>,
    child: Option<Child>,
    difficulty: DifficultyLevel,
}

impl EngineActor {
    pub fn spawn(stockfish_path: Option<String>) -> (mpsc::Sender<EngineCommand>, mpsc::Receiver<EngineEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCommand>();
        let (event_tx, event_rx) = mpsc::channel::<EngineEvent>();

        let path = stockfish_path.unwrap_or_else(|| "stockfish".to_string());
        tracing::info!("EngineActor spawn with path: {}", path);

        thread::spawn(move || {
            let mut actor = EngineActor {
                cmd_rx,
                event_tx,
                state: EngineState::Uninitialized,
                stdin: None,
                stdout: None,
                child: None,
                difficulty: DifficultyLevel::default(),
            };
            actor.run(path);
        });

        (cmd_tx, event_rx)
    }

    fn run(&mut self, stockfish_path: String) {
        tracing::info!("EngineActor run loop started");
        loop {
            let cmd = match self.cmd_rx.recv() {
                Ok(cmd) => {
                    tracing::debug!("Received command: {:?}", cmd);
                    cmd
                }
                Err(_) => {
                    tracing::info!("Command channel closed, shutting down engine");
                    break;
                }
            };

            match cmd {
                EngineCommand::Init => {
                    if let Err(e) = self.init(&stockfish_path) {
                        tracing::error!("Engine init failed: {}", e);
                        let _ = self.event_tx.send(EngineEvent::Error(e.to_string()));
                    }
                }
                EngineCommand::SetDifficulty(level) => {
                    self.difficulty = level;
                    if let Err(e) = self.apply_difficulty() {
                        let _ = self.event_tx.send(EngineEvent::Error(e.to_string()));
                    }
                }
                EngineCommand::NewGame => {
                    if let Err(e) = self.new_game() {
                        let _ = self.event_tx.send(EngineEvent::Error(e.to_string()));
                    }
                }
                EngineCommand::Go { fen, moves, movetime_ms } => {
                    if let Err(e) = self.go(&fen, &moves, movetime_ms) {
                        let _ = self.event_tx.send(EngineEvent::Error(e.to_string()));
                    }
                }
                EngineCommand::Stop => {
                    if let Err(e) = self.stop() {
                        let _ = self.event_tx.send(EngineEvent::Error(e.to_string()));
                    }
                }
                EngineCommand::Quit => {
                    let _ = self.quit();
                    break;
                }
            }
        }

        self.state = EngineState::Terminated;
        let _ = self.event_tx.send(EngineEvent::Terminated);
    }

    fn init(&mut self, stockfish_path: &str) -> Result<()> {
        tracing::info!("Initializing Stockfish at: {}", stockfish_path);

        if !std::path::Path::new(stockfish_path).exists() {
            anyhow::bail!("Stockfish binary not found at: {}", stockfish_path);
        }
        tracing::info!("Stockfish binary exists");

        let stockfish_dir = std::path::Path::new(stockfish_path).parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        tracing::info!("Setting working directory to: {:?}", stockfish_dir);

        tracing::info!("Spawning Stockfish process...");
        let mut child = Command::new(stockfish_path)
            .current_dir(&stockfish_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn Stockfish process")?;
        tracing::info!("Stockfish process spawned successfully with PID: {:?}", child.id());

        let stdin = child.stdin.take().context("No stdin")?;
        let stdout = child.stdout.take().context("No stdout")?;
        tracing::info!("Got stdin and stdout handles");

        self.stdin = Some(BufWriter::new(stdin));
        self.stdout = Some(BufReader::new(stdout));
        self.child = Some(child);

        self.state = EngineState::Initializing;
        tracing::info!("Sending UCI command...");

        self.send_command("uci")?;
        tracing::info!("UCI command sent, waiting for uciok...");
        self.wait_for_response("uciok")?;
        tracing::info!("Got uciok!");

        tracing::info!("Sending isready...");
        self.send_command("isready")?;
        self.wait_for_response("readyok")?;
        tracing::info!("Got readyok!");

        self.state = EngineState::Idle;

        self.apply_difficulty()?;

        let _ = self.event_tx.send(EngineEvent::Ready);
        tracing::info!("Stockfish initialized successfully");

        Ok(())
    }

    fn apply_difficulty(&mut self) -> Result<()> {
        if self.stdin.is_none() {
            return Ok(());
        }

        for cmd in self.difficulty.uci_commands() {
            self.send_command(&cmd)?;
        }

        self.send_command("isready")?;
        self.wait_for_response("readyok")?;

        Ok(())
    }

    fn new_game(&mut self) -> Result<()> {
        self.send_command("ucinewgame")?;
        self.send_command("isready")?;
        self.wait_for_response("readyok")?;
        Ok(())
    }

    fn go(&mut self, fen: &str, _moves: &[String], movetime_ms: Option<u64>) -> Result<()> {
        let position_cmd = format!("position fen {}", fen);
        self.send_command(&position_cmd)?;

        let go_cmd = match movetime_ms {
            Some(ms) => format!("go movetime {}", ms),
            None => "go movetime 1000".to_string(),
        };

        self.state = EngineState::Thinking;
        self.send_command(&go_cmd)?;

        self.read_until_bestmove()?;

        self.state = EngineState::Idle;

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if self.state != EngineState::Thinking {
            return Ok(());
        }

        self.send_command("stop")?;
        self.read_until_bestmove()?;
        self.state = EngineState::Idle;

        Ok(())
    }

    fn quit(&mut self) -> Result<()> {
        let _ = self.send_command("quit");

        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
        Ok(())
    }

    fn send_command(&mut self, cmd: &str) -> Result<()> {
        let stdin = self.stdin.as_mut().context("No stdin available")?;
        tracing::debug!("Sending to engine: {}", cmd);
        writeln!(stdin, "{}", cmd)?;
        stdin.flush()?;
        Ok(())
    }

    fn wait_for_response(&mut self, expected: &str) -> Result<()> {
        let stdout = self.stdout.as_mut().context("No stdout available")?;
        let mut line = String::new();
        tracing::info!("Waiting for '{}'...", expected);

        loop {
            line.clear();
            tracing::debug!("Reading line from engine...");
            let n = stdout.read_line(&mut line)?;
            tracing::debug!("Read {} bytes", n);
            if n == 0 {
                anyhow::bail!("Engine closed stdout unexpectedly (waiting for '{}')", expected);
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                tracing::info!("Engine output: {}", trimmed);
            }

            if trimmed.starts_with(expected) {
                tracing::info!("Got expected response: {}", expected);
                return Ok(());
            }
        }
    }

    fn read_until_bestmove(&mut self) -> Result<()> {
        let stdout = self.stdout.as_mut().context("No stdout available")?;
        let mut line = String::new();

        loop {
            line.clear();
            let n = stdout.read_line(&mut line)?;
            if n == 0 {
                anyhow::bail!("Engine closed stdout unexpectedly");
            }
            let trimmed = line.trim();
            tracing::debug!("Engine: {}", trimmed);

            if trimmed.starts_with("info ") {
                if let Some(event) = Self::parse_info_line(trimmed) {
                    let _ = self.event_tx.send(event);
                }
            } else if trimmed.starts_with("bestmove ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let best_move = parts.get(1).unwrap_or(&"").to_string();
                let ponder = if parts.len() >= 4 && parts[2] == "ponder" {
                    Some(parts[3].to_string())
                } else {
                    None
                };
                let _ = self.event_tx.send(EngineEvent::BestMove { best_move, ponder });
                return Ok(());
            }
        }
    }

    fn parse_info_line(line: &str) -> Option<EngineEvent> {
        let parts: Vec<&str> = line.split_whitespace().collect();

        let mut depth = None;
        let mut score_cp = None;
        let mut score_mate = None;
        let mut pv = Vec::new();
        let mut nodes = None;
        let mut time_ms = None;

        let mut i = 1;
        while i < parts.len() {
            match parts[i] {
                "depth" => {
                    if i + 1 < parts.len() {
                        depth = parts[i + 1].parse().ok();
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "score" => {
                    if i + 2 < parts.len() {
                        match parts[i + 1] {
                            "cp" => score_cp = parts[i + 2].parse().ok(),
                            "mate" => score_mate = parts[i + 2].parse().ok(),
                            _ => {}
                        }
                        i += 3;
                    } else {
                        i += 1;
                    }
                }
                "nodes" => {
                    if i + 1 < parts.len() {
                        nodes = parts[i + 1].parse().ok();
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "time" => {
                    if i + 1 < parts.len() {
                        time_ms = parts[i + 1].parse().ok();
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "pv" => {
                    i += 1;
                    while i < parts.len() && !["depth", "score", "nodes", "time", "nps", "multipv", "seldepth", "hashfull", "tbhits", "string", "currmove", "currmovenumber"].contains(&parts[i]) {
                        pv.push(parts[i].to_string());
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }

        if depth.is_some() || score_cp.is_some() || score_mate.is_some() || !pv.is_empty() {
            Some(EngineEvent::Info {
                depth,
                score_cp,
                score_mate,
                pv,
                nodes,
                time_ms,
            })
        } else {
            None
        }
    }
}
