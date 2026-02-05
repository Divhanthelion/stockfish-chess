use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Classic,
    Lichess,
    ChessCom,
    Dark,
}

impl Theme {
    pub fn all() -> &'static [Theme] {
        &[Theme::Classic, Theme::Lichess, Theme::ChessCom, Theme::Dark]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Theme::Classic => "Classic",
            Theme::Lichess => "Lichess",
            Theme::ChessCom => "Chess.com",
            Theme::Dark => "Dark",
        }
    }

    pub fn light_square(&self) -> Color32 {
        match self {
            Theme::Classic => Color32::from_rgb(240, 217, 181),
            Theme::Lichess => Color32::from_rgb(240, 217, 181),
            Theme::ChessCom => Color32::from_rgb(238, 238, 210),
            Theme::Dark => Color32::from_rgb(100, 100, 100),
        }
    }

    pub fn dark_square(&self) -> Color32 {
        match self {
            Theme::Classic => Color32::from_rgb(181, 136, 99),
            Theme::Lichess => Color32::from_rgb(181, 136, 99),
            Theme::ChessCom => Color32::from_rgb(118, 150, 86),
            Theme::Dark => Color32::from_rgb(60, 60, 60),
        }
    }

    pub fn selected_square(&self) -> Color32 {
        match self {
            Theme::Classic => Color32::from_rgb(186, 202, 68),
            Theme::Lichess => Color32::from_rgb(186, 202, 68),
            Theme::ChessCom => Color32::from_rgb(186, 202, 68),
            Theme::Dark => Color32::from_rgb(130, 151, 105),
        }
    }

    pub fn last_move_highlight(&self) -> Color32 {
        match self {
            Theme::Classic => Color32::from_rgb(205, 210, 106),
            Theme::Lichess => Color32::from_rgb(205, 210, 106),
            Theme::ChessCom => Color32::from_rgb(247, 247, 105),
            Theme::Dark => Color32::from_rgb(170, 162, 58),
        }
    }

    pub fn legal_move_dot(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 40)
    }

    pub fn check_highlight(&self) -> Color32 {
        Color32::from_rgb(255, 100, 100)
    }

    pub fn coordinate_color_light(&self) -> Color32 {
        self.dark_square()
    }

    pub fn coordinate_color_dark(&self) -> Color32 {
        self.light_square()
    }
}
