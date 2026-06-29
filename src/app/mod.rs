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
    tree::{flatten, get_node_at_path, get_node_at_path_mut, set_node_at_path, JNode, JKey, JPath, JScalar},
};

pub const EDIT_TYPES: [&str; 6] = ["Object", "Array", "String", "Number", "Boolean", "Null"];

#[derive(PartialEq)]
pub enum EditMode {
    Edit,      // editing an existing node (v0.3)
    AddChild,  // adding a new child to a container
}

pub enum EditPhase {
    TypeSelect,
    KeyEdit(tui_textarea::TextArea<'static>),   // Object child: key name input
    ValueEdit(tui_textarea::TextArea<'static>), // scalar value input
}

pub struct EditState {
    pub path: JPath,            // Edit: path of node; AddChild: path of container
    pub mode: EditMode,
    pub original: Option<JNode>, // Some for Edit (for potential cancel), None for AddChild
    pub phase: EditPhase,
    pub type_cursor: usize,     // 0-5, index into EDIT_TYPES
    pub pending_key: Option<String>, // key entered in KeyEdit, held until ValueEdit commits
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
    pub confirm_quit: bool,
    pub edit: Option<EditState>,
    pub clipboard: Option<JNode>,
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
            file, modified: false, status, quit: false, confirm_quit: false,
            edit: None, clipboard: None,
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
            (KeyModifiers::CONTROL, Char('c')) => {
                self.quit = true;
            }
            (KeyModifiers::NONE, Char('q')) => {
                if self.confirm_quit {
                    self.quit = true;
                } else {
                    self.confirm_quit = true;
                }
            }
            (KeyModifiers::NONE, Down) | (KeyModifiers::NONE, Char('j')) => {
                self.confirm_quit = false;
                if self.cursor + 1 < self.flat.len() {
                    self.cursor += 1;
                }
            }
            (KeyModifiers::NONE, Up) | (KeyModifiers::NONE, Char('k')) => {
                self.confirm_quit = false;
                self.cursor = self.cursor.saturating_sub(1);
            }
            (KeyModifiers::NONE, PageDown) => {
                self.confirm_quit = false;
                self.cursor = (self.cursor + 20).min(self.flat.len().saturating_sub(1));
            }
            (KeyModifiers::NONE, PageUp) => {
                self.confirm_quit = false;
                self.cursor = self.cursor.saturating_sub(20);
            }
            (KeyModifiers::NONE, Enter) | (KeyModifiers::NONE, Char(' ')) => {
                self.confirm_quit = false;
                self.toggle_collapse();
            }
            (KeyModifiers::NONE, Char('e')) => { self.confirm_quit = false; self.start_edit(); }
            (KeyModifiers::NONE, Char('a')) => { self.confirm_quit = false; self.start_add_child(); }
            (KeyModifiers::NONE, Char('d')) => { self.confirm_quit = false; self.delete_node(); }
            (KeyModifiers::SHIFT, Char('D')) | (KeyModifiers::NONE, Char('D')) => { self.confirm_quit = false; self.duplicate_node(); }
            (KeyModifiers::NONE, Char('y')) => { self.confirm_quit = false; self.copy_node(); }
            (KeyModifiers::NONE, Char('p')) => { self.confirm_quit = false; self.paste_node(true); }
            (KeyModifiers::SHIFT, Char('P')) | (KeyModifiers::NONE, Char('P')) => { self.confirm_quit = false; self.paste_node(false); }
            (KeyModifiers::SHIFT, Char('K')) | (KeyModifiers::NONE, Char('K')) => { self.confirm_quit = false; self.move_node(true); }
            (KeyModifiers::SHIFT, Char('J')) | (KeyModifiers::NONE, Char('J')) => { self.confirm_quit = false; self.move_node(false); }
            (KeyModifiers::NONE, Char('s')) => { self.confirm_quit = false; self.save_file(); }
            (KeyModifiers::NONE, Char('[')) => { self.confirm_quit = false; self.show_left = !self.show_left; }
            (KeyModifiers::NONE, Char(']')) => { self.confirm_quit = false; self.show_preview = !self.show_preview; }
            _ => { self.confirm_quit = false; }
        }
    }

    fn handle_key_edit(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;

        let phase_kind = match self.edit.as_ref().map(|s| &s.phase) {
            Some(EditPhase::TypeSelect)  => 0,
            Some(EditPhase::KeyEdit(_))  => 1,
            Some(EditPhase::ValueEdit(_)) => 2,
            None => return,
        };

        match phase_kind {
            0 => match key.code {
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
            },
            1 => match key.code {
                Esc => {
                    if let Some(s) = self.edit.as_mut() {
                        s.phase = EditPhase::TypeSelect;
                        s.pending_key = None;
                    }
                }
                Enter => self.confirm_key(),
                _ => {
                    if let Some(s) = self.edit.as_mut() {
                        if let EditPhase::KeyEdit(ta) = &mut s.phase {
                            ta.input(key);
                        }
                    }
                }
            },
            _ => match key.code {
                Esc => {
                    if let Some(s) = self.edit.as_mut() {
                        s.phase = EditPhase::TypeSelect;
                        s.pending_key = None;
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
            },
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
            mode: EditMode::Edit,
            original: Some(row.node.clone()),
            phase: EditPhase::TypeSelect,
            type_cursor,
            pending_key: None,
        });
    }

    fn start_add_child(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        if !matches!(&row.node, JNode::Object { .. } | JNode::Array { .. }) {
            self.status = "select a container to add a child".to_string();
            return;
        }

        let path = row.path.clone();
        let is_collapsed = row.node.is_collapsed();

        // Auto-expand so the new child is visible immediately
        if is_collapsed {
            toggle_node_collapse(&mut self.root, &path);
            self.refresh_flat();
            if let Some(pos) = self.flat.iter().position(|r| r.path == path) {
                self.cursor = pos;
            }
        }

        self.edit = Some(EditState {
            path,
            mode: EditMode::AddChild,
            original: None,
            phase: EditPhase::TypeSelect,
            type_cursor: 2, // default to String
            pending_key: None,
        });
    }

    fn confirm_type(&mut self) {
        let Some(state) = self.edit.take() else { return };
        match state.mode {
            EditMode::Edit     => self.confirm_type_edit(state),
            EditMode::AddChild => self.confirm_type_add(state),
        }
    }

    fn confirm_type_edit(&mut self, state: EditState) {
        let original = state.original.as_ref().map(|n| n).cloned().unwrap_or(JNode::Scalar(JScalar::Null));
        match state.type_cursor {
            0 => {
                set_node_at_path(&mut self.root, &state.path, JNode::Object {
                    entries: indexmap::IndexMap::new(), collapsed: false,
                });
                self.refresh_flat();
                self.modified = true;
            }
            1 => {
                set_node_at_path(&mut self.root, &state.path, JNode::Array {
                    items: Vec::new(), collapsed: false,
                });
                self.refresh_flat();
                self.modified = true;
            }
            5 => {
                set_node_at_path(&mut self.root, &state.path, JNode::Scalar(JScalar::Null));
                self.refresh_flat();
                self.modified = true;
            }
            tc => {
                let initial = initial_text(tc, &original);
                let mut ta = tui_textarea::TextArea::new(vec![initial]);
                ta.move_cursor(tui_textarea::CursorMove::End);
                self.show_preview = true;
                self.edit = Some(EditState { phase: EditPhase::ValueEdit(ta), ..state });
            }
        }
    }

    fn confirm_type_add(&mut self, state: EditState) {
        let parent_is_object = matches!(
            get_node_at_path(&self.root, &state.path),
            Some(JNode::Object { .. })
        );

        if parent_is_object {
            // Need key name before value
            let ta = tui_textarea::TextArea::new(vec![String::new()]);
            self.show_preview = true;
            self.edit = Some(EditState { phase: EditPhase::KeyEdit(ta), ..state });
        } else {
            // Array parent
            let path = state.path.clone();
            let tc = state.type_cursor;
            match tc {
                0 => self.insert_child(&path, None, JNode::Object { entries: indexmap::IndexMap::new(), collapsed: false }),
                1 => self.insert_child(&path, None, JNode::Array { items: Vec::new(), collapsed: false }),
                5 => self.insert_child(&path, None, JNode::Scalar(JScalar::Null)),
                _ => {
                    let initial = initial_text(tc, &JNode::Scalar(JScalar::Null));
                    let mut ta = tui_textarea::TextArea::new(vec![initial]);
                    ta.move_cursor(tui_textarea::CursorMove::End);
                    self.show_preview = true;
                    self.edit = Some(EditState { phase: EditPhase::ValueEdit(ta), ..state });
                }
            }
        }
    }

    fn confirm_key(&mut self) {
        let key = {
            if let Some(ref state) = self.edit {
                if let EditPhase::KeyEdit(ref ta) = state.phase {
                    ta.lines().first().cloned().unwrap_or_default().trim().to_string()
                } else { return; }
            } else { return; }
        };

        if key.is_empty() {
            self.status = "key name cannot be empty".to_string();
            return;
        }

        let Some(state) = self.edit.take() else { return };
        let path = state.path.clone();
        let tc = state.type_cursor;

        match tc {
            0 => self.insert_child(&path, Some(&key), JNode::Object { entries: indexmap::IndexMap::new(), collapsed: false }),
            1 => self.insert_child(&path, Some(&key), JNode::Array { items: Vec::new(), collapsed: false }),
            5 => self.insert_child(&path, Some(&key), JNode::Scalar(JScalar::Null)),
            _ => {
                let initial = initial_text(tc, &JNode::Scalar(JScalar::Null));
                let mut ta = tui_textarea::TextArea::new(vec![initial]);
                ta.move_cursor(tui_textarea::CursorMove::End);
                self.show_preview = true;
                self.edit = Some(EditState {
                    path: state.path,
                    mode: state.mode,
                    original: state.original,
                    phase: EditPhase::ValueEdit(ta),
                    type_cursor: state.type_cursor,
                    pending_key: Some(key),
                });
            }
        }
    }

    fn commit_value(&mut self) {
        let Some(state) = self.edit.take() else { return };
        let EditPhase::ValueEdit(ta) = state.phase else { return };

        let text = ta.lines().first().cloned().unwrap_or_default();
        let new_node = JNode::Scalar(match state.type_cursor {
            2 => JScalar::String(text),
            3 => JScalar::Number(text.trim().to_string()),
            4 => JScalar::Bool(text.trim() == "true"),
            _ => JScalar::Null,
        });

        match state.mode {
            EditMode::Edit => {
                set_node_at_path(&mut self.root, &state.path, new_node);
                self.refresh_flat();
                self.modified = true;
            }
            EditMode::AddChild => {
                let path = state.path.clone();
                let key = state.pending_key.as_deref().map(|s| s.to_string());
                self.insert_child(&path, key.as_deref(), new_node);
            }
        }
    }

    fn delete_node(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        if row.path.is_empty() {
            self.status = "cannot delete root".to_string();
            return;
        }

        let parent_path = row.path[..row.path.len() - 1].to_vec();
        let last_key = row.path.last().unwrap().clone();

        if let Some(parent) = get_node_at_path_mut(&mut self.root, &parent_path) {
            match (parent, &last_key) {
                (JNode::Object { entries, .. }, JKey::Field(k)) => {
                    entries.shift_remove(k.as_str());
                }
                (JNode::Array { items, .. }, JKey::Index(i)) => {
                    items.remove(*i);
                }
                _ => return,
            }
        }

        self.refresh_flat();
        self.modified = true;
    }

    fn duplicate_node(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        if row.path.is_empty() {
            return;
        }

        let node_clone = row.node.clone();
        let parent_path = row.path[..row.path.len() - 1].to_vec();
        let last_key = row.path.last().unwrap().clone();
        let mut new_path: Option<JPath> = None;

        if let Some(parent) = get_node_at_path_mut(&mut self.root, &parent_path) {
            match (parent, &last_key) {
                (JNode::Array { items, .. }, JKey::Index(i)) => {
                    let insert_at = *i + 1;
                    items.insert(insert_at, node_clone);
                    let mut p = parent_path.clone();
                    p.push(JKey::Index(insert_at));
                    new_path = Some(p);
                }
                (JNode::Object { entries, .. }, JKey::Field(k)) => {
                    let idx = entries.get_index_of(k.as_str()).unwrap_or(entries.len() - 1);
                    let new_key = unique_key(entries, k);
                    entries.shift_insert(idx + 1, new_key.clone(), node_clone);
                    let mut p = parent_path.clone();
                    p.push(JKey::Field(new_key));
                    new_path = Some(p);
                }
                _ => {}
            }
        }

        self.refresh_flat();
        self.modified = true;

        if let Some(np) = new_path {
            if let Some(pos) = self.flat.iter().position(|r| r.path == np) {
                self.cursor = pos;
            }
        }
    }

    fn copy_node(&mut self) {
        if let Some(row) = self.flat.get(self.cursor) {
            let label = match (&row.key, row.index) {
                (Some(k), _) => k.clone(),
                (None, Some(i)) => format!("Item[{}]", i),
                (None, None) => "<root>".to_string(),
            };
            self.clipboard = Some(row.node.clone());
            self.status = format!("copied: {}", label);
        }
    }

    fn paste_node(&mut self, after: bool) {
        let Some(node) = self.clipboard.clone() else {
            self.status = "clipboard is empty  (y to copy)".to_string();
            return;
        };
        let Some(row) = self.flat.get(self.cursor) else { return };
        if row.path.is_empty() {
            self.status = "cannot paste sibling of root".to_string();
            return;
        }

        let parent_path = row.path[..row.path.len() - 1].to_vec();
        let last_key = row.path.last().unwrap().clone();
        let mut new_path: Option<JPath> = None;

        if let Some(parent) = get_node_at_path_mut(&mut self.root, &parent_path) {
            match (parent, &last_key) {
                (JNode::Array { items, .. }, JKey::Index(i)) => {
                    let insert_at = if after { *i + 1 } else { *i };
                    items.insert(insert_at, node);
                    let mut p = parent_path.clone();
                    p.push(JKey::Index(insert_at));
                    new_path = Some(p);
                }
                (JNode::Object { entries, .. }, JKey::Field(k)) => {
                    let idx = entries.get_index_of(k.as_str()).unwrap_or(0);
                    let base = format!("{}_paste", k);
                    let new_key = unique_key(entries, &base);
                    let insert_at = if after { idx + 1 } else { idx };
                    entries.shift_insert(insert_at, new_key.clone(), node);
                    let mut p = parent_path.clone();
                    p.push(JKey::Field(new_key));
                    new_path = Some(p);
                }
                _ => {}
            }
        }

        self.refresh_flat();
        self.modified = true;

        if let Some(np) = new_path {
            if let Some(pos) = self.flat.iter().position(|r| r.path == np) {
                self.cursor = pos;
            }
        }
    }

    fn move_node(&mut self, up: bool) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        if row.path.is_empty() { return; }

        let parent_path = row.path[..row.path.len() - 1].to_vec();
        let last_key = row.path.last().unwrap().clone();
        let mut new_last_key: Option<JKey> = None;

        if let Some(parent) = get_node_at_path_mut(&mut self.root, &parent_path) {
            match (parent, &last_key) {
                (JNode::Array { items, .. }, JKey::Index(i)) => {
                    let i = *i;
                    if up && i > 0 {
                        items.swap(i, i - 1);
                        new_last_key = Some(JKey::Index(i - 1));
                    } else if !up && i + 1 < items.len() {
                        items.swap(i, i + 1);
                        new_last_key = Some(JKey::Index(i + 1));
                    }
                }
                (JNode::Object { entries, .. }, JKey::Field(k)) => {
                    if let Some(idx) = entries.get_index_of(k.as_str()) {
                        if up && idx > 0 {
                            entries.swap_indices(idx, idx - 1);
                            new_last_key = Some(last_key.clone()); // key unchanged
                        } else if !up && idx + 1 < entries.len() {
                            entries.swap_indices(idx, idx + 1);
                            new_last_key = Some(last_key.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        if new_last_key.is_none() { return; }

        self.refresh_flat();
        self.modified = true;

        let mut new_path = parent_path;
        new_path.push(new_last_key.unwrap());
        if let Some(pos) = self.flat.iter().position(|r| r.path == new_path) {
            self.cursor = pos;
        }
    }

    fn insert_child(&mut self, container_path: &JPath, key: Option<&str>, new_node: JNode) {
        let mut new_path: Option<JPath> = None;

        if let Some(container) = get_node_at_path_mut(&mut self.root, container_path) {
            match container {
                JNode::Array { items, collapsed } => {
                    *collapsed = false;
                    let new_idx = items.len();
                    items.push(new_node);
                    let mut p = container_path.to_vec();
                    p.push(JKey::Index(new_idx));
                    new_path = Some(p);
                }
                JNode::Object { entries, collapsed } => {
                    if let Some(k) = key {
                        *collapsed = false;
                        entries.insert(k.to_string(), new_node);
                        let mut p = container_path.to_vec();
                        p.push(JKey::Field(k.to_string()));
                        new_path = Some(p);
                    }
                }
                _ => {}
            }
        }

        self.refresh_flat();
        self.modified = true;

        if let Some(np) = new_path {
            if let Some(pos) = self.flat.iter().position(|r| r.path == np) {
                self.cursor = pos;
            }
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

fn initial_text(type_cursor: usize, original: &JNode) -> String {
    let current = match original {
        JNode::Scalar(JScalar::Null)       => String::new(),
        JNode::Scalar(JScalar::Bool(b))    => b.to_string(),
        JNode::Scalar(JScalar::Number(n))  => n.clone(),
        JNode::Scalar(JScalar::String(s))  => s.clone(),
        _ => String::new(),
    };
    match type_cursor {
        2 => current,
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

fn unique_key(entries: &indexmap::IndexMap<String, JNode>, base: &str) -> String {
    let candidate = format!("{}_copy", base);
    if !entries.contains_key(&candidate) {
        return candidate;
    }
    for i in 2..=99 {
        let c = format!("{}_{}", base, i);
        if !entries.contains_key(&c) {
            return c;
        }
    }
    candidate
}

fn toggle_node_collapse(node: &mut JNode, path: &[JKey]) {
    if path.is_empty() {
        match node {
            JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            _ => {}
        }
        return;
    }
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

        let table_inner_h = (content_h * 65 / 100).saturating_sub(3);
        if app.cursor < app.scroll {
            app.scroll = app.cursor;
        } else if app.cursor >= app.scroll + table_inner_h {
            app.scroll = app.cursor.saturating_sub(table_inner_h) + 1;
        }

        let left_inner_h = content_h.saturating_sub(2);
        if let Some(row) = app.flat.get(app.cursor) {
            let cp = &row.path;
            let first = app.annotated.iter().position(|al| al.path.starts_with(cp.as_slice()));
            let last  = app.annotated.iter().rposition(|al| al.path.starts_with(cp.as_slice()));
            if let Some(f0) = first {
                let l0 = last.unwrap_or(f0);
                let block_h = l0 - f0 + 1;
                if f0 < app.left_scroll {
                    // Top of block scrolled out of view: bring it back
                    app.left_scroll = f0;
                } else if block_h <= left_inner_h && l0 >= app.left_scroll + left_inner_h {
                    // Block fits in view but bottom is below: scroll to reveal bottom
                    app.left_scroll = l0 + 1 - left_inner_h;
                }
                // If block is larger than the view (e.g. root, large container):
                // keep current scroll — no need to chase the bottom
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
