use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::path::PathBuf;

use crate::{
    convert::parse_any,
    event::{next_event, AppEvent},
    pretty::{annotate, AnnotatedLine},
    tree::{flatten, set_node_at_path, JNode, JPath, JScalar},
};

/// Type names shown in the dropdown — index is the type code used throughout
pub const EDIT_TYPES: [&str; 6] = ["Object", "Array", "String", "Number", "Boolean", "Null"];

pub enum EditPhase {
    TypeSelect,
    ValueEdit(tui_textarea::TextArea<'static>),
}

pub struct EditState {
    pub path: JPath,
    pub original: JNode,
    pub phase: EditPhase,
    pub type_cursor: usize, // 0-5, index into EDIT_TYPES
}

pub struct App {
    pub root: JNode,
    pub flat: Vec<crate::tree::FlatRow>,
    pub annotated: Vec<AnnotatedLine>,
    pub cursor: usize,
    pub scroll: usize,
    pub left_scroll: usize,
    pub file: Option<PathBuf>,
    pub modified: bool,
    pub status: String,
    pub quit: bool,
    pub edit: Option<EditState>,
    pub show_left: bool,
    pub show_preview: bool,
}

impl App {
    pub fn new(file: Option<PathBuf>) -> Result<Self> {
        let (root, status) = if let Some(ref path) = file {
            let src = std::fs::read_to_string(path)?;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
            let value = parse_any(&src, ext)?;
            let node = JNode::from_value(value);
            let msg = format!("{}", path.display());
            (node, msg)
        } else {
            (JNode::Object { entries: indexmap::IndexMap::new(), collapsed: false }, "new file".to_string())
        };

        let flat = flatten(&root);
        let annotated = annotate(&root);
        Ok(Self {
            root, flat, annotated,
            cursor: 0, scroll: 0, left_scroll: 0,
            file, modified: false, status, quit: false,
            edit: None,
            show_left: true, show_preview: true,
        })
    }

    fn refresh_flat(&mut self) {
        self.flat = flatten(&self.root);
        self.annotated = annotate(&self.root);
        if self.cursor >= self.flat.len() {
            self.cursor = self.flat.len().saturating_sub(1);
        }
    }

    fn current_path(&self) -> JPath {
        self.flat.get(self.cursor).map(|r| r.path.clone()).unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;

        if self.edit.is_some() {
            self.handle_key_edit(key);
            return;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, Char('q')) | (KeyModifiers::CONTROL, Char('c')) => {
                self.quit = true;
            }
            (KeyModifiers::NONE, Down) | (KeyModifiers::NONE, Char('j')) => {
                if self.cursor + 1 < self.flat.len() {
                    self.cursor += 1;
                }
            }
            (KeyModifiers::NONE, Up) | (KeyModifiers::NONE, Char('k')) => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            (KeyModifiers::NONE, PageDown) => {
                self.cursor = (self.cursor + 20).min(self.flat.len().saturating_sub(1));
            }
            (KeyModifiers::NONE, PageUp) => {
                self.cursor = self.cursor.saturating_sub(20);
            }
            (KeyModifiers::NONE, Enter) | (KeyModifiers::NONE, Char(' ')) => {
                self.toggle_collapse();
            }
            (KeyModifiers::NONE, Char('e')) => self.start_edit(),
            (KeyModifiers::NONE, Char('s')) => self.save_file(),
            (KeyModifiers::NONE, Char('[')) => self.show_left = !self.show_left,
            (KeyModifiers::NONE, Char(']')) => self.show_preview = !self.show_preview,
            _ => {}
        }
    }

    fn handle_key_edit(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;

        let in_value_edit = matches!(
            self.edit.as_ref().map(|s| &s.phase),
            Some(EditPhase::ValueEdit(_))
        );

        if in_value_edit {
            match key.code {
                Esc => {
                    // Step back to type selection without losing the path
                    if let Some(s) = self.edit.as_mut() {
                        s.phase = EditPhase::TypeSelect;
                    }
                }
                Enter => self.commit_value(),
                _ => {
                    if let Some(s) = self.edit.as_mut() {
                        if let EditPhase::ValueEdit(ta) = &mut s.phase {
                            ta.input(key);
                        }
                    }
                }
            }
        } else {
            // TypeSelect phase
            match key.code {
                Esc => { self.edit = None; }
                Up => {
                    if let Some(s) = self.edit.as_mut() {
                        s.type_cursor = s.type_cursor.saturating_sub(1);
                    }
                }
                Down => {
                    if let Some(s) = self.edit.as_mut() {
                        s.type_cursor = (s.type_cursor + 1).min(5);
                    }
                }
                Enter | Tab => self.confirm_type(),
                _ => {}
            }
        }
    }

    fn start_edit(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };

        let type_cursor = match &row.node {
            JNode::Object { .. }               => 0,
            JNode::Array { .. }                => 1,
            JNode::Scalar(JScalar::String(_))  => 2,
            JNode::Scalar(JScalar::Number(_))  => 3,
            JNode::Scalar(JScalar::Bool(_))    => 4,
            JNode::Scalar(JScalar::Null)       => 5,
        };

        self.edit = Some(EditState {
            path: row.path.clone(),
            original: row.node.clone(),
            phase: EditPhase::TypeSelect,
            type_cursor,
        });
    }

    fn confirm_type(&mut self) {
        let Some(state) = self.edit.take() else { return };
        let type_cursor = state.type_cursor;
        let path = state.path.clone();
        let original = state.original.clone();

        match type_cursor {
            0 => {
                set_node_at_path(&mut self.root, &path, JNode::Object {
                    entries: indexmap::IndexMap::new(), collapsed: false,
                });
                self.refresh_flat();
                self.modified = true;
            }
            1 => {
                set_node_at_path(&mut self.root, &path, JNode::Array {
                    items: Vec::new(), collapsed: false,
                });
                self.refresh_flat();
                self.modified = true;
            }
            5 => {
                set_node_at_path(&mut self.root, &path, JNode::Scalar(JScalar::Null));
                self.refresh_flat();
                self.modified = true;
            }
            _ => {
                // String (2), Number (3), Boolean (4): go to inline value editing
                let initial = initial_text(type_cursor, &original);
                let mut ta = tui_textarea::TextArea::new(vec![initial]);
                ta.move_cursor(tui_textarea::CursorMove::End);
                self.show_preview = true; // ensure Detail panel is visible for editing
                self.edit = Some(EditState {
                    path,
                    original,
                    phase: EditPhase::ValueEdit(ta),
                    type_cursor,
                });
            }
        }
    }

    fn commit_value(&mut self) {
        let Some(state) = self.edit.take() else { return };
        let EditState { path, type_cursor, phase, .. } = state;

        if let EditPhase::ValueEdit(ta) = phase {
            let text = ta.lines().first().cloned().unwrap_or_default();
            let new_node = JNode::Scalar(match type_cursor {
                2 => JScalar::String(text),
                3 => JScalar::Number(text.trim().to_string()),
                4 => JScalar::Bool(text.trim() == "true"),
                _ => JScalar::Null,
            });
            set_node_at_path(&mut self.root, &path, new_node);
            self.refresh_flat();
            self.modified = true;
        }
    }

    fn save_file(&mut self) {
        let path = match &self.file {
            Some(p) => p.clone(),
            None => {
                self.status = "no file to save".to_string();
                return;
            }
        };
        let value = self.root.to_value();
        match crate::convert::serialize_to(&value, "json") {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(()) => {
                    self.modified = false;
                    self.status = path.display().to_string();
                }
                Err(e) => self.status = format!("save error: {e}"),
            },
            Err(e) => self.status = format!("serialize error: {e}"),
        }
    }

    fn toggle_collapse(&mut self) {
        let path = self.current_path();
        toggle_node_collapse(&mut self.root, &path);
        self.refresh_flat();
    }
}

/// Convert the original node's value to a sensible starting text for the given scalar type.
fn initial_text(type_cursor: usize, original: &JNode) -> String {
    let current = match original {
        JNode::Scalar(JScalar::Null)       => String::new(),
        JNode::Scalar(JScalar::Bool(b))    => b.to_string(),
        JNode::Scalar(JScalar::Number(n))  => n.clone(),
        JNode::Scalar(JScalar::String(s))  => s.clone(),
        _ => String::new(),
    };
    match type_cursor {
        2 => current, // String: keep as-is
        3 => {
            if serde_json::from_str::<serde_json::Number>(current.trim()).is_ok() {
                current
            } else {
                "0".to_string()
            }
        }
        4 => {
            if current == "true" || current == "false" { current } else { "false".to_string() }
        }
        _ => String::new(),
    }
}

fn toggle_node_collapse(node: &mut JNode, path: &[crate::tree::JKey]) {
    if path.is_empty() {
        match node {
            JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            _ => {}
        }
        return;
    }
    use crate::tree::JKey;
    match node {
        JNode::Object { entries, .. } => {
            if let JKey::Field(k) = &path[0] {
                if let Some(child) = entries.get_mut(k) {
                    toggle_node_collapse(child, &path[1..]);
                }
            }
        }
        JNode::Array { items, .. } => {
            if let JKey::Index(i) = path[0] {
                if let Some(child) = items.get_mut(i) {
                    toggle_node_collapse(child, &path[1..]);
                }
            }
        }
        _ => {}
    }
}

pub fn run(file: Option<PathBuf>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(file)?;

    loop {
        let area = terminal.size()?;
        let h = area.height as usize;
        let content_h = h.saturating_sub(1);

        // Table scroll: right-top panel is 65% of content, minus 2 borders and 1 header row
        let table_inner_h = (content_h * 65 / 100).saturating_sub(3);
        if app.cursor < app.scroll {
            app.scroll = app.cursor;
        } else if app.cursor >= app.scroll + table_inner_h {
            app.scroll = app.cursor.saturating_sub(table_inner_h) + 1;
        }

        // Left panel scroll: auto-follow cursor highlight
        let left_inner_h = content_h.saturating_sub(2);
        if let Some(row) = app.flat.get(app.cursor) {
            let cp = &row.path;
            let first = app.annotated.iter().position(|al| al.path.starts_with(cp.as_slice()));
            let last = app.annotated.iter().rposition(|al| al.path.starts_with(cp.as_slice()));
            if let Some(f0) = first {
                if f0 < app.left_scroll {
                    app.left_scroll = f0;
                } else if let Some(l0) = last {
                    if l0 >= app.left_scroll + left_inner_h {
                        app.left_scroll = l0 + 1 - left_inner_h;
                    }
                }
            }
        }

        terminal.draw(|f| crate::ui::render(f, &app))?;

        match next_event(250)? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => {}
        }

        if app.quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
