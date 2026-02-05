use crate::study::{Study, StudyManager};
use egui::Ui;

/// Navigation action from study panel
#[derive(Debug, Clone)]
pub enum StudyNavAction {
    /// Navigate to a specific position by path of child indices
    GoToPosition(Vec<usize>),
}

pub struct StudyPanel {
    study_manager: StudyManager,
    available_studies: Vec<(String, String)>, // (id, name)
    show_new_study_dialog: bool,
    new_study_name: String,
    current_comment: String,
    show_load_dialog: bool,
    export_pgn: bool,
}

impl Default for StudyPanel {
    fn default() -> Self {
        let study_manager = StudyManager::new();
        let available_studies = study_manager.list_studies().unwrap_or_default();
        
        Self {
            study_manager,
            available_studies,
            show_new_study_dialog: false,
            new_study_name: String::new(),
            current_comment: String::new(),
            show_load_dialog: false,
            export_pgn: false,
        }
    }
}

impl StudyPanel {
    /// Shows the study panel and returns any navigation action
    pub fn show(&mut self, ui: &mut Ui, study: &mut Study) -> Option<StudyNavAction> {
        let mut nav_action = None;
        
        // Handle export PGN
        if self.export_pgn {
            let pgn = study.to_pgn();
            ui.ctx().copy_text(pgn);
            self.export_pgn = false;
        }

        ui.heading("Study");
        ui.separator();

        // Study name
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut study.name);
        });

        // Chapter selector
        let current_chapter_name = study.current_chapter().name.clone();
        let current_chapter = study.current_chapter;
        let chapter_count = study.chapters.len();
        let mut switch_to: Option<usize> = None;
        ui.horizontal(|ui| {
            ui.label("Chapter:");
            egui::ComboBox::from_id_salt("chapter_select")
                .selected_text(&current_chapter_name)
                .show_ui(ui, |ui| {
                    for (idx, chapter) in study.chapters.iter().enumerate() {
                        if ui.selectable_label(
                            current_chapter == idx,
                            &chapter.name
                        ).clicked() {
                            switch_to = Some(idx);
                        }
                    }
                });
            
            if ui.button("+").clicked() {
                let chapter_num = chapter_count + 1;
                study.add_chapter(format!("Chapter {}", chapter_num));
            }
        });
        if let Some(idx) = switch_to {
            study.switch_chapter(idx);
        }

        ui.separator();

        // Comments section
        ui.label("Comments:");
        
        // Show existing comments
        let comments: Vec<String> = study.current_chapter().current_node().comments.clone();
        if comments.is_empty() {
            ui.label("No comments yet...");
        } else {
            for (i, comment) in comments.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", i + 1));
                    ui.label(comment);
                });
            }
        }

        // Add comment input
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.current_comment);
            if ui.button("Add").clicked() && !self.current_comment.is_empty() {
                study.current_chapter_mut().add_comment(self.current_comment.clone());
                self.current_comment.clear();
                study.update_timestamp();
            }
        });

        ui.separator();

        // Variations tree
        ui.label("Variations:");
        if let Some(action) = self.show_variation_tree(ui, study) {
            nav_action = Some(action);
        }

        ui.separator();

        // Save/Load buttons
        ui.horizontal(|ui| {
            if ui.button("💾 Save").clicked() {
                if let Err(e) = self.study_manager.save_study(study) {
                    tracing::error!("Failed to save study: {}", e);
                } else {
                    self.available_studies = self.study_manager.list_studies().unwrap_or_default();
                }
            }
            
            if ui.button("📂 Load").clicked() {
                self.show_load_dialog = true;
            }
            
            if ui.button("🆕 New").clicked() {
                self.show_new_study_dialog = true;
            }
        });

        // Export PGN
        if ui.button("📄 Export PGN").clicked() {
            self.export_pgn = true;
        }

        // New study dialog
        if self.show_new_study_dialog {
            egui::Window::new("New Study")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label("Study name:");
                    ui.text_edit_singleline(&mut self.new_study_name);
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() && !self.new_study_name.is_empty() {
                            *study = Study::new(self.new_study_name.clone());
                            self.new_study_name.clear();
                            self.show_new_study_dialog = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.new_study_name.clear();
                            self.show_new_study_dialog = false;
                        }
                    });
                });
        }

        // Load study dialog
        if self.show_load_dialog {
            egui::Window::new("Load Study")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    if self.available_studies.is_empty() {
                        ui.label("No saved studies found.");
                    } else {
                        for (id, name) in self.available_studies.clone().iter() {
                            if ui.button(name).clicked() {
                                if let Ok(loaded) = self.study_manager.load_study(id) {
                                    *study = loaded;
                                }
                                self.show_load_dialog = false;
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Close").clicked() {
                        self.show_load_dialog = false;
                    }
                });
        }
        
        nav_action
    }

    fn show_variation_tree(&self, ui: &mut Ui, study: &Study) -> Option<StudyNavAction> {
        let chapter = study.current_chapter();
        let mut nav_action = None;

        // Show path to current position as clickable moves
        ui.horizontal_wrapped(|ui| {
            // Start button - goes to root
            let start_text = egui::RichText::new("Start")
                .color(ui.visuals().hyperlink_color);
            let start_btn = ui.add(egui::Button::new(start_text)
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE)
                .sense(egui::Sense::click()));
            
            if start_btn.clicked() {
                nav_action = Some(StudyNavAction::GoToPosition(Vec::new()));
            }
            
            let mut node = &chapter.root;
            let mut current_path = Vec::new();
            
            for (depth, &idx) in chapter.current_path.iter().enumerate() {
                if idx < node.children.len() {
                    let child = &node.children[idx];
                    current_path.push(idx);
                    
                    if let Some(ref mv) = child.move_record {
                        // Highlight if this is on our current path
                        let is_current = depth == chapter.current_path.len() - 1;
                        
                        let text = if is_current {
                            egui::RichText::new(&mv.san)
                                .color(ui.visuals().selection.stroke.color)
                                .strong()
                        } else {
                            egui::RichText::new(&mv.san)
                                .color(ui.visuals().hyperlink_color)
                                .underline()
                        };
                        
                        let btn = ui.add(egui::Button::new(text)
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE)
                            .sense(egui::Sense::click()));
                        
                        if btn.clicked() {
                            // Navigate to this position
                            nav_action = Some(StudyNavAction::GoToPosition(current_path.clone()));
                        }
                    }
                    node = child;
                }
            }
        });

        // Show alternatives at current position as clickable moves
        let current_node = chapter.current_node();
        if !current_node.children.is_empty() {
            ui.label("Alternatives:");
            for (idx, child) in current_node.children.iter().enumerate() {
                if let Some(ref mv) = child.move_record {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", idx + 1));
                        
                        // Make the move SAN a clickable hyperlink
                        let text = egui::RichText::new(&mv.san)
                            .color(ui.visuals().hyperlink_color)
                            .underline();
                        
                        let btn = ui.add(egui::Button::new(text)
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE)
                            .sense(egui::Sense::click()));
                        
                        if btn.clicked() {
                            // Build path: current path + this child index
                            let mut new_path = chapter.current_path.clone();
                            new_path.push(idx);
                            nav_action = Some(StudyNavAction::GoToPosition(new_path));
                        }
                    });
                }
            }
        }
        
        nav_action
    }
}
