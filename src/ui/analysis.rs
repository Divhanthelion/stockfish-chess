use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Default)]
pub struct EngineLine {
    pub id: u32, // 1-indexed multipv id from engine
    pub score_cp: Option<i32>,
    pub score_mate: Option<i32>,
    pub depth: u32,
    pub pv: Vec<String>,
}

impl EngineLine {
    pub fn format_score(&self) -> String {
        if let Some(mate) = self.score_mate {
            if mate > 0 {
                format!("+M{}", mate)
            } else {
                format!("-M{}", mate.abs())
            }
        } else if let Some(cp) = self.score_cp {
            let pawns = cp as f32 / 100.0;
            if pawns >= 0.0 {
                format!("+{:.2}", pawns)
            } else {
                format!("{:.2}", pawns)
            }
        } else {
            "--".to_string()
        }
    }

    pub fn score_for_sorting(&self) -> f32 {
        if let Some(mate) = self.score_mate {
            if mate > 0 {
                1000.0 - mate as f32
            } else {
                -1000.0 + mate.abs() as f32
            }
        } else if let Some(cp) = self.score_cp {
            cp as f32 / 100.0
        } else {
            0.0
        }
    }

    pub fn normalized_score(&self) -> f32 {
        if let Some(mate) = self.score_mate {
            if mate > 0 { 1.0 } else { -1.0 }
        } else if let Some(cp) = self.score_cp {
            let pawns = (cp as f32 / 100.0).clamp(-10.0, 10.0);
            pawns / 10.0
        } else {
            0.0
        }
    }
}

pub struct AnalysisPanel {
    /// All lines received from engine (up to 5)
    pub all_lines: Vec<EngineLine>,
    /// Number of lines to display (1-5)
    pub display_lines: u32,
    /// Maximum lines the engine is calculating
    pub max_calculated: u32,
    pub is_analyzing: bool,
    pub total_nodes: u64,
    pub current_depth: u32,
}

impl Default for AnalysisPanel {
    fn default() -> Self {
        Self {
            all_lines: Vec::new(),
            display_lines: 3,
            max_calculated: 5,
            is_analyzing: false,
            total_nodes: 0,
            current_depth: 0,
        }
    }
}

impl AnalysisPanel {
    /// Returns clicked moves if user clicked on PV moves
    /// Returns Vec<(move_uci, line_index)> for all clicked moves
    pub fn show(&mut self, ui: &mut Ui) -> Vec<(String, usize)> {
        let mut clicked_moves: Vec<(String, usize)> = Vec::new();
        
        ui.vertical(|ui| {
            ui.heading("Analysis");
            ui.separator();

            // Status and controls
            ui.horizontal(|ui| {
                if self.is_analyzing {
                    ui.spinner();
                    ui.label("Analyzing...");
                } else {
                    ui.label("⏸ Paused");
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("d{}", self.current_depth));
                });
            });

            ui.add_space(8.0);

            // Evaluation bar (from best line)
            if let Some(best) = self.all_lines.first() {
                self.show_eval_bar(ui, best);
            }

            ui.add_space(8.0);

            // Number of lines dropdown
            ui.horizontal(|ui| {
                ui.label("Lines:");
                egui::ComboBox::from_id_salt("lines_dropdown")
                    .width(60.0)
                    .selected_text(format!("{}", self.display_lines))
                    .show_ui(ui, |ui| {
                        for n in 1..=5 {
                            ui.selectable_value(&mut self.display_lines, n, format!("{}", n));
                        }
                    });
                ui.label(format!("/ {} calculating", self.max_calculated));
            });

            ui.add_space(8.0);
            ui.separator();

            // Engine lines - only show display_lines
            let lines_to_show: Vec<_> = self.all_lines.iter()
                .take(self.display_lines as usize)
                .cloned()
                .collect();
                
            for line in &lines_to_show {
                if let Some((mv, idx)) = self.show_engine_line(ui, line) {
                    clicked_moves.push((mv, idx));
                }
            }

            if self.all_lines.is_empty() {
                ui.label("No analysis yet...");
            }
        });
        
        clicked_moves
    }

    fn show_eval_bar(&self, ui: &mut Ui, line: &EngineLine) {
        let available_width = ui.available_width();
        if available_width < 20.0 {
            return;
        }
        let bar_height = 24.0;
        let (rect, _response) = ui.allocate_exact_size(
            Vec2::new(available_width, bar_height),
            egui::Sense::hover(),
        );

        if rect.width() < 1.0 || rect.height() < 1.0 {
            return;
        }

        let painter = ui.painter();

        // Background (black side)
        painter.rect_filled(rect, CornerRadius::same(4), Color32::BLACK);

        // White portion
        let score = line.normalized_score();
        let white_width = (rect.width() * (0.5 + score * 0.5)).clamp(0.0, rect.width());
        
        if white_width > 0.0 {
            let white_rect = Rect::from_min_size(
                rect.min,
                Vec2::new(white_width, rect.height()),
            );
            painter.rect_filled(white_rect, CornerRadius::same(4), Color32::WHITE);
        }

        // Center line
        let center_x = rect.min.x + rect.width() * 0.5;
        painter.line_segment(
            [
                Pos2::new(center_x, rect.min.y),
                Pos2::new(center_x, rect.max.y),
            ],
            Stroke::new(2.0, Color32::GRAY),
        );

        // Border
        painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Middle);

        // Score text
        if rect.width() > 50.0 && rect.height() > 10.0 {
            let score_text = line.format_score();
            let text_color = Color32::WHITE;
            let _ = painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                score_text,
                egui::FontId::proportional(12.0),
                text_color,
            );
        }
    }

    /// Shows an engine line, returns Some(move) if a move was clicked
    /// Returns the move UCI and the index in the PV (for multi-move navigation)
    fn show_engine_line(&self, ui: &mut Ui, line: &EngineLine) -> Option<(String, usize)> {
        let mut clicked = None;
        
        ui.horizontal_wrapped(|ui| {
            // Line number and score
            ui.label(format!("{}.", line.id));
            
            let score_text = line.format_score();
            let color = if line.score_cp.unwrap_or(0) > 0 || line.score_mate.unwrap_or(0) > 0 {
                Color32::GREEN
            } else if line.score_cp.unwrap_or(0) < 0 || line.score_mate.unwrap_or(0) < 0 {
                Color32::RED
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(color, score_text);
            
            // PV moves as clickable hyperlinks (ALL of them)
            if !line.pv.is_empty() {
                for (i, mv) in line.pv.iter().enumerate() {
                    // All moves are clickable
                    let response = ui.add(egui::Label::new(
                        egui::RichText::new(mv)
                            .color(ui.visuals().hyperlink_color)
                            .underline()
                    ).sense(egui::Sense::click()));
                    
                    if response.clicked() {
                        clicked = Some((mv.clone(), i));
                    }
                    ui.label(" ");
                }
            }
        });
        
        clicked
    }

    /// Update a line from engine output (always store up to 5)
    pub fn update_line(&mut self, multipv: u32, score_cp: Option<i32>, score_mate: Option<i32>, depth: Option<u32>, pv: Vec<String>) {
        let id = multipv.max(1);
        
        if let Some(d) = depth {
            self.current_depth = self.current_depth.max(d);
        }
        
        // Find existing line or create new
        if let Some(line) = self.all_lines.iter_mut().find(|l| l.id == id) {
            line.score_cp = score_cp;
            line.score_mate = score_mate;
            if let Some(d) = depth {
                line.depth = d;
            }
            if !pv.is_empty() {
                line.pv = pv;
            }
        } else {
            self.all_lines.push(EngineLine {
                id,
                score_cp,
                score_mate,
                depth: depth.unwrap_or(0),
                pv,
            });
            // Sort by score (best first)
            self.all_lines.sort_by(|a, b| {
                b.score_for_sorting().partial_cmp(&a.score_for_sorting()).unwrap()
            });
            // Reassign IDs after sorting to match multipv order
            for (i, line) in self.all_lines.iter_mut().enumerate() {
                line.id = (i + 1) as u32;
            }
        }
        
        // Track max calculated
        self.max_calculated = self.max_calculated.max(id);
    }

    pub fn clear(&mut self) {
        self.all_lines.clear();
        self.current_depth = 0;
        self.total_nodes = 0;
        self.max_calculated = 5;
    }

    pub fn get_display_lines(&self) -> u32 {
        self.display_lines
    }
    
    pub fn set_display_lines(&mut self, n: u32) {
        self.display_lines = n.clamp(1, 5);
    }
}
