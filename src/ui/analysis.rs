use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

#[derive(Debug, Clone)]
pub struct Evaluation {
    pub score_cp: Option<i32>,
    pub score_mate: Option<i32>,
    pub depth: u32,
    pub pv: Vec<String>,
    pub nodes: u64,
}

impl Default for Evaluation {
    fn default() -> Self {
        Self {
            score_cp: None,
            score_mate: None,
            depth: 0,
            pv: Vec::new(),
            nodes: 0,
        }
    }
}

impl Evaluation {
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

    pub fn is_white_better(&self) -> bool {
        if let Some(mate) = self.score_mate {
            mate > 0
        } else if let Some(cp) = self.score_cp {
            cp > 0
        } else {
            false
        }
    }

    /// Get a value between -1.0 (black winning) and 1.0 (white winning)
    pub fn normalized_score(&self) -> f32 {
        if let Some(mate) = self.score_mate {
            if mate > 0 {
                1.0
            } else {
                -1.0
            }
        } else if let Some(cp) = self.score_cp {
            // Clamp between -10 and +10 pawns, then normalize
            let pawns = (cp as f32 / 100.0).clamp(-10.0, 10.0);
            pawns / 10.0
        } else {
            0.0
        }
    }
}

pub struct AnalysisPanel {
    pub evaluation: Evaluation,
    pub is_analyzing: bool,
}

impl Default for AnalysisPanel {
    fn default() -> Self {
        Self {
            evaluation: Evaluation::default(),
            is_analyzing: false,
        }
    }
}

impl AnalysisPanel {
    pub fn show(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("Analysis");
            ui.separator();

            // Status indicator
            ui.horizontal(|ui| {
                if self.is_analyzing {
                    ui.spinner();
                    ui.label("Analyzing...");
                } else {
                    ui.label("⏸ Paused");
                }
            });

            ui.add_space(8.0);

            // Evaluation bar
            self.show_eval_bar(ui);

            ui.add_space(8.0);

            // Score display
            ui.horizontal(|ui| {
                ui.label("Score:");
                let score_text = self.evaluation.format_score();
                let color = if self.evaluation.is_white_better() {
                    Color32::GREEN
                } else if self.evaluation.score_cp.unwrap_or(0) < 0 || self.evaluation.score_mate.unwrap_or(1) < 0 {
                    Color32::RED
                } else {
                    ui.visuals().text_color()
                };
                ui.colored_label(color, score_text);
            });

            // Depth and nodes
            ui.horizontal(|ui| {
                ui.label(format!("Depth: {}", self.evaluation.depth));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Nodes: {}", format_nodes(self.evaluation.nodes)));
                });
            });

            ui.add_space(8.0);
            ui.separator();

            // Principal variation (best line)
            ui.label("Best line:");
            if !self.evaluation.pv.is_empty() {
                let pv_text = self.evaluation.pv.join(" ");
                ui.add(egui::Label::new(pv_text).wrap());
            } else {
                ui.label("--");
            }
        });
    }

    fn show_eval_bar(&self, ui: &mut Ui) {
        let available_width = ui.available_width();
        let bar_height = 24.0;
        let (rect, _response) = ui.allocate_exact_size(
            Vec2::new(available_width, bar_height),
            egui::Sense::hover(),
        );

        let painter = ui.painter();

        // Background (black side)
        painter.rect_filled(rect, CornerRadius::same(4), Color32::BLACK);

        // White portion
        let score = self.evaluation.normalized_score();
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
    }

    pub fn update_evaluation(&mut self, score_cp: Option<i32>, score_mate: Option<i32>, depth: Option<u32>, pv: Vec<String>, nodes: Option<u64>) {
        if let Some(d) = depth {
            self.evaluation.depth = d;
        }
        if let Some(cp) = score_cp {
            self.evaluation.score_cp = Some(cp);
            self.evaluation.score_mate = None;
        }
        if let Some(mate) = score_mate {
            self.evaluation.score_mate = Some(mate);
        }
        self.evaluation.pv = pv;
        if let Some(n) = nodes {
            self.evaluation.nodes = n;
        }
    }
}

fn format_nodes(nodes: u64) -> String {
    if nodes >= 1_000_000 {
        format!("{:.1}M", nodes as f64 / 1_000_000.0)
    } else if nodes >= 1_000 {
        format!("{:.1}K", nodes as f64 / 1_000.0)
    } else {
        nodes.to_string()
    }
}
