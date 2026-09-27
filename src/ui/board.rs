use crate::game::GameState;
use crate::ui::{PieceRenderer, Theme};
use egui::{pos2, vec2, Color32, Id, Pos2, Rect, Sense, Stroke, Ui};
use shakmaty::{File, Move, Rank, Square};

pub struct ChessBoard<'a> {
    game: &'a GameState,
    theme: Theme,
    flipped: bool,
    piece_renderer: &'a mut PieceRenderer,
}

pub struct BoardResponse {
    pub move_candidates: Vec<Move>,
    pub square_clicked: Option<Square>,
    /// A drag began on this square; the caller decides whether to select it.
    pub drag_started: Option<Square>,
}

/// The square under `pos`, or `None` outside the board.
fn square_at(board_rect: Rect, flipped: bool, pos: Pos2) -> Option<Square> {
    if !board_rect.contains(pos) {
        return None;
    }
    let square_size = board_rect.width() / 8.0;
    let column = (((pos.x - board_rect.min.x) / square_size) as u32).min(7);
    let row = (((pos.y - board_rect.min.y) / square_size) as u32).min(7);
    let (file, rank) = if flipped {
        (7 - column, row)
    } else {
        (column, 7 - row)
    };
    Some(Square::from_coords(File::new(file), Rank::new(rank)))
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
            move_candidates: Vec::new(),
            square_clicked: None,
            drag_started: None,
        };
        // The selected piece while it is being dragged; drawn last, under the pointer.
        let mut dragged_piece = None;

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

            let pointer_pos = ui.ctx().pointer_interact_pos();

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
                        board_rect.min
                            + vec2(
                                display_file as f32 * square_size,
                                display_rank as f32 * square_size,
                            ),
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
                    let is_legal_destination =
                        legal_moves_for_selected.iter().any(|m| m.to() == square);

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

                    let square_id = Id::new(("chess_square", file_idx, rank_idx));
                    let is_dragging_from_here =
                        *selected_square == Some(square) && ui.ctx().is_being_dragged(square_id);

                    // Draw piece
                    if let Some((role, color)) = self.game.piece_at(square) {
                        if is_dragging_from_here {
                            dragged_piece = Some((role, color));
                        }
                    }
                    if let Some((role, color)) = self
                        .game
                        .piece_at(square)
                        .filter(|_| !is_dragging_from_here)
                    {
                        let piece_size = (square_size * 0.9) as u32;
                        if piece_size > 0 {
                            if let Some(texture) =
                                self.piece_renderer
                                    .get_texture(ui.ctx(), role, color, piece_size)
                            {
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
                        let rank_char = (b'1' + rank_idx) as char;
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
                        let file_char = (b'a' + file_idx) as char;
                        ui.painter().text(
                            rect.right_bottom() - vec2(2.0, 2.0),
                            egui::Align2::RIGHT_BOTTOM,
                            file_char.to_string(),
                            egui::FontId::proportional(square_size * 0.18),
                            coord_color,
                        );
                    }

                    // Handle click and drag interaction
                    let square_response = ui.interact(rect, square_id, Sense::click_and_drag());

                    if square_response.drag_started() {
                        response.drag_started = Some(square);
                    }

                    // A click, or a drag released over another square, targets
                    // that square with the currently selected piece.
                    let target = if square_response.clicked() {
                        Some(square)
                    } else if square_response.drag_stopped() {
                        pointer_pos.and_then(|pos| square_at(board_rect, self.flipped, pos))
                    } else {
                        None
                    };

                    if let Some(target) = target {
                        tracing::debug!("Square targeted: {:?}", target);
                        response.square_clicked = Some(target);
                        response.move_candidates = legal_moves_for_selected
                            .iter()
                            .filter(|m| m.to() == target)
                            .copied()
                            .collect();
                    }
                }
            }

            if let (Some((role, color)), Some(pos)) = (dragged_piece, pointer_pos) {
                let piece_size = (square_size * 0.9) as u32;
                if let Some(texture) =
                    self.piece_renderer
                        .get_texture(ui.ctx(), role, color, piece_size.max(1))
                {
                    ui.painter().image(
                        texture.id(),
                        Rect::from_center_size(pos, vec2(square_size * 0.9, square_size * 0.9)),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            }
        });

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 800.0))
    }

    #[test]
    fn maps_pointer_to_square_for_white() {
        assert_eq!(
            square_at(board(), false, pos2(50.0, 750.0)),
            Some(Square::A1)
        );
        assert_eq!(
            square_at(board(), false, pos2(750.0, 50.0)),
            Some(Square::H8)
        );
        assert_eq!(
            square_at(board(), false, pos2(450.0, 450.0)),
            Some(Square::E4)
        );
    }

    #[test]
    fn maps_pointer_to_square_when_flipped() {
        assert_eq!(
            square_at(board(), true, pos2(50.0, 750.0)),
            Some(Square::H8)
        );
        assert_eq!(
            square_at(board(), true, pos2(750.0, 50.0)),
            Some(Square::A1)
        );
        assert_eq!(
            square_at(board(), true, pos2(350.0, 350.0)),
            Some(Square::E4)
        );
    }

    #[test]
    fn ignores_pointer_outside_the_board() {
        assert_eq!(square_at(board(), false, pos2(-1.0, 400.0)), None);
        assert_eq!(square_at(board(), false, pos2(400.0, 801.0)), None);
    }
}
