use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DifficultyLevel {
    Novice,
    Beginner,
    #[default]
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

    /// Returns the UCI commands needed to configure Stockfish for this difficulty.
    ///
    /// Every level sets `Skill Level` explicitly because engine options persist:
    /// Novice lowers it, which would otherwise keep crippling a stronger level
    /// chosen later.
    pub fn uci_commands(&self) -> Vec<String> {
        let (limit_strength, elo, skill_level) = match self {
            // UCI_Elo minimum is 1320, so we use Skill Level for very weak play
            DifficultyLevel::Novice => (false, None, 0),
            DifficultyLevel::Beginner => (true, Some(1350), 20),
            DifficultyLevel::Casual => (true, Some(1500), 20),
            DifficultyLevel::Intermediate => (true, Some(1800), 20),
            DifficultyLevel::Advanced => (true, Some(2100), 20),
            DifficultyLevel::Expert => (true, Some(2500), 20),
            DifficultyLevel::Maximum => (false, None, 20),
        };

        let mut commands = vec![
            format!("setoption name UCI_LimitStrength value {limit_strength}"),
            format!("setoption name Skill Level value {skill_level}"),
        ];
        if let Some(elo) = elo {
            commands.push(format!("setoption name UCI_Elo value {elo}"));
        }
        commands
    }
}

impl std::fmt::Display for DifficultyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stronger_levels_restore_full_skill_after_novice() {
        for level in DifficultyLevel::all() {
            let commands = level.uci_commands();
            let expected_skill = if *level == DifficultyLevel::Novice {
                "setoption name Skill Level value 0"
            } else {
                "setoption name Skill Level value 20"
            };
            assert!(
                commands.iter().any(|command| command == expected_skill),
                "{level:?} should send `{expected_skill}`, got {commands:?}"
            );
        }
    }

    #[test]
    fn maximum_strength_is_unlimited() {
        let commands = DifficultyLevel::Maximum.uci_commands();
        assert!(commands.contains(&"setoption name UCI_LimitStrength value false".to_string()));
        assert!(!commands.iter().any(|command| command.contains("UCI_Elo")));
    }
}
