use shakmaty::{
    fen::Fen, san::San, uci::UciMove, CastlingMode, Chess, Color, EnPassantMode, Move,
    Position, Role, Square,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GameError {
    #[error("Invalid move: {0}")]
    InvalidMove(String),
    #[error("Invalid FEN: {0}")]
    InvalidFen(String),
    #[error("Game is already over")]
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlayerColor {
    White,
    Black,
}

impl From<Color> for PlayerColor {
    fn from(c: Color) -> Self {
        match c {
            Color::White => PlayerColor::White,
            Color::Black => PlayerColor::Black,
        }
    }
}

impl From<PlayerColor> for Color {
    fn from(c: PlayerColor) -> Self {
        match c {
            PlayerColor::White => Color::White,
            PlayerColor::Black => Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOutcome {
    Checkmate(PlayerColor), // Winner
    Stalemate,
    InsufficientMaterial,
    ThreefoldRepetition,
    FiftyMoveRule,
    InProgress,
}

#[derive(Debug, Clone)]
pub struct MoveRecord {
    pub san: String,
    pub uci: String,
    pub resulting_fen: String,
}

pub struct GameState {
    position: Chess,
    move_history: Vec<MoveRecord>,
    position_hashes: Vec<u64>,
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        let position = Chess::default();
        let hash = Self::compute_hash(&position);
        Self {
            position,
            move_history: Vec::new(),
            position_hashes: vec![hash],
        }
    }

    pub fn from_fen(fen: &str) -> Result<Self, GameError> {
        let fen: Fen = fen.parse().map_err(|e| GameError::InvalidFen(format!("{:?}", e)))?;
        let position: Chess = fen
            .into_position(CastlingMode::Standard)
            .map_err(|e| GameError::InvalidFen(format!("{:?}", e)))?;
        let hash = Self::compute_hash(&position);
        Ok(Self {
            position,
            move_history: Vec::new(),
            position_hashes: vec![hash],
        })
    }

    fn compute_hash(position: &Chess) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        position.board().hash(&mut hasher);
        position.turn().hash(&mut hasher);
        // Hash castling rights as individual booleans
        position.castles().has(Color::White, shakmaty::CastlingSide::KingSide).hash(&mut hasher);
        position.castles().has(Color::White, shakmaty::CastlingSide::QueenSide).hash(&mut hasher);
        position.castles().has(Color::Black, shakmaty::CastlingSide::KingSide).hash(&mut hasher);
        position.castles().has(Color::Black, shakmaty::CastlingSide::QueenSide).hash(&mut hasher);
        position.ep_square(EnPassantMode::Legal).hash(&mut hasher);
        hasher.finish()
    }

    pub fn fen(&self) -> String {
        Fen::from_position(&self.position, EnPassantMode::Legal).to_string()
    }

    pub fn turn(&self) -> PlayerColor {
        self.position.turn().into()
    }

    pub fn is_check(&self) -> bool {
        self.position.is_check()
    }

    pub fn outcome(&self) -> GameOutcome {
        if self.position.is_checkmate() {
            let winner = match self.position.turn() {
                Color::White => PlayerColor::Black,
                Color::Black => PlayerColor::White,
            };
            return GameOutcome::Checkmate(winner);
        }

        if self.position.is_stalemate() {
            return GameOutcome::Stalemate;
        }

        if self.position.is_insufficient_material() {
            return GameOutcome::InsufficientMaterial;
        }

        let current_hash = *self.position_hashes.last().unwrap();
        let repetitions = self
            .position_hashes
            .iter()
            .filter(|&&h| h == current_hash)
            .count();
        if repetitions >= 3 {
            return GameOutcome::ThreefoldRepetition;
        }

        if self.position.halfmoves() >= 100 {
            return GameOutcome::FiftyMoveRule;
        }

        GameOutcome::InProgress
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        self.position.legal_moves().into_iter().collect()
    }

    pub fn legal_moves_for_square(&self, square: Square) -> Vec<Move> {
        self.legal_moves()
            .into_iter()
            .filter(|m| m.from() == Some(square))
            .collect()
    }

    pub fn make_move_san(&mut self, san_str: &str) -> Result<MoveRecord, GameError> {
        if self.outcome() != GameOutcome::InProgress {
            return Err(GameError::GameOver);
        }

        let san: San = san_str
            .parse()
            .map_err(|_| GameError::InvalidMove(san_str.to_string()))?;

        let m = san
            .to_move(&self.position)
            .map_err(|_| GameError::InvalidMove(san_str.to_string()))?;

        self.apply_move(m)
    }

    pub fn make_move_uci(&mut self, uci_str: &str) -> Result<MoveRecord, GameError> {
        if self.outcome() != GameOutcome::InProgress {
            return Err(GameError::GameOver);
        }

        let uci: UciMove = uci_str
            .parse()
            .map_err(|_| GameError::InvalidMove(uci_str.to_string()))?;

        let m = uci
            .to_move(&self.position)
            .map_err(|_| GameError::InvalidMove(uci_str.to_string()))?;

        self.apply_move(m)
    }

    pub fn make_move(&mut self, m: Move) -> Result<MoveRecord, GameError> {
        if self.outcome() != GameOutcome::InProgress {
            return Err(GameError::GameOver);
        }

        if !self.legal_moves().contains(&m) {
            return Err(GameError::InvalidMove(format!("{:?}", m)));
        }

        self.apply_move(m)
    }

    fn apply_move(&mut self, m: Move) -> Result<MoveRecord, GameError> {
        let san = San::from_move(&self.position, m.clone());
        let uci = UciMove::from_move(m.clone(), CastlingMode::Standard);

        self.position = self.position.clone().play(m).map_err(|e| {
            GameError::InvalidMove(format!("{:?}", e))
        })?;

        let resulting_fen = self.fen();
        let hash = Self::compute_hash(&self.position);
        self.position_hashes.push(hash);

        let record = MoveRecord {
            san: san.to_string(),
            uci: uci.to_string(),
            resulting_fen,
        };
        self.move_history.push(record.clone());

        Ok(record)
    }

    pub fn move_history(&self) -> &[MoveRecord] {
        &self.move_history
    }

    pub fn piece_at(&self, square: Square) -> Option<(Role, Color)> {
        let piece = self.position.board().piece_at(square)?;
        Some((piece.role, piece.color))
    }

    pub fn all_pieces(&self) -> impl Iterator<Item = (Square, Role, Color)> + '_ {
        Square::ALL.iter().filter_map(|&sq| {
            self.piece_at(sq).map(|(role, color)| (sq, role, color))
        })
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn last_move(&self) -> Option<&MoveRecord> {
        self.move_history.last()
    }

    pub fn last_move_squares(&self) -> Option<(Square, Square)> {
        self.move_history.last().and_then(|record| {
            let uci: UciMove = record.uci.parse().ok()?;
            match uci {
                UciMove::Normal { from, to, .. } => Some((from, to)),
                UciMove::Put { .. } => None,
                UciMove::Null => None,
            }
        })
    }

    pub fn king_square(&self, color: PlayerColor) -> Option<Square> {
        let c: Color = color.into();
        self.position.board().king_of(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game() {
        let game = GameState::new();
        assert_eq!(game.turn(), PlayerColor::White);
        assert_eq!(game.outcome(), GameOutcome::InProgress);
        assert!(!game.is_check());
    }

    #[test]
    fn test_make_move() {
        let mut game = GameState::new();
        let result = game.make_move_san("e4");
        assert!(result.is_ok());
        assert_eq!(game.turn(), PlayerColor::Black);
    }

    #[test]
    fn test_scholars_mate() {
        let mut game = GameState::new();
        game.make_move_san("e4").unwrap();
        game.make_move_san("e5").unwrap();
        game.make_move_san("Qh5").unwrap();
        game.make_move_san("Nc6").unwrap();
        game.make_move_san("Bc4").unwrap();
        game.make_move_san("Nf6").unwrap();
        game.make_move_san("Qxf7").unwrap();

        assert_eq!(game.outcome(), GameOutcome::Checkmate(PlayerColor::White));
    }
}
