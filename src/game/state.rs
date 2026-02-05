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
    #[error("No previous position")]
    NoPreviousPosition,
    #[error("No next position")]
    NoNextPosition,
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

/// Represents a position in the game history
#[derive(Debug, Clone)]
struct PositionState {
    position: Chess,
    hash: u64,
}

pub struct GameState {
    /// All positions in the game, index 0 is starting position
    positions: Vec<PositionState>,
    /// All moves made (san, uci, and resulting FEN)
    move_history: Vec<MoveRecord>,
    /// Current position index we're viewing (may be less than positions.len() - 1)
    current_index: usize,
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
            positions: vec![PositionState { position, hash }],
            move_history: Vec::new(),
            current_index: 0,
        }
    }

    pub fn from_fen(fen: &str) -> Result<Self, GameError> {
        let fen: Fen = fen.parse().map_err(|e| GameError::InvalidFen(format!("{:?}", e)))?;
        let position: Chess = fen
            .into_position(CastlingMode::Standard)
            .map_err(|e| GameError::InvalidFen(format!("{:?}", e)))?;
        let hash = Self::compute_hash(&position);
        Ok(Self {
            positions: vec![PositionState { position, hash }],
            move_history: Vec::new(),
            current_index: 0,
        })
    }

    fn compute_hash(position: &Chess) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        position.board().hash(&mut hasher);
        position.turn().hash(&mut hasher);
        position.castles().has(Color::White, shakmaty::CastlingSide::KingSide).hash(&mut hasher);
        position.castles().has(Color::White, shakmaty::CastlingSide::QueenSide).hash(&mut hasher);
        position.castles().has(Color::Black, shakmaty::CastlingSide::KingSide).hash(&mut hasher);
        position.castles().has(Color::Black, shakmaty::CastlingSide::QueenSide).hash(&mut hasher);
        position.ep_square(EnPassantMode::Legal).hash(&mut hasher);
        hasher.finish()
    }

    /// Get current position (the one we're viewing)
    fn current_position(&self) -> &Chess {
        &self.positions[self.current_index].position
    }

    pub fn fen(&self) -> String {
        Fen::from_position(self.current_position(), EnPassantMode::Legal).to_string()
    }

    pub fn turn(&self) -> PlayerColor {
        self.current_position().turn().into()
    }

    pub fn is_check(&self) -> bool {
        self.current_position().is_check()
    }

    pub fn outcome(&self) -> GameOutcome {
        let pos = self.current_position();
        
        if pos.is_checkmate() {
            let winner = match pos.turn() {
                Color::White => PlayerColor::Black,
                Color::Black => PlayerColor::White,
            };
            return GameOutcome::Checkmate(winner);
        }

        if pos.is_stalemate() {
            return GameOutcome::Stalemate;
        }

        if pos.is_insufficient_material() {
            return GameOutcome::InsufficientMaterial;
        }

        // Check for threefold repetition using all positions up to current
        let current_hash = self.positions[self.current_index].hash;
        let repetitions = self.positions[..=self.current_index]
            .iter()
            .filter(|p| p.hash == current_hash)
            .count();
        if repetitions >= 3 {
            return GameOutcome::ThreefoldRepetition;
        }

        if pos.halfmoves() >= 100 {
            return GameOutcome::FiftyMoveRule;
        }

        GameOutcome::InProgress
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        self.current_position().legal_moves().into_iter().collect()
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
            .to_move(self.current_position())
            .map_err(|_| GameError::InvalidMove(san_str.to_string()))?;

        self.make_move(m)
    }

    pub fn make_move_uci(&mut self, uci_str: &str) -> Result<MoveRecord, GameError> {
        if self.outcome() != GameOutcome::InProgress {
            return Err(GameError::GameOver);
        }

        let uci: UciMove = uci_str
            .parse()
            .map_err(|_| GameError::InvalidMove(uci_str.to_string()))?;

        let m = uci
            .to_move(self.current_position())
            .map_err(|_| GameError::InvalidMove(uci_str.to_string()))?;

        self.make_move(m)
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
        let san = San::from_move(self.current_position(), m.clone());
        let uci = UciMove::from_move(m.clone(), CastlingMode::Standard);

        // Play the move on current position
        let new_position = self.current_position().clone().play(m).map_err(|e| {
            GameError::InvalidMove(format!("{:?}", e))
        })?;

        let resulting_fen = Fen::from_position(&new_position, EnPassantMode::Legal).to_string();
        let hash = Self::compute_hash(&new_position);

        // If we're not at the end, truncate the future
        if self.current_index < self.positions.len() - 1 {
            self.positions.truncate(self.current_index + 1);
            self.move_history.truncate(self.current_index);
        }

        // Add new position and move
        self.positions.push(PositionState { position: new_position, hash });
        self.current_index += 1;

        let record = MoveRecord {
            san: san.to_string(),
            uci: uci.to_string(),
            resulting_fen,
        };
        self.move_history.push(record.clone());

        Ok(record)
    }

    /// Go to previous position (undo) - returns true if successful
    pub fn go_back(&mut self) -> Result<(), GameError> {
        if self.current_index == 0 {
            return Err(GameError::NoPreviousPosition);
        }
        self.current_index -= 1;
        Ok(())
    }

    /// Go to next position (redo) - returns true if successful
    pub fn go_forward(&mut self) -> Result<(), GameError> {
        if self.current_index >= self.positions.len() - 1 {
            return Err(GameError::NoNextPosition);
        }
        self.current_index += 1;
        Ok(())
    }

    /// Go to a specific move number (0 = start position)
    pub fn go_to_position(&mut self, index: usize) -> Result<(), GameError> {
        if index >= self.positions.len() {
            return Err(GameError::InvalidMove("Position index out of range".to_string()));
        }
        self.current_index = index;
        Ok(())
    }

    /// Go to start position
    pub fn go_to_start(&mut self) {
        self.current_index = 0;
    }

    /// Go to end (latest position)
    pub fn go_to_end(&mut self) {
        self.current_index = self.positions.len() - 1;
    }

    /// Check if we can go back
    pub fn can_go_back(&self) -> bool {
        self.current_index > 0
    }

    /// Check if we can go forward
    pub fn can_go_forward(&self) -> bool {
        self.current_index < self.positions.len() - 1
    }

    /// Get current position index
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Get total number of positions
    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    pub fn move_history(&self) -> &[MoveRecord] {
        &self.move_history
    }

    pub fn piece_at(&self, square: Square) -> Option<(Role, Color)> {
        let piece = self.current_position().board().piece_at(square)?;
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
        if self.current_index == 0 || self.current_index > self.move_history.len() {
            return None;
        }
        self.move_history.get(self.current_index - 1)
    }

    pub fn last_move_squares(&self) -> Option<(Square, Square)> {
        self.last_move().and_then(|record| {
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
        self.current_position().board().king_of(c)
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
    fn test_navigation() {
        let mut game = GameState::new();
        
        // Make some moves
        game.make_move_san("e4").unwrap();
        game.make_move_san("e5").unwrap();
        game.make_move_san("Nf3").unwrap();
        
        assert_eq!(game.current_index(), 3);
        
        // Go back
        game.go_back().unwrap();
        assert_eq!(game.current_index(), 2);
        
        // Go forward
        game.go_forward().unwrap();
        assert_eq!(game.current_index(), 3);
        
        // Go to start
        game.go_to_start();
        assert_eq!(game.current_index(), 0);
        
        // Go to end
        game.go_to_end();
        assert_eq!(game.current_index(), 3);
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
