use crate::engine::difficulty::DifficultyLevel;
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const ENGINE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const ENGINE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const ENGINE_EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const ENGINE_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const ENGINE_SEARCH_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum EngineCommand {
    Init,
    SetDifficulty(DifficultyLevel),
    SetMultiPV(u32),
    NewGame,
    Go {
        request_id: u64,
        fen: String,
        moves: Vec<String>,
        movetime_ms: Option<u64>,
    },
    /// Start infinite analysis.
    Analyze {
        request_id: u64,
        fen: String,
        moves: Vec<String>,
    },
    Stop,
    Quit,
}

impl EngineCommand {
    fn is_search(&self) -> bool {
        matches!(
            self,
            EngineCommand::Go { .. } | EngineCommand::Analyze { .. }
        )
    }
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Ready,
    BestMove {
        request_id: u64,
        best_move: String,
        ponder: Option<String>,
    },
    Info {
        request_id: u64,
        depth: Option<u32>,
        score_cp: Option<i32>,
        score_mate: Option<i32>,
        pv: Vec<String>,
        nodes: Option<u64>,
        time_ms: Option<u64>,
        multipv: Option<u32>,
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
    Analyzing,
    Stopping,
    Terminated,
}

#[derive(Debug, Clone, Copy)]
struct ActiveSearch {
    request_id: u64,
    deadline: Option<Instant>,
}

enum OutputEvent {
    Line(String),
    Error(String),
    Eof,
}

pub struct EngineActor {
    cmd_rx: mpsc::Receiver<EngineCommand>,
    event_tx: mpsc::Sender<EngineEvent>,
    state: EngineState,
    stdin: Option<BufWriter<ChildStdin>>,
    output_rx: Option<mpsc::Receiver<OutputEvent>>,
    child: Option<Child>,
    difficulty: DifficultyLevel,
    active_search: Option<ActiveSearch>,
    pending_commands: VecDeque<EngineCommand>,
    pending_difficulty: Option<DifficultyLevel>,
}

impl EngineActor {
    pub fn spawn(
        stockfish_path: PathBuf,
    ) -> (mpsc::Sender<EngineCommand>, mpsc::Receiver<EngineEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        tracing::info!("EngineActor spawn with path: {}", stockfish_path.display());

        thread::spawn(move || {
            let mut actor = EngineActor {
                cmd_rx,
                event_tx,
                state: EngineState::Uninitialized,
                stdin: None,
                output_rx: None,
                child: None,
                difficulty: DifficultyLevel::default(),
                active_search: None,
                pending_commands: VecDeque::new(),
                pending_difficulty: None,
            };
            actor.run(stockfish_path);
        });

        (cmd_tx, event_rx)
    }

    fn run(&mut self, stockfish_path: PathBuf) {
        tracing::info!(
            "EngineActor run loop started for: {}",
            stockfish_path.display()
        );

        while self.state != EngineState::Terminated {
            if let Err(error) = self.drain_output_events() {
                self.report_error(error);
                let _ = self.quit();
                break;
            }

            if let Err(error) = self.check_search_deadline() {
                self.report_error(error);
                let _ = self.quit();
                break;
            }

            if self.active_search.is_none() {
                if let Some(difficulty) = self.pending_difficulty.take() {
                    self.difficulty = difficulty;
                    if let Err(error) = self.apply_difficulty() {
                        self.report_error(error);
                    }
                    continue;
                }

                if let Some(command) = self.pending_commands.pop_front() {
                    if let Err(error) = self.handle_command(command, stockfish_path.as_path()) {
                        self.report_error(error);
                    }
                    continue;
                }
            }

            match self.cmd_rx.recv_timeout(ENGINE_POLL_INTERVAL) {
                Ok(command) => {
                    if let Err(error) = self.handle_command(command, stockfish_path.as_path()) {
                        self.report_error(error);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::info!("Command channel closed, shutting down engine");
                    let _ = self.quit();
                }
            }
        }

        self.state = EngineState::Terminated;
        let _ = self.event_tx.send(EngineEvent::Terminated);
    }

    fn report_error(&self, error: anyhow::Error) {
        tracing::error!("Engine error: {error:#}");
        let _ = self.event_tx.send(EngineEvent::Error(format!("{error:#}")));
    }

    fn handle_command(&mut self, command: EngineCommand, stockfish_path: &Path) -> Result<()> {
        match command {
            EngineCommand::Quit => {
                self.pending_commands.clear();
                self.pending_difficulty = None;
                self.quit()
            }
            EngineCommand::Stop => {
                self.pending_commands.clear();
                self.request_stop()
            }
            EngineCommand::SetDifficulty(level) if self.active_search.is_some() => {
                self.difficulty = level;
                self.pending_difficulty = Some(level);
                Ok(())
            }
            command if self.active_search.is_some() => {
                if command.is_search() {
                    self.pending_commands.retain(|pending| !pending.is_search());
                }
                self.pending_commands.push_back(command);
                self.request_stop()
            }
            command => self.execute_idle_command(command, stockfish_path),
        }
    }

    fn execute_idle_command(
        &mut self,
        command: EngineCommand,
        stockfish_path: &Path,
    ) -> Result<()> {
        match command {
            EngineCommand::Init => self.init(stockfish_path),
            EngineCommand::SetDifficulty(level) => {
                self.difficulty = level;
                self.apply_difficulty()
            }
            EngineCommand::SetMultiPV(lines) => self.set_multipv(lines),
            EngineCommand::NewGame => self.new_game(),
            EngineCommand::Go {
                request_id,
                fen,
                moves,
                movetime_ms,
            } => self.go(request_id, &fen, &moves, movetime_ms),
            EngineCommand::Analyze {
                request_id,
                fen,
                moves,
            } => self.analyze(request_id, &fen, &moves),
            EngineCommand::Stop => Ok(()),
            EngineCommand::Quit => self.quit(),
        }
    }

    fn init(&mut self, stockfish_path: &Path) -> Result<()> {
        tracing::info!("Initializing Stockfish at: {}", stockfish_path.display());

        let mut command = Command::new(stockfish_path);
        if let Some(stockfish_dir) = stockfish_path
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
        {
            command.current_dir(stockfish_dir);
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to start Stockfish at '{}'. Check the path and executable permissions",
                    stockfish_path.display()
                )
            })?;

        let stdin = child
            .stdin
            .take()
            .context("Stockfish did not expose stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Stockfish did not expose stdout")?;
        let (output_tx, output_rx) = mpsc::channel();

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = output_tx.send(OutputEvent::Eof);
                        break;
                    }
                    Ok(_) => {
                        if output_tx
                            .send(OutputEvent::Line(line.trim().to_string()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = output_tx.send(OutputEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
        });

        self.stdin = Some(BufWriter::new(stdin));
        self.output_rx = Some(output_rx);
        self.child = Some(child);
        self.state = EngineState::Initializing;

        self.send_command("uci")?;
        self.wait_for_response("uciok")?;
        self.send_command("isready")?;
        self.wait_for_response("readyok")?;

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

        for command in self.difficulty.uci_commands() {
            self.send_command(&command)?;
        }
        self.send_command("isready")?;
        self.wait_for_response("readyok")
    }

    fn set_multipv(&mut self, lines: u32) -> Result<()> {
        if self.stdin.is_none() {
            return Ok(());
        }

        self.send_command(&format!(
            "setoption name MultiPV value {}",
            lines.clamp(1, 5)
        ))?;
        self.send_command("isready")?;
        self.wait_for_response("readyok")
    }

    fn new_game(&mut self) -> Result<()> {
        self.send_command("ucinewgame")?;
        self.send_command("isready")?;
        self.wait_for_response("readyok")
    }

    fn go(
        &mut self,
        request_id: u64,
        fen: &str,
        moves: &[String],
        movetime_ms: Option<u64>,
    ) -> Result<()> {
        self.send_command(&Self::position_command(fen, moves))?;
        let movetime_ms = movetime_ms.unwrap_or(1_000);
        self.send_command(&format!("go movetime {movetime_ms}"))?;
        self.active_search = Some(ActiveSearch {
            request_id,
            deadline: Some(
                Instant::now() + Duration::from_millis(movetime_ms) + ENGINE_SEARCH_GRACE,
            ),
        });
        self.state = EngineState::Thinking;
        Ok(())
    }

    fn analyze(&mut self, request_id: u64, fen: &str, moves: &[String]) -> Result<()> {
        self.send_command(&Self::position_command(fen, moves))?;
        self.send_command("go infinite")?;
        self.active_search = Some(ActiveSearch {
            request_id,
            deadline: None,
        });
        self.state = EngineState::Analyzing;
        Ok(())
    }

    fn request_stop(&mut self) -> Result<()> {
        if self.active_search.is_some() && self.state != EngineState::Stopping {
            self.send_command("stop")?;
            if let Some(active_search) = &mut self.active_search {
                active_search.deadline = Some(Instant::now() + ENGINE_STOP_TIMEOUT);
            }
            self.state = EngineState::Stopping;
        }
        Ok(())
    }

    fn check_search_deadline(&self) -> Result<()> {
        if let Some(active_search) = self.active_search {
            if active_search
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                anyhow::bail!(
                    "Stockfish timed out while completing request {}",
                    active_search.request_id
                );
            }
        }
        Ok(())
    }

    fn quit(&mut self) -> Result<()> {
        self.pending_commands.clear();
        self.pending_difficulty = None;
        self.active_search = None;

        if self.stdin.is_some() {
            let _ = self.send_command("quit");
        }

        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + ENGINE_EXIT_TIMEOUT;
            loop {
                if child.try_wait()?.is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    child.kill()?;
                    let _ = child.wait();
                    break;
                }
                thread::sleep(ENGINE_POLL_INTERVAL);
            }
        }

        self.stdin = None;
        self.output_rx = None;
        self.state = EngineState::Terminated;
        Ok(())
    }

    fn send_command(&mut self, command: &str) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("Stockfish stdin is unavailable")?;
        tracing::debug!("Sending to engine: {command}");
        writeln!(stdin, "{command}")?;
        stdin.flush()?;
        Ok(())
    }

    fn wait_for_response(&mut self, expected: &str) -> Result<()> {
        let deadline = Instant::now() + ENGINE_RESPONSE_TIMEOUT;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("Timed out waiting for Stockfish response '{expected}'");
            }

            let event = self
                .output_rx
                .as_ref()
                .context("Stockfish stdout is unavailable")?
                .recv_timeout(remaining);

            match event {
                Ok(OutputEvent::Line(line)) => {
                    if !line.is_empty() {
                        tracing::debug!("Engine: {line}");
                    }
                    if line.starts_with(expected) {
                        return Ok(());
                    }
                }
                Ok(OutputEvent::Error(error)) => {
                    anyhow::bail!("Failed reading Stockfish output: {error}");
                }
                Ok(OutputEvent::Eof) => {
                    anyhow::bail!("Stockfish closed stdout while waiting for '{expected}'");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    anyhow::bail!("Timed out waiting for Stockfish response '{expected}'");
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("Stockfish output reader stopped unexpectedly");
                }
            }
        }
    }

    fn drain_output_events(&mut self) -> Result<()> {
        // Bound each drain so a very chatty engine cannot starve commands.
        for _ in 0..256 {
            let event = match self.output_rx.as_ref() {
                Some(output_rx) => output_rx.try_recv(),
                None => return Ok(()),
            };

            match event {
                Ok(OutputEvent::Line(line)) => self.handle_output_line(&line),
                Ok(OutputEvent::Error(error)) => {
                    anyhow::bail!("Failed reading Stockfish output: {error}");
                }
                Ok(OutputEvent::Eof) => {
                    anyhow::bail!("Stockfish closed stdout unexpectedly");
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    anyhow::bail!("Stockfish output reader stopped unexpectedly");
                }
            }
        }

        Ok(())
    }

    fn handle_output_line(&mut self, line: &str) {
        tracing::debug!("Engine: {line}");

        if line.starts_with("info ") {
            if let Some(active_search) = self.active_search {
                if let Some(event) = Self::parse_info_line(active_search.request_id, line) {
                    let _ = self.event_tx.send(event);
                }
            }
            return;
        }

        if line.starts_with("bestmove ") {
            let Some(active_search) = self.active_search.take() else {
                tracing::debug!("Ignoring bestmove without an active search");
                return;
            };

            let parts = line.split_whitespace().collect::<Vec<_>>();
            let best_move = parts.get(1).copied().unwrap_or("(none)").to_string();
            let ponder = (parts.len() >= 4 && parts[2] == "ponder").then(|| parts[3].to_string());
            self.state = EngineState::Idle;
            let _ = self.event_tx.send(EngineEvent::BestMove {
                request_id: active_search.request_id,
                best_move,
                ponder,
            });
        }
    }

    fn position_command(fen: &str, moves: &[String]) -> String {
        if moves.is_empty() {
            format!("position fen {fen}")
        } else {
            format!("position fen {fen} moves {}", moves.join(" "))
        }
    }

    fn parse_info_line(request_id: u64, line: &str) -> Option<EngineEvent> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        let mut depth = None;
        let mut score_cp = None;
        let mut score_mate = None;
        let mut pv = Vec::new();
        let mut nodes = None;
        let mut time_ms = None;
        let mut multipv = None;
        let mut index = 1;

        while index < parts.len() {
            match parts[index] {
                "depth" => {
                    depth = parts.get(index + 1).and_then(|value| value.parse().ok());
                    index += 2;
                }
                "multipv" => {
                    multipv = parts.get(index + 1).and_then(|value| value.parse().ok());
                    index += 2;
                }
                "score" => {
                    if let (Some(score_type), Some(value)) =
                        (parts.get(index + 1), parts.get(index + 2))
                    {
                        match *score_type {
                            "cp" => score_cp = value.parse().ok(),
                            "mate" => score_mate = value.parse().ok(),
                            _ => {}
                        }
                    }
                    index += 3;
                }
                "nodes" => {
                    nodes = parts.get(index + 1).and_then(|value| value.parse().ok());
                    index += 2;
                }
                "time" => {
                    time_ms = parts.get(index + 1).and_then(|value| value.parse().ok());
                    index += 2;
                }
                "pv" => {
                    index += 1;
                    while index < parts.len()
                        && ![
                            "depth",
                            "score",
                            "nodes",
                            "time",
                            "nps",
                            "multipv",
                            "seldepth",
                            "hashfull",
                            "tbhits",
                            "string",
                            "currmove",
                            "currmovenumber",
                        ]
                        .contains(&parts[index])
                    {
                        pv.push(parts[index].to_string());
                        index += 1;
                    }
                }
                _ => index += 1,
            }
        }

        (depth.is_some() || score_cp.is_some() || score_mate.is_some() || !pv.is_empty()).then_some(
            EngineEvent::Info {
                request_id,
                depth,
                score_cp,
                score_mate,
                pv,
                nodes,
                time_ms,
                multipv,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn builds_position_command_without_moves() {
        assert_eq!(
            EngineActor::position_command("test fen", &[]),
            "position fen test fen"
        );
    }

    #[test]
    fn builds_position_command_with_move_history() {
        assert_eq!(
            EngineActor::position_command("test fen", &["e2e4".to_string(), "e7e5".to_string()]),
            "position fen test fen moves e2e4 e7e5"
        );
    }

    #[test]
    fn parses_stockfish_info_line() {
        let event = EngineActor::parse_info_line(
            42,
            "info depth 18 multipv 2 score cp -37 nodes 12345 time 50 pv e2e4 e7e5",
        )
        .unwrap();

        match event {
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
                assert_eq!(request_id, 42);
                assert_eq!(depth, Some(18));
                assert_eq!(score_cp, Some(-37));
                assert_eq!(score_mate, None);
                assert_eq!(pv, ["e2e4", "e7e5"]);
                assert_eq!(nodes, Some(12_345));
                assert_eq!(time_ms, Some(50));
                assert_eq!(multipv, Some(2));
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[cfg(unix)]
    fn fake_engine(script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "stockfish-chess-actor-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("stockfish");
        fs::write(&path, script).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn stop_interrupts_a_silent_analysis() {
        let path = fake_engine(
            r#"#!/bin/sh
while IFS= read -r line; do
    case "$line" in
        "uci") echo "uciok" ;;
        "isready") echo "readyok" ;;
        "stop") echo "bestmove e2e4" ;;
        "quit") exit 0 ;;
    esac
done
"#,
        );
        let directory = path.parent().unwrap().to_path_buf();
        let (command_tx, event_rx) = EngineActor::spawn(path);

        command_tx.send(EngineCommand::Init).unwrap();
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            EngineEvent::Ready
        ));

        command_tx
            .send(EngineCommand::Analyze {
                request_id: 7,
                fen: "test fen".to_string(),
                moves: Vec::new(),
            })
            .unwrap();
        command_tx.send(EngineCommand::Stop).unwrap();

        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            EngineEvent::BestMove { request_id: 7, .. }
        ));

        command_tx.send(EngineCommand::Quit).unwrap();
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            EngineEvent::Terminated
        ));
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stopped_search_has_a_deadline() {
        let path = fake_engine(
            r#"#!/bin/sh
while IFS= read -r line; do
    case "$line" in
        "uci") echo "uciok" ;;
        "isready") echo "readyok" ;;
        "quit") exit 0 ;;
    esac
done
"#,
        );
        let directory = path.parent().unwrap().to_path_buf();
        let (command_tx, event_rx) = EngineActor::spawn(path);

        command_tx.send(EngineCommand::Init).unwrap();
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            EngineEvent::Ready
        ));

        command_tx
            .send(EngineCommand::Analyze {
                request_id: 9,
                fen: "test fen".to_string(),
                moves: Vec::new(),
            })
            .unwrap();
        command_tx.send(EngineCommand::Stop).unwrap();

        match event_rx.recv_timeout(Duration::from_secs(4)).unwrap() {
            EngineEvent::Error(error) => assert!(error.contains("timed out")),
            event => panic!("unexpected event: {event:?}"),
        }
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            EngineEvent::Terminated
        ));

        drop(command_tx);
        fs::remove_dir_all(directory).unwrap();
    }
}
