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

pub struct App {
    pub root: JNode,
    pub flat: Vec<crate::tree::FlatRow>,
    pub annotated: Vec<AnnotatedLine>,
    pub cursor: usize,
    pub scroll: usize,       // table scroll (right-top panel)
    pub left_scroll: usize,  // left panel scroll (follows cursor)
    pub file: Option<PathBuf>,
    pub modified: bool,
    pub status: String,
    pub quit: bool,
    pub editing: Option<(tui_textarea::TextArea<'static>, JPath)>,
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
            editing: None,
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

        if self.editing.is_some() {
            self.handle_key_editing(key);
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
            _ => {}
        }
    }

    fn handle_key_editing(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc => { self.editing = None; }
            KeyCode::Enter => { self.commit_edit(); }
            _ => {
                if let Some((ref mut ta, _)) = self.editing {
                    ta.input(key);
                }
            }
        }
    }

    fn start_edit(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        let JNode::Scalar(ref scalar) = row.node else { return };

        let text = match scalar {
            JScalar::Null => "null".to_string(),
            JScalar::Bool(b) => b.to_string(),
            JScalar::Number(n) => n.clone(),
            JScalar::String(s) => s.clone(),
        };

        let path = row.path.clone();
        let mut ta = tui_textarea::TextArea::new(vec![text]);
        ta.move_cursor(tui_textarea::CursorMove::End);
        self.editing = Some((ta, path));
    }

    fn commit_edit(&mut self) {
        let Some((ta, path)) = self.editing.take() else { return };
        let text = ta.lines().first().cloned().unwrap_or_default();
        let scalar = parse_scalar(&text);
        set_node_at_path(&mut self.root, &path, JNode::Scalar(scalar));
        self.refresh_flat();
        self.modified = true;
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

fn parse_scalar(text: &str) -> JScalar {
    let t = text.trim();
    match t {
        "null" => JScalar::Null,
        "true" => JScalar::Bool(true),
        "false" => JScalar::Bool(false),
        _ => {
            if serde_json::from_str::<serde_json::Number>(t).is_ok() {
                JScalar::Number(t.to_string())
            } else {
                JScalar::String(text.to_string())
            }
        }
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

        // Left panel scroll: follows cursor highlight
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
