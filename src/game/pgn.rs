#![allow(dead_code)]

use crate::game::GameState;
use shakmaty::san::San;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum PgnError {
    #[error("Invalid move in PGN: {0}")]
    InvalidMove(String),
    #[error("Invalid FEN in PGN: {0}")]
    InvalidFen(String),
}

#[derive(Debug, Clone, Default)]
pub struct PgnHeaders {
    pub event: Option<String>,
    pub site: Option<String>,
    pub date: Option<String>,
    pub round: Option<String>,
    pub white: Option<String>,
    pub black: Option<String>,
    pub result: Option<String>,
    pub fen: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedPgn {
    pub headers: PgnHeaders,
    pub moves: Vec<String>,
}

fn parse_header_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    let content = &trimmed[1..trimmed.len() - 1];
    let mut parts = content.splitn(2, ' ');
    let key = parts.next()?.trim().to_string();
    let value = parts.next()?.trim();

    if !value.starts_with('"') || !value.ends_with('"') {
        return None;
    }
    let value = value[1..value.len() - 1].to_string();

    Some((key, value))
}

fn strip_move_numbers(movetext: &str) -> Vec<String> {
    let mut moves = Vec::new();
    let tokens: Vec<&str> = movetext.split_whitespace().collect();

    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        if token == "*"
            || token == "1-0"
            || token == "0-1"
            || token == "1/2-1/2"
            || token.ends_with('.')
            || token.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            continue;
        }

        if let Some(san) = token.strip_suffix("+").or(Some(token)) {
            let san = san.strip_suffix("#").unwrap_or(san);
            if san.chars().next().is_some_and(|c| c.is_alphabetic()) {
                moves.push(token.to_string());
            }
        }
    }

    moves
}

pub fn parse_pgn(pgn: &str) -> Result<ParsedPgn, PgnError> {
    let mut headers = PgnHeaders::default();
    let mut movetext = String::new();
    let mut in_movetext = false;

    for line in pgn.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if headers.event.is_some()
                || headers.white.is_some()
                || headers.fen.is_some()
                || !movetext.is_empty()
            {
                in_movetext = true;
            }
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') && !in_movetext {
            if let Some((key, value)) = parse_header_line(trimmed) {
                match key.to_lowercase().as_str() {
                    "event" => headers.event = Some(value),
                    "site" => headers.site = Some(value),
                    "date" => headers.date = Some(value),
                    "round" => headers.round = Some(value),
                    "white" => headers.white = Some(value),
                    "black" => headers.black = Some(value),
                    "result" => headers.result = Some(value),
                    "fen" => headers.fen = Some(value),
                    _ => {}
                }
            }
        } else {
            in_movetext = true;
            if !movetext.is_empty() {
                movetext.push(' ');
            }
            movetext.push_str(trimmed);
        }
    }

    let moves = strip_move_numbers(&movetext);

    Ok(ParsedPgn { headers, moves })
}

pub fn import_pgn(pgn: &str) -> Result<(GameState, PgnHeaders), PgnError> {
    let parsed = parse_pgn(pgn)?;

    let mut game = if let Some(fen) = &parsed.headers.fen {
        GameState::from_fen(fen).map_err(|e| PgnError::InvalidFen(e.to_string()))?
    } else {
        GameState::new()
    };

    for san_str in &parsed.moves {
        let san: San = san_str
            .parse()
            .map_err(|_| PgnError::InvalidMove(san_str.clone()))?;

        let m = san
            .to_move(game.current_position())
            .map_err(|_| PgnError::InvalidMove(san_str.clone()))?;

        game.make_move(m)
            .map_err(|_| PgnError::InvalidMove(san_str.clone()))?;
    }

    Ok((game, parsed.headers))
}

pub fn export_pgn(game: &GameState, headers: &PgnHeaders) -> String {
    let mut pgn = String::new();

    pgn.push_str(&format!(
        "[Event \"{}\"]\n",
        headers.event.as_deref().unwrap_or("?")
    ));
    pgn.push_str(&format!(
        "[Site \"{}\"]\n",
        headers.site.as_deref().unwrap_or("?")
    ));
    pgn.push_str(&format!(
        "[Date \"{}\"]\n",
        headers.date.as_deref().unwrap_or("????.??.??")
    ));
    pgn.push_str(&format!(
        "[Round \"{}\"]\n",
        headers.round.as_deref().unwrap_or("-")
    ));
    pgn.push_str(&format!(
        "[White \"{}\"]\n",
        headers.white.as_deref().unwrap_or("?")
    ));
    pgn.push_str(&format!(
        "[Black \"{}\"]\n",
        headers.black.as_deref().unwrap_or("?")
    ));
    pgn.push_str(&format!(
        "[Result \"{}\"]\n",
        headers.result.as_deref().unwrap_or("*")
    ));

    if let Some(fen) = &headers.fen {
        if fen != "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" {
            pgn.push_str(&format!("[FEN \"{}\"]\n", fen));
            pgn.push_str("[SetUp \"1\"]\n");
        }
    }

    pgn.push('\n');

    let history = game.move_history();
    for (i, record) in history.iter().enumerate() {
        if i % 2 == 0 {
            pgn.push_str(&format!("{}. ", i / 2 + 1));
        }
        pgn.push_str(&record.san);
        pgn.push(' ');

        if (i + 1) % 10 == 0 {
            pgn.push('\n');
        }
    }

    let result = headers.result.as_deref().unwrap_or("*");
    if !pgn.ends_with('\n') && !history.is_empty() {
        pgn.push('\n');
    }
    pgn.push_str(result);
    pgn.push('\n');

    pgn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameOutcome;

    #[test]
    fn parses_simple_pgn() {
        let pgn = r#"[Event "Test Game"]
[White "Player"]
[Black "Stockfish"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 *"#;

        let parsed = parse_pgn(pgn).unwrap();
        assert_eq!(parsed.headers.event, Some("Test Game".to_string()));
        assert_eq!(parsed.headers.white, Some("Player".to_string()));
        assert_eq!(parsed.headers.black, Some("Stockfish".to_string()));
        assert_eq!(parsed.moves, vec!["e4", "e5", "Nf3", "Nc6", "Bb5"]);
    }

    #[test]
    fn parses_pgn_with_result() {
        let pgn = r#"[Result "1-0"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0"#;

        let parsed = parse_pgn(pgn).unwrap();
        assert_eq!(parsed.headers.result, Some("1-0".to_string()));
        assert_eq!(
            parsed.moves,
            vec!["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]
        );
    }

    #[test]
    fn imports_scholars_mate() {
        let pgn = r#"[Event "Scholar's Mate"]
[Result "1-0"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0"#;

        let (game, headers) = import_pgn(pgn).unwrap();
        assert_eq!(headers.event, Some("Scholar's Mate".to_string()));
        assert!(matches!(
            game.outcome(),
            GameOutcome::Checkmate(crate::game::PlayerColor::White)
        ));
        assert_eq!(game.move_history().len(), 7);
    }

    #[test]
    fn round_trip_preserves_moves() {
        let original_pgn = r#"[Event "Round Trip Test"]
[Site "Local"]
[Date "2024.01.01"]
[White "Player"]
[Black "Stockfish"]
[Result "*"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 *"#;

        let (game, headers) = import_pgn(original_pgn).unwrap();
        let exported = export_pgn(&game, &headers);

        let (reimported_game, _) = import_pgn(&exported).unwrap();

        let original_moves: Vec<_> = game.move_history().iter().map(|r| &r.san).collect();
        let reimported_moves: Vec<_> = reimported_game
            .move_history()
            .iter()
            .map(|r| &r.san)
            .collect();

        assert_eq!(original_moves, reimported_moves);
    }

    #[test]
    fn round_trip_with_promotion() {
        let mut game = GameState::from_fen("7k/P7/8/8/8/8/8/7K w - - 0 1").unwrap();
        game.make_move_uci("a7a8q").unwrap();

        let headers = PgnHeaders {
            fen: Some("7k/P7/8/8/8/8/8/7K w - - 0 1".to_string()),
            ..Default::default()
        };

        let pgn = export_pgn(&game, &headers);
        let (reimported, _) = import_pgn(&pgn).unwrap();

        assert_eq!(reimported.move_history().len(), 1);
        assert_eq!(reimported.move_history()[0].san, "a8=Q");
    }

    #[test]
    fn round_trip_with_checkmate() {
        let pgn = r#"1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0"#;

        let (game, headers) = import_pgn(pgn).unwrap();
        assert!(matches!(
            game.outcome(),
            GameOutcome::Checkmate(crate::game::PlayerColor::White)
        ));

        let exported = export_pgn(&game, &headers);
        let (reimported, _) = import_pgn(&exported).unwrap();

        assert!(matches!(
            reimported.outcome(),
            GameOutcome::Checkmate(crate::game::PlayerColor::White)
        ));
    }

    #[test]
    fn import_rejects_invalid_moves() {
        let pgn = r#"1. e4 e5 2. Qh5 Qh1"#;
        let result = import_pgn(pgn);
        assert!(matches!(result, Err(PgnError::InvalidMove(_))));
    }

    #[test]
    fn parses_pgn_with_custom_fen() {
        let pgn = r#"[FEN "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3"]

3. Bb5 a6 4. Ba4 *"#;

        let (game, _) = import_pgn(pgn).unwrap();
        assert_eq!(game.move_history().len(), 3);
        assert_eq!(game.move_history()[0].san, "Bb5");
    }

    #[test]
    fn handles_check_and_checkmate_annotations() {
        let pgn = r#"1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7+"#;
        let parsed = parse_pgn(pgn).unwrap();
        assert_eq!(parsed.moves.last(), Some(&"Qxf7+".to_string()));

        let pgn_mate = r#"1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7#"#;
        let parsed_mate = parse_pgn(pgn_mate).unwrap();
        assert_eq!(parsed_mate.moves.last(), Some(&"Qxf7#".to_string()));
    }

    #[test]
    fn exports_standard_headers() {
        let game = GameState::new();
        let headers = PgnHeaders {
            event: Some("Test Event".to_string()),
            white: Some("White Player".to_string()),
            black: Some("Black Player".to_string()),
            ..Default::default()
        };

        let pgn = export_pgn(&game, &headers);
        assert!(pgn.contains("[Event \"Test Event\"]"));
        assert!(pgn.contains("[White \"White Player\"]"));
        assert!(pgn.contains("[Black \"Black Player\"]"));
    }

    #[test]
    fn round_trip_underpromotion() {
        let mut game = GameState::from_fen("7k/P7/8/8/8/8/8/7K w - - 0 1").unwrap();
        game.make_move_uci("a7a8n").unwrap();

        let headers = PgnHeaders {
            fen: Some("7k/P7/8/8/8/8/8/7K w - - 0 1".to_string()),
            ..Default::default()
        };

        let pgn = export_pgn(&game, &headers);
        let (reimported, _) = import_pgn(&pgn).unwrap();

        assert_eq!(reimported.move_history().len(), 1);
        assert_eq!(reimported.move_history()[0].san, "a8=N");
    }
}
