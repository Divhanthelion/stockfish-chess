use crate::game::{GameState};
use crate::ui::{PieceRenderer, Theme};
use egui::{
    pos2, vec2, Color32, Id, Rect, Response, Sense, Stroke, Ui,
};
use shakmaty::{File, Move, Rank, Square};

pub struct ChessBoard<'a> {
    game: &'a GameState,
    theme: Theme,
    flipped: bool,
    piece_renderer: &'a mut PieceRenderer,
}

pub struct BoardResponse {
    pub move_made: Option<Move>,
    pub square_clicked: Option<Square>,
}

impl<'a> ChessBoard<'a> {
    pub fn new(
        game: &'a GameState,
        theme: Theme,
        flipped: bool,
        piece_renderer: &'a mut PieceRenderer,
    ) -> Self {
        Self {
            game,
            theme,
            flipped,
            piece_renderer,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        selected_square: &mut Option<Square>,
        legal_moves_for_selected: &[Move],
    ) -> BoardResponse {
        let mut response = BoardResponse {
            move_made: None,
            square_clicked: None,
        };

        let available_size = ui.available_size();
        let board_size = available_size.x.min(available_size.y);
        let square_size = board_size / 8.0;

        // Use a scope to isolate board interactions
        ui.scope(|ui| {
            // Allocate the board area
            let board_rect = ui
                .allocate_rect(
                    egui::Rect::from_min_size(ui.cursor().min, vec2(board_size, board_size)),
                    Sense::hover(),
                )
                .rect;

        let last_move_squares = self.game.last_move_squares();

        let king_in_check = if self.game.is_check() {
            self.game.king_square(self.game.turn())
        } else {
            None
        };

        // Draw and handle interaction for each square
        for rank_idx in 0u8..8 {
            for file_idx in 0u8..8 {
                let (display_file, display_rank) = if self.flipped {
                    (7 - file_idx, rank_idx)
                } else {
                    (file_idx, 7 - rank_idx)
                };

                let file = File::new(file_idx as u32);
                let rank = Rank::new(rank_idx as u32);
                let square = Square::from_coords(file, rank);

                let rect = Rect::from_min_size(
                    board_rect.min + vec2(display_file as f32 * square_size, display_rank as f32 * square_size),
                    vec2(square_size, square_size),
                );

                // Determine square color
                let is_light = (file_idx + rank_idx) % 2 == 1;
                let is_selected = *selected_square == Some(square);
                let is_last_move = last_move_squares
                    .map(|(from, to)| square == from || square == to)
                    .unwrap_or(false);
                let is_king_in_check = king_in_check == Some(square);

                let bg_color = if is_king_in_check {
                    self.theme.check_highlight()
                } else if is_selected {
                    self.theme.selected_square()
                } else if is_last_move {
                    self.theme.last_move_highlight()
                } else if is_light {
                    self.theme.light_square()
                } else {
                    self.theme.dark_square()
                };

                // Draw square background using painter
                ui.painter().rect_filled(rect, 0.0, bg_color);

                // Draw legal move indicator
                let is_legal_destination = legal_moves_for_selected
                    .iter()
                    .any(|m| m.to() == square);

                if is_legal_destination {
                    let has_piece = self.game.piece_at(square).is_some();
                    if has_piece {
                        // Draw ring for captures
                        ui.painter().circle_stroke(
                            rect.center(),
                            square_size * 0.45,
                            Stroke::new(square_size * 0.08, self.theme.legal_move_dot()),
                        );
                    } else {
                        // Draw dot for moves
                        ui.painter().circle_filled(
                            rect.center(),
                            square_size * 0.15,
                            self.theme.legal_move_dot(),
                        );
                    }
                }

                // Draw piece
                if let Some((role, color)) = self.game.piece_at(square) {
                    let piece_size = (square_size * 0.9) as u32;
                    if piece_size > 0 {
                        if let Some(texture) = self.piece_renderer.get_texture(ui.ctx(), role, color, piece_size) {
                            let piece_rect = Rect::from_center_size(
                                rect.center(),
                                vec2(square_size * 0.9, square_size * 0.9),
                            );
                            ui.painter().image(
                                texture.id(),
                                piece_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                    }
                }

                // Draw coordinates on edge squares
                if display_file == 0 {
                    let coord_color = if is_light {
                        self.theme.coordinate_color_light()
                    } else {
                        self.theme.coordinate_color_dark()
                    };
                    let rank_char = if self.flipped {
                        (b'8' - rank_idx) as char
                    } else {
                        (b'1' + rank_idx) as char
                    };
                    ui.painter().text(
                        rect.left_top() + vec2(2.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        rank_char.to_string(),
                        egui::FontId::proportional(square_size * 0.18),
                        coord_color,
                    );
                }
                if display_rank == 7 {
                    let coord_color = if is_light {
                        self.theme.coordinate_color_light()
                    } else {
                        self.theme.coordinate_color_dark()
                    };
                    let file_char = if self.flipped {
                        (b'h' - file_idx) as char
                    } else {
                        (b'a' + file_idx) as char
                    };
                    ui.painter().text(
                        rect.right_bottom() - vec2(2.0, 2.0),
                        egui::Align2::RIGHT_BOTTOM,
                        file_char.to_string(),
                        egui::FontId::proportional(square_size * 0.18),
                        coord_color,
                    );
                }

                // Handle click interaction
                let square_id = Id::new(("chess_square", file_idx, rank_idx));
                let square_response = ui.interact(rect, square_id, Sense::click());
                
                if square_response.clicked() {
                    tracing::info!("Square CLICKED: {:?} (file_idx={}, rank_idx={})", square, file_idx, rank_idx);
                    response.square_clicked = Some(square);

                    // Check if clicking on a legal destination
                    if let Some(m) = legal_moves_for_selected
                        .iter()
                        .find(|m| m.to() == square)
                    {
                        tracing::info!("Move made: {:?}", m);
                        response.move_made = Some(m.clone());
                    }
                }
            }
        }
        });

        response
    }
}
