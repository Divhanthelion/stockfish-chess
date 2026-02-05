use crate::game::MoveRecord;
use serde::{Deserialize, Serialize};

/// A node in the study tree - represents a position with comments and child variations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyNode {
    /// Index into the parent's children (for identification)
    pub id: usize,
    /// The move that leads to this position (None for root)
    pub move_record: Option<MoveRecord>,
    /// Position in FEN
    pub fen: String,
    /// User comments on this position
    pub comments: Vec<String>,
    /// Child variations from this position
    pub children: Vec<StudyNode>,
}

impl StudyNode {
    pub fn new_root(fen: String) -> Self {
        Self {
            id: 0,
            move_record: None,
            fen,
            comments: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn new_child(id: usize, move_record: MoveRecord, fen: String) -> Self {
        Self {
            id,
            move_record: Some(move_record),
            fen,
            comments: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Add a child node
    pub fn add_child(&mut self, move_record: MoveRecord, fen: String) -> usize {
        let id = self.children.len();
        self.children.push(StudyNode::new_child(id, move_record, fen));
        id
    }

    /// Get all lines (sequences of moves) from this node
    pub fn get_lines(&self) -> Vec<Vec<String>> {
        let mut lines = Vec::new();
        
        for child in &self.children {
            let child_lines = child.get_lines_recursive();
            for mut line in child_lines {
                line.insert(0, child.move_record.as_ref().unwrap().san.clone());
                lines.push(line);
            }
        }
        
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        
        lines
    }

    fn get_lines_recursive(&self) -> Vec<Vec<String>> {
        let mut lines = Vec::new();
        
        for child in &self.children {
            let child_lines = child.get_lines_recursive();
            for mut line in child_lines {
                line.insert(0, child.move_record.as_ref().unwrap().san.clone());
                lines.push(line);
            }
        }
        
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        
        lines
    }
}

/// A study chapter - contains a tree of positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyChapter {
    pub id: usize,
    pub name: String,
    pub root: StudyNode,
    /// Current position in the tree (path of child indices)
    pub current_path: Vec<usize>,
}

impl StudyChapter {
    pub fn new(id: usize, name: String) -> Self {
        Self {
            id,
            name,
            root: StudyNode::new_root("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string()),
            current_path: Vec::new(),
        }
    }

    /// Get the current node based on current_path
    pub fn current_node(&self) -> &StudyNode {
        let mut node = &self.root;
        for &idx in &self.current_path {
            if idx < node.children.len() {
                node = &node.children[idx];
            } else {
                break;
            }
        }
        node
    }

    /// Get the current node mutably
    fn current_node_mut(&mut self) -> &mut StudyNode {
        let mut node = &mut self.root;
        for &idx in &self.current_path {
            if idx < node.children.len() {
                node = &mut node.children[idx];
            } else {
                break;
            }
        }
        node
    }

    /// Navigate to parent
    pub fn go_back(&mut self) -> bool {
        if self.current_path.is_empty() {
            false
        } else {
            self.current_path.pop();
            true
        }
    }

    /// Navigate to a specific child
    pub fn go_to_child(&mut self, child_idx: usize) -> bool {
        let current = self.current_node();
        if child_idx < current.children.len() {
            self.current_path.push(child_idx);
            true
        } else {
            false
        }
    }

    /// Add a move at current position
    /// Returns true if move was added, false if move already exists (navigates to it)
    pub fn add_move(&mut self, move_record: MoveRecord, fen: String) -> bool {
        let current = self.current_node_mut();
        
        // Check if this move already exists as a child
        for (idx, child) in current.children.iter().enumerate() {
            if let Some(ref child_move) = child.move_record {
                if child_move.uci == move_record.uci {
                    // Move exists, navigate to it
                    self.current_path.push(idx);
                    return false;
                }
            }
        }
        
        // Add new child
        let child_id = current.children.len();
        current.children.push(StudyNode::new_child(child_id, move_record, fen));
        self.current_path.push(child_id);
        true
    }

    /// Add a comment to current position
    pub fn add_comment(&mut self, comment: String) {
        let current = self.current_node_mut();
        current.comments.push(comment);
    }

    /// Get current FEN
    pub fn current_fen(&self) -> &str {
        &self.current_node().fen
    }

    /// Get the main line (longest variation)
    pub fn get_main_line(&self) -> Vec<String> {
        self.root.get_lines()
            .into_iter()
            .max_by_key(|line| line.len())
            .unwrap_or_default()
    }

    /// Go to start
    pub fn go_to_start(&mut self) {
        self.current_path.clear();
    }

    /// Check if we can go back
    pub fn can_go_back(&self) -> bool {
        !self.current_path.is_empty()
    }

    /// Check if we can go forward to a specific child
    pub fn can_go_forward(&self, child_idx: usize) -> bool {
        child_idx < self.current_node().children.len()
    }
}

/// A complete study with multiple chapters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Study {
    pub id: String,
    pub name: String,
    pub chapters: Vec<StudyChapter>,
    pub current_chapter: usize,
    pub created_at: String,
    pub updated_at: String,
}

impl Study {
    pub fn new(name: String) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        let mut study = Self {
            id: format!("study_{}", chrono::Local::now().timestamp_millis()),
            name,
            chapters: Vec::new(),
            current_chapter: 0,
            created_at: now.clone(),
            updated_at: now,
        };
        study.add_chapter("Chapter 1".to_string());
        study
    }

    pub fn add_chapter(&mut self, name: String) -> usize {
        let id = self.chapters.len();
        self.chapters.push(StudyChapter::new(id, name));
        self.current_chapter = id;
        id
    }

    pub fn current_chapter(&self) -> &StudyChapter {
        &self.chapters[self.current_chapter]
    }

    pub fn current_chapter_mut(&mut self) -> &mut StudyChapter {
        &mut self.chapters[self.current_chapter]
    }

    pub fn switch_chapter(&mut self, idx: usize) -> bool {
        if idx < self.chapters.len() {
            self.current_chapter = idx;
            true
        } else {
            false
        }
    }

    pub fn update_timestamp(&mut self) {
        self.updated_at = chrono::Local::now().to_rfc3339();
    }

    /// Export to PGN
    pub fn to_pgn(&self) -> String {
        let mut pgn = String::new();
        
        pgn.push_str(&format!("[Event \"{}\"]\n", self.name));
        pgn.push_str("[Site \"Stockfish Chess\"]\n");
        pgn.push_str(&format!("[Date \"{}\"]\n", &self.created_at[..10]));
        
        for chapter in &self.chapters {
            pgn.push('\n');
            pgn.push_str(&format!("[Chapter \"{}\"]\n", chapter.name));
            
            // Add comments for starting position
            if !chapter.root.comments.is_empty() {
                for comment in &chapter.root.comments {
                    pgn.push_str(&format!("{{ {} }} ", comment));
                }
                pgn.push('\n');
            }
            
            // Export main line
            let line = chapter.get_main_line();
            for (i, san) in line.iter().enumerate() {
                if i % 2 == 0 {
                    pgn.push_str(&format!("{}. ", i / 2 + 1));
                }
                pgn.push_str(san);
                pgn.push(' ');
            }
            
            pgn.push_str("*\n");
        }
        
        pgn
    }
}

impl Default for Study {
    fn default() -> Self {
        Self::new("Untitled Study".to_string())
    }
}

/// Manager for studies (save/load)
pub struct StudyManager {
    studies_dir: std::path::PathBuf,
}

impl StudyManager {
    pub fn new() -> Self {
        let studies_dir = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("Stockfish-Chess")
            .join("studies");
        
        std::fs::create_dir_all(&studies_dir).ok();
        
        Self { studies_dir }
    }

    pub fn save_study(&self, study: &Study) -> Result<(), std::io::Error> {
        let path = self.studies_dir.join(format!("{}.json", study.id));
        let json = serde_json::to_string_pretty(study)?;
        std::fs::write(path, json)
    }

    pub fn load_study(&self, id: &str) -> Result<Study, Box<dyn std::error::Error>> {
        let path = self.studies_dir.join(format!("{}.json", id));
        let json = std::fs::read_to_string(path)?;
        let study = serde_json::from_str(&json)?;
        Ok(study)
    }

    pub fn list_studies(&self) -> Result<Vec<(String, String)>, std::io::Error> {
        let mut studies = Vec::new();
        
        for entry in std::fs::read_dir(&self.studies_dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "json") {
                if let Ok(json) = std::fs::read_to_string(entry.path()) {
                    if let Ok(study) = serde_json::from_str::<Study>(&json) {
                        studies.push((study.id, study.name));
                    }
                }
            }
        }
        
        Ok(studies)
    }

    pub fn delete_study(&self, id: &str) -> Result<(), std::io::Error> {
        let path = self.studies_dir.join(format!("{}.json", id));
        std::fs::remove_file(path)
    }
}

impl Default for StudyManager {
    fn default() -> Self {
        Self::new()
    }
}
