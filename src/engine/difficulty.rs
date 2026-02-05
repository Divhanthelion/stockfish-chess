use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DifficultyLevel {
    Novice,
    Beginner,
    Casual,
    Intermediate,
    Advanced,
    Expert,
    Maximum,
}

impl DifficultyLevel {
    pub fn all() -> &'static [DifficultyLevel] {
        &[
            DifficultyLevel::Novice,
            DifficultyLevel::Beginner,
            DifficultyLevel::Casual,
            DifficultyLevel::Intermediate,
            DifficultyLevel::Advanced,
            DifficultyLevel::Expert,
            DifficultyLevel::Maximum,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            DifficultyLevel::Novice => "Novice (~1100)",
            DifficultyLevel::Beginner => "Beginner (~1350)",
            DifficultyLevel::Casual => "Casual (~1500)",
            DifficultyLevel::Intermediate => "Intermediate (~1800)",
            DifficultyLevel::Advanced => "Advanced (~2100)",
            DifficultyLevel::Expert => "Expert (~2500)",
            DifficultyLevel::Maximum => "Maximum Strength",
        }
    }

    /// Returns the UCI commands needed to configure Stockfish for this difficulty
    pub fn uci_commands(&self) -> Vec<String> {
        match self {
            DifficultyLevel::Novice => {
                // UCI_Elo minimum is 1320, so we use Skill Level for very weak play
                vec![
                    "setoption name UCI_LimitStrength value false".to_string(),
                    "setoption name Skill Level value 0".to_string(),
                ]
            }
            DifficultyLevel::Beginner => vec![
                "setoption name UCI_LimitStrength value true".to_string(),
                "setoption name UCI_Elo value 1350".to_string(),
            ],
            DifficultyLevel::Casual => vec![
                "setoption name UCI_LimitStrength value true".to_string(),
                "setoption name UCI_Elo value 1500".to_string(),
            ],
            DifficultyLevel::Intermediate => vec![
                "setoption name UCI_LimitStrength value true".to_string(),
                "setoption name UCI_Elo value 1800".to_string(),
            ],
            DifficultyLevel::Advanced => vec![
                "setoption name UCI_LimitStrength value true".to_string(),
                "setoption name UCI_Elo value 2100".to_string(),
            ],
            DifficultyLevel::Expert => vec![
                "setoption name UCI_LimitStrength value true".to_string(),
                "setoption name UCI_Elo value 2500".to_string(),
            ],
            DifficultyLevel::Maximum => vec![
                "setoption name UCI_LimitStrength value false".to_string(),
            ],
        }
    }

    pub fn approximate_elo(&self) -> u32 {
        match self {
            DifficultyLevel::Novice => 1100,
            DifficultyLevel::Beginner => 1350,
            DifficultyLevel::Casual => 1500,
            DifficultyLevel::Intermediate => 1800,
            DifficultyLevel::Advanced => 2100,
            DifficultyLevel::Expert => 2500,
            DifficultyLevel::Maximum => 3500,
        }
    }
}

impl Default for DifficultyLevel {
    fn default() -> Self {
        DifficultyLevel::Casual
    }
}

impl std::fmt::Display for DifficultyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}
