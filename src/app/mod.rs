use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::{Backend, CrosstermBackend}, Terminal};
use std::path::PathBuf;
use std::rc::Rc;

use crate::{
    convert::parse_any,
    event::{next_event, AppEvent},
    lint::{lint, patch_lint, LintWarning},
    plugin::{registry as plugin_registry, Plugin},
    pretty::{annotate, patch_annotated, AnnotatedLine},
    tree::{flatten, get_node_at_path, get_node_at_path_mut, patch_flat, set_node_at_path, JNode, JKey, JPath, JScalar, PatchSpan},
};

pub const EDIT_TYPES: [&str; 6] = ["Object", "Array", "String", "Number", "Boolean", "Null"];

pub const SAVE_AS_FORMATS: [&str; 5] = ["JSON", "YAML", "TOML", "CSV", "JSONL"];
pub const SAVE_AS_EXTS:    [&str; 5] = ["json", "yaml", "toml", "csv", "jsonl"];

pub enum SaveAsPhase {
    FormatPick,
    RedactKeys(tui_textarea::TextArea<'static>),
    FilenameEdit(tui_textarea::TextArea<'static>),
}

pub struct SaveAsState {
    pub phase: SaveAsPhase,
    pub format_cursor: usize,
    pub redact_keys: Vec<String>, // parsed once RedactKeys is confirmed; empty = skip redaction
}

#[derive(PartialEq)]
pub enum EditMode {
    Edit,      // editing an existing node (v0.3)
    AddChild,  // adding a new child to a container
    Rename,    // renaming an existing key in an Object
    Wrap,      // wrapping the selected node in an Object or Array
}

pub enum EditPhase {
    TypeSelect,
    KeyEdit(tui_textarea::TextArea<'static>),   // Object child: key name input
    ValueEdit(tui_textarea::TextArea<'static>), // scalar value input
}

pub struct EditState {
    pub path: JPath,            // Edit: path of node; AddChild/AddSibling: path of parent container
    pub mode: EditMode,
    pub original: Option<JNode>, // Some for Edit (for potential cancel), None for AddChild
    pub phase: EditPhase,
    pub type_cursor: usize,     // 0-5, index into EDIT_TYPES
    pub pending_key: Option<String>, // key entered in KeyEdit, held until ValueEdit commits
    pub insert_after: Option<usize>, // Some(idx) → insert sibling after position idx in parent
}

pub enum PluginPhase {
    Menu,
    Prompt(tui_textarea::TextArea<'static>),
}

pub struct PluginState {
    pub phase: PluginPhase,
    pub menu_cursor: usize,
}

/// One undo/redo stack entry: the pre-edit tree snapshot, plus the target path the edit that
/// produced the *current* root touched — reused to patch (not fully rebuild) on undo/redo, since
/// the diff between this snapshot and the state it's paired with is confined to that subtree.
pub struct UndoEntry {
    pub target: JPath,
    pub root: JNode,
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
    pub save_dialog: bool,
    pub edit: Option<EditState>,
    pub clipboard: Option<JNode>,
    pub show_left: bool,
    pub show_preview: bool,
    pub explorer_fullscreen: bool,
    pub saved_show_left: bool,
    pub saved_show_preview: bool,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    // Search
    pub search_query: String,
    pub search_active: bool,
    pub search_matches: Vec<usize>, // sorted flat-row indices of matching rows
    pub search_match_cursor: usize, // which match is current
    pub pending_g: bool,            // true after first `g` press (waiting for gg)
    // Save-as
    pub save_as: Option<SaveAsState>,
    pub stdout_mode: bool,
    // Lint
    pub lint_warnings: Vec<LintWarning>,
    pub lint_cursor: usize,
    // Plugins
    pub plugin: Option<PluginState>,
    // Perf indicator — (row count, refresh duration in ms) from the most recent refresh_flat()
    pub last_refresh: Option<(usize, f64)>,
}

/// Status bar only shows the perf indicator once the document is large enough for the
/// timing to be meaningful — avoids visual noise for typical small/medium documents.
pub const PERF_INDICATOR_THRESHOLD: usize = 5_000;

impl App {
    pub fn new(file: Option<PathBuf>, stdin_content: Option<String>) -> Result<Self> {
        let stdout_mode = stdin_content.is_some();
        let (root, status) = if let Some(content) = stdin_content {
            let value = parse_any(&content, "json")?;
            (JNode::from_value(value), "stdin".to_string())
        } else if let Some(ref path) = file {
            let src = std::fs::read_to_string(path)?;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
            let value = parse_any(&src, ext)?;
            let node = JNode::from_value(value);
            let msg = format!("{}", path.display());
            (node, msg)
        } else {
            (JNode::Object { entries: indexmap::IndexMap::new(), collapsed: false }, "new file".to_string())
        };

        let t0 = std::time::Instant::now();
        let flat = flatten(&root);
        let annotated = annotate(&root);
        let lint_warnings = lint(&root);
        let last_refresh = Some((flat.len(), t0.elapsed().as_secs_f64() * 1000.0));
        Ok(Self {
            root, flat, annotated,
            cursor: 0, scroll: 0, left_scroll: 0,
            file, modified: false, status, quit: false, confirm_quit: false, save_dialog: false,
            edit: None, clipboard: None,
            show_left: true, show_preview: true,
            explorer_fullscreen: false, saved_show_left: true, saved_show_preview: true,
            undo_stack: Vec::new(), redo_stack: Vec::new(),
            search_query: String::new(), search_active: false,
            search_matches: Vec::new(), search_match_cursor: 0, pending_g: false,
            save_as: None,
            stdout_mode,
            lint_warnings, lint_cursor: 0,
            plugin: None,
            last_refresh,
        })
    }

    fn refresh_flat(&mut self) {
        let t0 = std::time::Instant::now();
        self.flat = flatten(&self.root);
        self.annotated = annotate(&self.root);
        self.lint_warnings = lint(&self.root);
        self.last_refresh = Some((self.flat.len(), t0.elapsed().as_secs_f64() * 1000.0));
        if self.cursor >= self.flat.len() {
            self.cursor = self.flat.len().saturating_sub(1);
        }
        if self.lint_cursor >= self.lint_warnings.len() {
            self.lint_cursor = self.lint_warnings.len().saturating_sub(1);
        }
    }

    /// Localized replacement for `refresh_flat()` used by every edit whose effect is confined to
    /// one subtree — see the call-site table in the lazy-flatten plan for which `target` each
    /// caller passes (its own path for a "self" patch, or its parent/container path for a
    /// "parent" patch — `patch_flat`/`patch_annotated` don't need to know or care which kind it
    /// is, only the caller does).
    ///
    /// `affects_annotated` should be `false` only for pure collapse-state toggles — `emit()`
    /// never checks `is_collapsed()`, so patching `annotated` for a collapse-only change would be
    /// wasted work producing byte-identical output.
    ///
    /// Returns the `PatchSpan` for `self.flat` (not `self.annotated`) so callers can bound a
    /// subsequent path search to the freshly patched range instead of scanning the whole
    /// document (see `set_cursor_to`). Returns `None` only if the defensive fallback path was
    /// taken (should never happen for a `target` that actually exists in the tree/flat).
    fn refresh_at(&mut self, target: &JPath, affects_annotated: bool) -> Option<PatchSpan> {
        let t0 = std::time::Instant::now();

        let flat_span = patch_flat(&mut self.flat, &self.root, target);
        let ann_ok = if affects_annotated {
            patch_annotated(&mut self.annotated, &self.root, target).is_some()
        } else {
            true
        };

        let result = if flat_span.is_some() && ann_ok {
            flat_span
        } else {
            debug_assert!(false, "refresh_at: patch miss for target {:?}", target);
            self.flat = flatten(&self.root);
            self.annotated = annotate(&self.root);
            None
        };

        if !patch_lint(&mut self.lint_warnings, &self.root, target) {
            // Rare fallback: a new warning appeared in a subtree that previously had none (see
            // lint::patch_lint's doc comment) — only case patch_lint can't handle incrementally.
            self.lint_warnings = lint(&self.root);
        }
        self.last_refresh = Some((self.flat.len(), t0.elapsed().as_secs_f64() * 1000.0));

        if self.cursor >= self.flat.len() {
            self.cursor = self.flat.len().saturating_sub(1);
        }
        if self.lint_cursor >= self.lint_warnings.len() {
            self.lint_cursor = self.lint_warnings.len().saturating_sub(1);
        }
        result
    }

    /// Bounds a path search to the freshly patched span instead of scanning the whole document —
    /// replaces the `self.flat.iter().position(|r| r.path == np)` scans that used to run after
    /// every structural edit.
    fn set_cursor_to(&mut self, path: &JPath, hint: Option<PatchSpan>) {
        let range = match hint {
            Some(span) => span.start..span.new_end(),
            None => 0..self.flat.len(),
        };
        if let Some(rows) = self.flat.get(range.clone()) {
            if let Some(pos) = rows.iter().position(|r| r.path == *path) {
                self.cursor = range.start + pos;
            }
        }
    }

    fn current_path(&self) -> JPath {
        self.flat.get(self.cursor).map(|r| r.path.clone()).unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;

        if self.save_dialog {
            self.handle_save_dialog(key);
            return;
        }

        if self.save_as.is_some() {
            self.handle_key_save_as(key);
            return;
        }

        if self.edit.is_some() {
            self.handle_key_edit(key);
            return;
        }

        if self.plugin.is_some() {
            self.handle_key_plugin(key);
            return;
        }

        if self.search_active {
            self.handle_key_search(key);
            return;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, Char('c')) => {
                self.quit = true;
            }
            (KeyModifiers::NONE, Char('q')) => {
                if self.modified {
                    self.save_dialog = true;
                    self.confirm_quit = false;
                } else if self.confirm_quit {
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
            (KeyModifiers::NONE, Char('r')) => { self.confirm_quit = false; self.start_rename(); }
            (KeyModifiers::NONE, Char('a')) => { self.confirm_quit = false; self.start_add_child(); }
            (KeyModifiers::NONE, Char('d')) => { self.confirm_quit = false; self.delete_node(); }
            (KeyModifiers::SHIFT, Char('D')) | (KeyModifiers::NONE, Char('D')) => { self.confirm_quit = false; self.duplicate_node(); }
            (KeyModifiers::NONE, Char('y')) => { self.confirm_quit = false; self.copy_node(); }
            (KeyModifiers::NONE, Char('p')) => { self.confirm_quit = false; self.paste_node(true); }
            (KeyModifiers::SHIFT, Char('P')) | (KeyModifiers::NONE, Char('P')) => { self.confirm_quit = false; self.paste_node(false); }
            (KeyModifiers::SHIFT, Char('K')) | (KeyModifiers::NONE, Char('K')) => { self.confirm_quit = false; self.move_node(true); }
            (KeyModifiers::SHIFT, Char('J')) | (KeyModifiers::NONE, Char('J')) => { self.confirm_quit = false; self.move_node(false); }
            (KeyModifiers::NONE, Char('w')) => { self.confirm_quit = false; self.pending_g = false; self.start_wrap(); }
            (KeyModifiers::NONE, Char('u')) => { self.confirm_quit = false; self.undo(); }
            (KeyModifiers::CONTROL, Char('r')) => { self.confirm_quit = false; self.redo(); }
            (KeyModifiers::SHIFT, Char('S')) | (KeyModifiers::NONE, Char('S')) => { self.confirm_quit = false; self.sort_children(); }
            (KeyModifiers::SHIFT, Char('E')) | (KeyModifiers::NONE, Char('E')) => { self.confirm_quit = false; self.expand_all(); }
            (KeyModifiers::SHIFT, Char('C')) | (KeyModifiers::NONE, Char('C')) => { self.confirm_quit = false; self.collapse_all(); }
            (KeyModifiers::NONE, Char('s')) => { self.confirm_quit = false; self.save_file(); }
            (KeyModifiers::SHIFT, Char('W')) | (KeyModifiers::NONE, Char('W')) => { self.confirm_quit = false; self.pending_g = false; self.start_save_as(); }
            (KeyModifiers::NONE, Char('[')) => { self.confirm_quit = false; self.pending_g = false; self.explorer_fullscreen = false; self.show_left = !self.show_left; }
            (KeyModifiers::NONE, Char(']')) => { self.confirm_quit = false; self.pending_g = false; self.explorer_fullscreen = false; self.show_preview = !self.show_preview; }
            (KeyModifiers::CONTROL, Char('e')) => { self.confirm_quit = false; self.pending_g = false; self.toggle_explorer_fullscreen(); }
            (KeyModifiers::NONE, Tab) => {
                self.confirm_quit = false; self.pending_g = false;
                self.lint_next();
            }
            (KeyModifiers::SHIFT, BackTab) | (KeyModifiers::NONE, BackTab) => {
                self.confirm_quit = false; self.pending_g = false;
                self.lint_prev();
            }
            (KeyModifiers::SHIFT, Char('|')) | (KeyModifiers::NONE, Char('|')) => {
                self.confirm_quit = false; self.pending_g = false;
                self.start_plugin_menu();
            }
            (KeyModifiers::NONE, Char('/')) => {
                self.confirm_quit = false; self.pending_g = false;
                self.search_active = true;
                self.search_query.clear();
                self.search_matches.clear();
                self.search_match_cursor = 0;
            }
            (KeyModifiers::NONE, Char('n')) => {
                self.confirm_quit = false; self.pending_g = false;
                self.search_next();
            }
            (KeyModifiers::SHIFT, Char('N')) | (KeyModifiers::NONE, Char('N')) => {
                self.confirm_quit = false; self.pending_g = false;
                self.search_prev();
            }
            (KeyModifiers::NONE, Char('g')) => {
                self.confirm_quit = false;
                if self.pending_g {
                    self.pending_g = false;
                    self.cursor = 0;
                } else {
                    self.pending_g = true;
                }
            }
            (KeyModifiers::SHIFT, Char('G')) | (KeyModifiers::NONE, Char('G')) => {
                self.confirm_quit = false; self.pending_g = false;
                self.cursor = self.flat.len().saturating_sub(1);
            }
            (KeyModifiers::NONE, Esc) => {
                self.confirm_quit = false; self.pending_g = false;
                self.search_query.clear();
                self.search_matches.clear();
                self.search_match_cursor = 0;
                self.status = self.file.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "new file".to_string());
            }
            _ => { self.confirm_quit = false; self.pending_g = false; }
        }
    }

    fn handle_save_dialog(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;
        match key.code {
            Char('s') => {
                self.save_file();
                self.quit = true;
            }
            Char('n') | Char('q') => {
                self.quit = true;
            }
            Esc => {
                self.save_dialog = false;
            }
            _ => {}
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
                        let min = if matches!(s.mode, EditMode::Edit) { 2 } else { 0 };
                        s.type_cursor = s.type_cursor.saturating_sub(1).max(min);
                    }
                }
                Down => {
                    if let Some(s) = self.edit.as_mut() {
                        let max = if matches!(s.mode, EditMode::Wrap) { 1 } else { 5 };
                        s.type_cursor = (s.type_cursor + 1).min(max);
                    }
                }
                Enter | Tab => self.confirm_type(),
                _ => {}
            },
            1 => match key.code {
                Esc => {
                    let is_rename = self.edit.as_ref()
                        .map(|s| matches!(s.mode, EditMode::Rename))
                        .unwrap_or(false);
                    if is_rename {
                        self.edit = None;
                    } else if let Some(s) = self.edit.as_mut() {
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
        let path = row.path.clone();
        let Some(node) = get_node_at_path(&self.root, &path) else { return };

        if !matches!(node, JNode::Scalar(_)) {
            self.status = "use 'a' to add children · 'd' to delete · 'D' to duplicate".to_string();
            return;
        }

        let type_cursor = match node {
            JNode::Scalar(JScalar::String(_))  => 2,
            JNode::Scalar(JScalar::Number(_))  => 3,
            JNode::Scalar(JScalar::Bool(_))    => 4,
            JNode::Scalar(JScalar::Null)       => 5,
            _ => 2,
        };
        let original = node.clone();

        self.edit = Some(EditState {
            path,
            mode: EditMode::Edit,
            original: Some(original),
            phase: EditPhase::TypeSelect,
            type_cursor,
            pending_key: None,
            insert_after: None,
        });
    }

    fn start_add_child(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        let path = row.path.clone();
        let Some(node) = get_node_at_path(&self.root, &path) else { return };

        match node {
            JNode::Object { .. } | JNode::Array { .. } => {
                let is_collapsed = node.is_collapsed();
                if is_collapsed {
                    toggle_node_collapse(&mut self.root, &path);
                    let span = self.refresh_at(&path, false);
                    self.set_cursor_to(&path, span);
                }
                self.edit = Some(EditState {
                    path,
                    mode: EditMode::AddChild,
                    original: None,
                    phase: EditPhase::TypeSelect,
                    type_cursor: 2,
                    pending_key: None,
                    insert_after: None,
                });
            }
            JNode::Scalar(_) => {
                let current_path = path;
                if current_path.is_empty() {
                    self.status = "cannot add sibling to root".to_string();
                    return;
                }
                let parent_path: JPath = current_path[..current_path.len() - 1].to_vec();
                let after_idx = match current_path.last().unwrap() {
                    JKey::Index(i) => *i,
                    JKey::Field(k) => match get_node_at_path(&self.root, &parent_path) {
                        Some(JNode::Object { entries, .. }) => {
                            entries.get_index_of(k.as_ref()).unwrap_or(0)
                        }
                        _ => 0,
                    },
                };
                self.edit = Some(EditState {
                    path: parent_path,
                    mode: EditMode::AddChild,
                    original: None,
                    phase: EditPhase::TypeSelect,
                    type_cursor: 2,
                    pending_key: None,
                    insert_after: Some(after_idx),
                });
            }
        }
    }

    fn confirm_type(&mut self) {
        let Some(state) = self.edit.take() else { return };
        match state.mode {
            EditMode::Edit     => self.confirm_type_edit(state),
            EditMode::AddChild => self.confirm_type_add(state),
            EditMode::Rename   => {} // TypeSelect is not reachable from Rename
            EditMode::Wrap     => self.confirm_type_wrap(state),
        }
    }

    fn confirm_type_wrap(&mut self, state: EditState) {
        match state.type_cursor {
            0 => {
                // Object: need a key name
                let ta = tui_textarea::TextArea::new(vec![String::new()]);
                self.show_preview = true;
                self.edit = Some(EditState { phase: EditPhase::KeyEdit(ta), ..state });
            }
            _ => {
                // Array: wrap immediately
                self.do_wrap(state.path, None);
            }
        }
    }

    fn confirm_type_edit(&mut self, state: EditState) {
        let original = state.original.as_ref().cloned().unwrap_or(JNode::Scalar(JScalar::Null));
        match state.type_cursor {
            5 => {
                set_node_at_path(&mut self.root, &state.path, JNode::Scalar(JScalar::Null));
                self.refresh_at(&state.path, true);
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
            let after = state.insert_after;
            macro_rules! do_insert {
                ($node:expr) => {
                    match after {
                        Some(idx) => self.insert_sibling(&path, None, $node, idx),
                        None      => self.insert_child(&path, None, $node),
                    }
                };
            }
            match tc {
                0 => do_insert!(JNode::Object { entries: indexmap::IndexMap::new(), collapsed: false }),
                1 => do_insert!(JNode::Array { items: Vec::new(), collapsed: false }),
                5 => do_insert!(JNode::Scalar(JScalar::Null)),
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

        if matches!(state.mode, EditMode::Rename) {
            self.do_rename(state, key);
            return;
        }

        if matches!(state.mode, EditMode::Wrap) {
            self.do_wrap(state.path, Some(key));
            return;
        }

        let path = state.path.clone();
        let tc = state.type_cursor;

        let after = state.insert_after;
        macro_rules! do_insert {
            ($node:expr) => {
                match after {
                    Some(idx) => self.insert_sibling(&path, Some(&key), $node, idx),
                    None      => self.insert_child(&path, Some(&key), $node),
                }
            };
        }
        match tc {
            0 => do_insert!(JNode::Object { entries: indexmap::IndexMap::new(), collapsed: false }),
            1 => do_insert!(JNode::Array { items: Vec::new(), collapsed: false }),
            5 => do_insert!(JNode::Scalar(JScalar::Null)),
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
                    insert_after: after,
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
                self.push_undo(&state.path);
                set_node_at_path(&mut self.root, &state.path, new_node);
                self.refresh_at(&state.path, true);
                self.modified = true;
            }
            EditMode::AddChild => {
                let path = state.path.clone();
                let key = state.pending_key.as_deref().map(|s| s.to_string());
                match state.insert_after {
                    Some(idx) => self.insert_sibling(&path, key.as_deref(), new_node, idx),
                    None      => self.insert_child(&path, key.as_deref(), new_node),
                }
            }
            EditMode::Rename => {} // ValueEdit is not reachable from Rename
            EditMode::Wrap   => {} // ValueEdit is not reachable from Wrap
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

        self.push_undo(&parent_path);
        if let Some(parent) = get_node_at_path_mut(&mut self.root, &parent_path) {
            match (parent, &last_key) {
                (JNode::Object { entries, .. }, JKey::Field(k)) => {
                    entries.shift_remove(k.as_ref());
                }
                (JNode::Array { items, .. }, JKey::Index(i)) => {
                    items.remove(*i);
                }
                _ => { self.undo_stack.pop(); return; }
            }
        }

        self.refresh_at(&parent_path, true);
        self.modified = true;
    }

    fn duplicate_node(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        if row.path.is_empty() {
            return;
        }

        let path = row.path.clone();
        let Some(node_clone) = get_node_at_path(&self.root, &path).cloned() else { return };
        let parent_path = path[..path.len() - 1].to_vec();
        let last_key = path.last().unwrap().clone();
        let mut new_path: Option<JPath> = None;

        self.push_undo(&parent_path);
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
                    let idx = entries.get_index_of(k.as_ref()).unwrap_or(entries.len() - 1);
                    let new_key = unique_key(entries, k);
                    entries.shift_insert(idx + 1, new_key.clone(), node_clone);
                    let mut p = parent_path.clone();
                    p.push(JKey::Field(Rc::from(new_key.as_str())));
                    new_path = Some(p);
                }
                _ => {}
            }
        }

        let span = self.refresh_at(&parent_path, true);
        self.modified = true;

        if let Some(np) = new_path {
            self.set_cursor_to(&np, span);
        }
    }

    fn copy_node(&mut self) {
        if let Some(row) = self.flat.get(self.cursor) {
            let label = match (&row.key, row.index) {
                (Some(k), _) => k.clone(),
                (None, Some(i)) => format!("Item[{}]", i),
                (None, None) => "<root>".to_string(),
            };
            let path = row.path.clone();
            if let Some(node) = get_node_at_path(&self.root, &path) {
                self.clipboard = Some(node.clone());
            }
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

        self.push_undo(&parent_path);
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
                    let idx = entries.get_index_of(k.as_ref()).unwrap_or(0);
                    let base = format!("{}_paste", k);
                    let new_key = unique_key(entries, &base);
                    let insert_at = if after { idx + 1 } else { idx };
                    entries.shift_insert(insert_at, new_key.clone(), node);
                    let mut p = parent_path.clone();
                    p.push(JKey::Field(Rc::from(new_key.as_str())));
                    new_path = Some(p);
                }
                _ => {}
            }
        }

        let span = self.refresh_at(&parent_path, true);
        self.modified = true;

        if let Some(np) = new_path {
            self.set_cursor_to(&np, span);
        }
    }

    fn move_node(&mut self, up: bool) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        if row.path.is_empty() { return; }

        let parent_path = row.path[..row.path.len() - 1].to_vec();
        let last_key = row.path.last().unwrap().clone();
        let mut new_last_key: Option<JKey> = None;

        // push_undo before the mutable borrow; pop it back if no move actually happened
        self.push_undo(&parent_path);

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
                    if let Some(idx) = entries.get_index_of(k.as_ref()) {
                        if up && idx > 0 {
                            entries.swap_indices(idx, idx - 1);
                            new_last_key = Some(last_key.clone());
                        } else if !up && idx + 1 < entries.len() {
                            entries.swap_indices(idx, idx + 1);
                            new_last_key = Some(last_key.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        if new_last_key.is_none() { self.undo_stack.pop(); return; }

        let span = self.refresh_at(&parent_path, true);
        self.modified = true;

        let mut new_path = parent_path;
        new_path.push(new_last_key.unwrap());
        self.set_cursor_to(&new_path, span);
    }

    fn insert_child(&mut self, container_path: &JPath, key: Option<&str>, new_node: JNode) {
        let mut new_path: Option<JPath> = None;

        self.push_undo(container_path);
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
                        p.push(JKey::Field(Rc::from(k)));
                        new_path = Some(p);
                    }
                }
                _ => {}
            }
        }

        let span = self.refresh_at(container_path, true);
        self.modified = true;

        if let Some(np) = new_path {
            self.set_cursor_to(&np, span);
        }
    }

    fn insert_sibling(&mut self, parent_path: &JPath, key: Option<&str>, new_node: JNode, after: usize) {
        let mut new_path: Option<JPath> = None;
        let insert_idx = after + 1;

        self.push_undo(parent_path);
        if let Some(container) = get_node_at_path_mut(&mut self.root, parent_path) {
            match container {
                JNode::Array { items, .. } => {
                    let idx = insert_idx.min(items.len());
                    items.insert(idx, new_node);
                    let mut p = parent_path.to_vec();
                    p.push(JKey::Index(idx));
                    new_path = Some(p);
                }
                JNode::Object { entries, .. } => {
                    if let Some(k) = key {
                        let idx = insert_idx.min(entries.len());
                        entries.shift_insert(idx, k.to_string(), new_node);
                        let mut p = parent_path.to_vec();
                        p.push(JKey::Field(Rc::from(k)));
                        new_path = Some(p);
                    }
                }
                _ => {}
            }
        }

        let span = self.refresh_at(parent_path, true);
        self.modified = true;

        if let Some(np) = new_path {
            self.set_cursor_to(&np, span);
        }
    }

    fn save_file(&mut self) {
        if self.stdout_mode {
            let value = self.root.to_value();
            match crate::convert::serialize_to(&value, "json") {
                Ok(json) => {
                    use std::io::Write;
                    print!("{}", json);
                    let _ = std::io::stdout().flush();
                    self.modified = false;
                    self.quit = true;
                }
                Err(e) => self.status = format!("serialize error: {e}"),
            }
            return;
        }

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

    fn start_save_as(&mut self) {
        self.save_as = Some(SaveAsState {
            phase: SaveAsPhase::FormatPick,
            format_cursor: 0,
            redact_keys: Vec::new(),
        });
    }

    fn handle_key_save_as(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;

        let phase_kind = match self.save_as.as_ref().map(|s| &s.phase) {
            Some(SaveAsPhase::FormatPick)      => 0,
            Some(SaveAsPhase::RedactKeys(_))   => 1,
            Some(SaveAsPhase::FilenameEdit(_)) => 2,
            None => return,
        };

        match phase_kind {
            0 => match key.code {
                Esc => { self.save_as = None; }
                Up   => { if let Some(s) = self.save_as.as_mut() { s.format_cursor = s.format_cursor.saturating_sub(1); } }
                Down => { if let Some(s) = self.save_as.as_mut() { s.format_cursor = (s.format_cursor + 1).min(SAVE_AS_FORMATS.len() - 1); } }
                Enter => self.confirm_save_as_format(),
                _ => {}
            },
            1 => match key.code {
                Esc => {
                    if let Some(s) = self.save_as.as_mut() {
                        s.phase = SaveAsPhase::FormatPick;
                    }
                }
                Enter => self.confirm_save_as_redact(),
                _ => {
                    if let Some(s) = self.save_as.as_mut() {
                        if let SaveAsPhase::RedactKeys(ta) = &mut s.phase {
                            ta.input(key);
                        }
                    }
                }
            },
            _ => match key.code {
                Esc => {
                    if let Some(s) = self.save_as.as_mut() {
                        s.phase = SaveAsPhase::FormatPick;
                    }
                }
                Enter => self.do_save_as(),
                _ => {
                    if let Some(s) = self.save_as.as_mut() {
                        if let SaveAsPhase::FilenameEdit(ta) = &mut s.phase {
                            ta.input(key);
                        }
                    }
                }
            },
        }
    }

    fn confirm_save_as_format(&mut self) {
        let cursor = match self.save_as.as_ref() {
            Some(s) => s.format_cursor,
            None => return,
        };
        let ext = SAVE_AS_EXTS[cursor];

        if ext == "toml" && has_null(&self.root) {
            self.status = "warning: document contains null values — TOML does not support null".to_string();
        }

        let ta = tui_textarea::TextArea::new(vec![String::new()]);
        self.show_preview = true;

        if let Some(s) = self.save_as.as_mut() {
            s.phase = SaveAsPhase::RedactKeys(ta);
        }
    }

    fn confirm_save_as_redact(&mut self) {
        let keys: Vec<String> = match self.save_as.as_ref().map(|s| &s.phase) {
            Some(SaveAsPhase::RedactKeys(ta)) => ta
                .lines()
                .first()
                .cloned()
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => return,
        };

        let cursor = match self.save_as.as_ref() {
            Some(s) => s.format_cursor,
            None => return,
        };
        let ext = SAVE_AS_EXTS[cursor];
        let default_name = save_as_default_filename(self.file.as_ref(), ext);
        let mut ta = tui_textarea::TextArea::new(vec![default_name]);
        ta.move_cursor(tui_textarea::CursorMove::End);

        if let Some(s) = self.save_as.as_mut() {
            s.redact_keys = keys;
            s.phase = SaveAsPhase::FilenameEdit(ta);
        }
    }

    fn do_save_as(&mut self) {
        let (cursor, filename, redact_keys) = match self.save_as.as_ref() {
            Some(s) => match &s.phase {
                SaveAsPhase::FilenameEdit(ta) => {
                    let fname = ta.lines().first().cloned().unwrap_or_default().trim().to_string();
                    (s.format_cursor, fname, s.redact_keys.clone())
                }
                _ => return,
            },
            None => return,
        };

        if filename.is_empty() {
            self.status = "filename cannot be empty".to_string();
            return;
        }

        let fmt = SAVE_AS_EXTS[cursor];
        let path = std::path::PathBuf::from(&filename);
        let value = self.root.to_value();
        let value = crate::redact::redact(&value, &redact_keys);

        match crate::convert::serialize_to(&value, fmt) {
            Ok(content) => match std::fs::write(&path, &content) {
                Ok(()) => {
                    self.save_as = None;
                    self.status = format!("saved as {}", filename);
                }
                Err(e) => { self.status = format!("write error: {e}"); }
            },
            Err(e) => { self.status = format!("conversion error: {e}"); }
        }
    }

    fn toggle_collapse(&mut self) {
        let path = self.current_path();
        toggle_node_collapse(&mut self.root, &path);
        self.refresh_at(&path, false);
    }

    fn push_undo(&mut self, target: &JPath) {
        if self.undo_stack.len() >= 50 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(UndoEntry { target: target.clone(), root: self.root.clone() });
        self.redo_stack.clear();
    }

    fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            // Timed from here, not just inside refresh_at: the redo-stack snapshot clone below
            // is a real, currently-unoptimized O(N) cost (full JNode::clone(), pre-dating this
            // session's patch work) that refresh_at's own internal timer can't see — without
            // this, last_refresh would understate undo's true cost on large documents.
            let t0 = std::time::Instant::now();
            self.redo_stack.push(UndoEntry { target: entry.target.clone(), root: self.root.clone() });
            self.root = entry.root;
            self.refresh_at(&entry.target, true);
            self.last_refresh = Some((self.flat.len(), t0.elapsed().as_secs_f64() * 1000.0));
            self.modified = true;
            self.status = format!("undo · {} left", self.undo_stack.len());
        } else {
            self.status = "nothing to undo".to_string();
        }
    }

    fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            let t0 = std::time::Instant::now(); // see undo()'s comment on why this wraps the clone too
            self.undo_stack.push(UndoEntry { target: entry.target.clone(), root: self.root.clone() });
            self.root = entry.root;
            self.refresh_at(&entry.target, true);
            self.last_refresh = Some((self.flat.len(), t0.elapsed().as_secs_f64() * 1000.0));
            self.modified = true;
            self.status = format!("redo · {} forward", self.redo_stack.len());
        } else {
            self.status = "nothing to redo".to_string();
        }
    }

    fn handle_key_search(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, Esc) => {
                self.search_active = false;
                self.search_query.clear();
                self.search_matches.clear();
                self.search_match_cursor = 0;
            }
            (KeyModifiers::NONE, Enter) => {
                self.search_active = false;
                if !self.search_matches.is_empty() {
                    self.cursor = self.search_matches[self.search_match_cursor];
                }
            }
            (KeyModifiers::NONE, Backspace) => {
                self.search_query.pop();
                self.update_search_matches();
            }
            (_, Char(c)) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                self.search_query.push(c);
                self.update_search_matches();
            }
            _ => {}
        }
    }

    fn update_search_matches(&mut self) {
        let q = self.search_query.to_lowercase();
        if q.is_empty() {
            self.search_matches.clear();
            self.search_match_cursor = 0;
            return;
        }
        self.search_matches = self.flat.iter().enumerate()
            .filter(|(_, row)| {
                let key_match = row.key.as_deref()
                    .map(|k| k.to_lowercase().contains(&q))
                    .unwrap_or(false)
                    || row.index.map(|i| i.to_string().contains(&q)).unwrap_or(false);
                let val_match = match get_node_at_path(&self.root, &row.path) {
                    Some(JNode::Scalar(s)) => match s {
                        JScalar::String(v) => v.to_lowercase().contains(&q),
                        JScalar::Number(v) => v.contains(&q),
                        JScalar::Bool(b) => b.to_string().contains(&q),
                        JScalar::Null => "null".contains(&q),
                    },
                    _ => false,
                };
                key_match || val_match
            })
            .map(|(i, _)| i)
            .collect();
        self.search_match_cursor = 0;
        if let Some(&first) = self.search_matches.first() {
            self.cursor = first;
        }
    }

    fn search_next(&mut self) {
        if self.search_matches.is_empty() { return; }
        self.search_match_cursor = (self.search_match_cursor + 1) % self.search_matches.len();
        self.cursor = self.search_matches[self.search_match_cursor];
    }

    fn search_prev(&mut self) {
        if self.search_matches.is_empty() { return; }
        self.search_match_cursor = if self.search_match_cursor == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_match_cursor - 1
        };
        self.cursor = self.search_matches[self.search_match_cursor];
    }

    fn start_plugin_menu(&mut self) {
        self.plugin = Some(PluginState { phase: PluginPhase::Menu, menu_cursor: 0 });
    }

    fn handle_key_plugin(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;

        let phase_kind = match self.plugin.as_ref().map(|s| &s.phase) {
            Some(PluginPhase::Menu)      => 0,
            Some(PluginPhase::Prompt(_)) => 1,
            None => return,
        };

        let plugins = plugin_registry();

        match phase_kind {
            0 => match key.code {
                Esc => { self.plugin = None; }
                Up => {
                    if let Some(s) = self.plugin.as_mut() {
                        s.menu_cursor = s.menu_cursor.saturating_sub(1);
                    }
                }
                Down => {
                    if let Some(s) = self.plugin.as_mut() {
                        s.menu_cursor = (s.menu_cursor + 1).min(plugins.len().saturating_sub(1));
                    }
                }
                Enter => self.confirm_plugin_select(&plugins),
                _ => {}
            },
            _ => match key.code {
                Esc => {
                    if let Some(s) = self.plugin.as_mut() {
                        s.phase = PluginPhase::Menu;
                    }
                }
                Enter => self.run_plugin(&plugins),
                _ => {
                    if let Some(s) = self.plugin.as_mut() {
                        if let PluginPhase::Prompt(ta) = &mut s.phase {
                            ta.input(key);
                        }
                    }
                }
            },
        }
    }

    fn confirm_plugin_select(&mut self, plugins: &[Box<dyn Plugin>]) {
        let Some(cursor) = self.plugin.as_ref().map(|s| s.menu_cursor) else { return };
        if plugins.get(cursor).is_none() { return; }

        let ta = tui_textarea::TextArea::new(vec![String::new()]);
        self.show_preview = true;
        if let Some(s) = self.plugin.as_mut() {
            s.phase = PluginPhase::Prompt(ta);
        }
    }

    fn run_plugin(&mut self, plugins: &[Box<dyn Plugin>]) {
        let Some(state) = self.plugin.as_ref() else { return };
        let PluginPhase::Prompt(ref ta) = state.phase else { return };
        let arg = ta.lines().first().cloned().unwrap_or_default();
        let cursor = state.menu_cursor;

        if arg.trim().is_empty() {
            self.status = "expression cannot be empty".to_string();
            return;
        }

        let Some(plugin) = plugins.get(cursor) else { return };
        let path = self.current_path();
        let Some(node) = get_node_at_path(&self.root, &path) else { return };

        match plugin.run(node, arg.trim()) {
            Ok(new_node) => {
                self.push_undo(&path);
                set_node_at_path(&mut self.root, &path, new_node);
                self.refresh_at(&path, true);
                self.modified = true;
                self.plugin = None;
                self.status = format!("{} applied", plugin.name());
            }
            Err(e) => {
                self.status = format!("{}: {e}", plugin.name());
            }
        }
    }

    fn start_wrap(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        let path = row.path.clone();
        self.edit = Some(EditState {
            path,
            mode: EditMode::Wrap,
            original: None,
            phase: EditPhase::TypeSelect,
            type_cursor: 1, // default to Array
            pending_key: None,
            insert_after: None,
        });
    }

    fn do_wrap(&mut self, path: JPath, key_opt: Option<String>) {
        let original = match get_node_at_path(&self.root, &path) {
            Some(n) => n.clone(),
            None => return,
        };

        // Wrap-in-Object on a named field: rename-and-nest.
        // new_key becomes the outer key; original_key becomes the inner key.
        // e.g.  url: "v"  +  new_key="mainurl"  →  mainurl: { url: "v" }
        if let (Some(new_key), Some(JKey::Field(original_key))) = (&key_opt, path.last()) {
            let original_key = original_key.to_string();
            let new_key = new_key.clone();
            let parent_path = path[..path.len() - 1].to_vec();

            self.push_undo(&parent_path);
            if let Some(JNode::Object { entries, .. }) = get_node_at_path_mut(&mut self.root, &parent_path) {
                if let Some(pos) = entries.get_index_of(original_key.as_str()) {
                    let mut inner = indexmap::IndexMap::new();
                    inner.insert(original_key, original);
                    let wrapper = JNode::Object { entries: inner, collapsed: false };
                    entries.shift_remove_index(pos);
                    entries.shift_insert(pos, new_key.clone(), wrapper);
                }
            }
            let span = self.refresh_at(&parent_path, true);
            self.modified = true;
            let mut new_path = parent_path;
            new_path.push(JKey::Field(Rc::from(new_key.as_str())));
            self.set_cursor_to(&new_path, span);
            return;
        }

        // Default: wrap the value in place (Array items, or wrap-in-Array for any node).
        self.push_undo(&path);
        let wrapped = match key_opt {
            None => JNode::Array { items: vec![original], collapsed: false },
            Some(k) => {
                let mut entries = indexmap::IndexMap::new();
                entries.insert(k, original);
                JNode::Object { entries, collapsed: false }
            }
        };
        set_node_at_path(&mut self.root, &path, wrapped);
        let span = self.refresh_at(&path, true);
        self.modified = true;
        self.set_cursor_to(&path, span);
    }

    fn start_rename(&mut self) {
        let Some(row) = self.flat.get(self.cursor) else { return };
        let current_key = match row.path.last() {
            Some(JKey::Field(k)) => k.to_string(),
            Some(JKey::Index(_)) => {
                self.status = "array items have no key to rename".to_string();
                return;
            }
            None => {
                self.status = "cannot rename root".to_string();
                return;
            }
        };

        let mut ta = tui_textarea::TextArea::new(vec![current_key]);
        ta.move_cursor(tui_textarea::CursorMove::End);
        self.show_preview = true;
        self.edit = Some(EditState {
            path: row.path.clone(),
            mode: EditMode::Rename,
            original: None,
            phase: EditPhase::KeyEdit(ta),
            type_cursor: 0,
            pending_key: None,
            insert_after: None,
        });
    }

    fn do_rename(&mut self, state: EditState, new_key: String) {
        if state.path.is_empty() { return; }
        let parent_path = state.path[..state.path.len() - 1].to_vec();
        let old_key = match state.path.last() {
            Some(JKey::Field(k)) => k.to_string(),
            _ => return,
        };

        if new_key == old_key { return; }

        // Immutable check: validity and index (borrow released before mutations below)
        let (key_exists, old_idx) = match get_node_at_path(&self.root, &parent_path) {
            Some(JNode::Object { entries, .. }) => {
                (entries.contains_key(&new_key), entries.get_index_of(old_key.as_str()))
            }
            _ => return,
        };

        if key_exists {
            self.status = format!("key '{}' already exists", new_key);
            return;
        }
        let Some(idx) = old_idx else { return; };

        self.push_undo(&parent_path);
        if let Some(JNode::Object { entries, .. }) = get_node_at_path_mut(&mut self.root, &parent_path) {
            let Some((_, val)) = entries.shift_remove_index(idx) else { return; };
            entries.shift_insert(idx, new_key.clone(), val);
        }
        let span = self.refresh_at(&parent_path, true);
        self.modified = true;
        let mut new_path = parent_path;
        new_path.push(JKey::Field(Rc::from(new_key.as_str())));
        self.set_cursor_to(&new_path, span);
    }

    fn sort_children(&mut self) {
        let path = self.current_path();
        if !matches!(get_node_at_path(&self.root, &path), Some(JNode::Object { .. })) {
            self.status = "select an Object to sort its keys".to_string();
            return;
        }
        self.push_undo(&path);
        if let Some(JNode::Object { entries, .. }) = get_node_at_path_mut(&mut self.root, &path) {
            entries.sort_keys();
        }
        self.refresh_at(&path, true);
        self.modified = true;
    }

    fn expand_all(&mut self) {
        let path = self.current_path();
        if let Some(node) = get_node_at_path_mut(&mut self.root, &path) {
            set_all_collapsed(node, false);
        }
        self.refresh_at(&path, false);
    }

    fn collapse_all(&mut self) {
        let path = self.current_path();
        if let Some(node) = get_node_at_path_mut(&mut self.root, &path) {
            set_all_collapsed(node, true);
        }
        self.refresh_at(&path, false);
    }

    fn lint_next(&mut self) {
        if self.lint_warnings.is_empty() { return; }
        self.lint_cursor = (self.lint_cursor + 1) % self.lint_warnings.len();
        self.jump_to_lint(self.lint_cursor);
    }

    fn lint_prev(&mut self) {
        if self.lint_warnings.is_empty() { return; }
        self.lint_cursor = if self.lint_cursor == 0 {
            self.lint_warnings.len() - 1
        } else {
            self.lint_cursor - 1
        };
        self.jump_to_lint(self.lint_cursor);
    }

    fn jump_to_lint(&mut self, idx: usize) {
        let path = match self.lint_warnings.get(idx) {
            Some(w) => w.path.clone(),
            None => return,
        };
        self.expand_ancestors(&path.clone());
        self.refresh_flat();
        if let Some(pos) = self.flat.iter().position(|r| r.path == path) {
            self.cursor = pos;
        }
    }

    fn expand_ancestors(&mut self, path: &JPath) {
        for depth in 0..path.len() {
            let prefix = &path[..depth];
            if let Some(node) = get_node_at_path_mut(&mut self.root, prefix) {
                match node {
                    JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => {
                        *collapsed = false;
                    }
                    _ => {}
                }
            }
        }
    }

    fn toggle_explorer_fullscreen(&mut self) {
        if self.explorer_fullscreen {
            self.show_left    = self.saved_show_left;
            self.show_preview = self.saved_show_preview;
            self.explorer_fullscreen = false;
        } else {
            self.saved_show_left    = self.show_left;
            self.saved_show_preview = self.show_preview;
            self.show_left    = false;
            self.show_preview = false;
            self.explorer_fullscreen = true;
        }
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

fn has_null(node: &JNode) -> bool {
    match node {
        JNode::Scalar(JScalar::Null)      => true,
        JNode::Object { entries, .. }     => entries.values().any(has_null),
        JNode::Array  { items, .. }       => items.iter().any(has_null),
        _                                 => false,
    }
}

fn save_as_default_filename(file: Option<&PathBuf>, ext: &str) -> String {
    if let Some(path) = file {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        format!("{}.{}", stem, ext)
    } else {
        format!("output.{}", ext)
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

fn set_all_collapsed(node: &mut JNode, collapsed: bool) {
    match node {
        JNode::Object { entries, collapsed: c } => {
            *c = collapsed;
            for v in entries.values_mut() {
                set_all_collapsed(v, collapsed);
            }
        }
        JNode::Array { items, collapsed: c } => {
            *c = collapsed;
            for item in items.iter_mut() {
                set_all_collapsed(item, collapsed);
            }
        }
        _ => {}
    }
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
                if let Some(child) = entries.get_mut(k.as_ref()) {
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

fn tui_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        let area = terminal.size()?;
        let h = area.height as usize;
        let content_h = h.saturating_sub(2);

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
                    app.left_scroll = f0;
                } else if block_h <= left_inner_h && l0 >= app.left_scroll + left_inner_h {
                    app.left_scroll = l0 + 1 - left_inner_h;
                }
            }
        }

        terminal.draw(|f| crate::ui::render(f, app))?;

        match next_event(250)? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => {}
        }

        if app.quit { break; }
    }
    Ok(())
}

pub fn run(file: Option<PathBuf>, stdin_content: Option<String>) -> Result<()> {
    let stdout_mode = stdin_content.is_some();

    // Parse the document before touching the terminal — a parse error prints cleanly.
    let mut app = App::new(file, stdin_content)?;

    enable_raw_mode()?;

    if stdout_mode {
        // TUI on stderr — stdout stays free for the JSON output
        let mut stderr = std::io::stderr();
        execute!(stderr, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(std::io::stderr());
        let mut terminal = Terminal::new(backend)?;
        tui_loop(&mut terminal, &mut app)?;
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    } else {
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        tui_loop(&mut terminal, &mut app)?;
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    }

    Ok(())
}

#[cfg(test)]
mod undo_redo_tests {
    use super::*;

    fn test_app(value: serde_json::Value) -> App {
        App::new(None, Some(value.to_string())).unwrap()
    }

    fn field(name: &str) -> JKey {
        JKey::Field(Rc::from(name))
    }

    fn row_index(app: &App, path: &JPath) -> usize {
        app.flat.iter().position(|r| &r.path == path).unwrap()
    }

    #[test]
    fn undo_after_delete_restores_value_and_matches_full_rebuild() {
        let mut app = test_app(serde_json::json!({"a": 1, "b": 2, "c": 3}));
        let target: JPath = vec![field("b")];
        app.cursor = row_index(&app, &target);

        app.delete_node();
        assert!(get_node_at_path(&app.root, &target).is_none());

        app.undo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
        assert_eq!(get_node_at_path(&app.root, &target), Some(&JNode::Scalar(JScalar::Number("2".into()))));
    }

    #[test]
    fn redo_after_undo_reapplies_delete() {
        let mut app = test_app(serde_json::json!({"a": 1, "b": 2, "c": 3}));
        let target: JPath = vec![field("b")];
        app.cursor = row_index(&app, &target);

        app.delete_node();
        app.undo();
        app.redo();

        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
        assert!(get_node_at_path(&app.root, &target).is_none());
    }

    #[test]
    fn two_edits_at_different_targets_undo_in_reverse_order() {
        let mut app = test_app(serde_json::json!({"a": {"x": 1}, "b": {"y": 2}}));

        let target_a: JPath = vec![field("a"), field("x")];
        app.cursor = row_index(&app, &target_a);
        app.delete_node();

        let target_b: JPath = vec![field("b"), field("y")];
        app.cursor = row_index(&app, &target_b);
        app.delete_node();

        assert!(get_node_at_path(&app.root, &target_a).is_none());
        assert!(get_node_at_path(&app.root, &target_b).is_none());

        // undo the second delete first
        app.undo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
        assert!(get_node_at_path(&app.root, &target_a).is_none());
        assert!(get_node_at_path(&app.root, &target_b).is_some());

        // then the first
        app.undo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
        assert!(get_node_at_path(&app.root, &target_a).is_some());
        assert!(get_node_at_path(&app.root, &target_b).is_some());
    }

    #[test]
    fn move_node_undo_matches_full_rebuild() {
        let mut app = test_app(serde_json::json!({"items": [1, 2, 3]}));
        let target: JPath = vec![field("items"), JKey::Index(1)];
        app.cursor = row_index(&app, &target);
        app.move_node(true); // move up: swaps index 1 and 0
        app.undo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
    }

    #[test]
    fn sort_children_undo_matches_full_rebuild() {
        let mut app = test_app(serde_json::json!({"obj": {"z": 1, "a": 2, "m": 3}}));
        let target: JPath = vec![field("obj")];
        app.cursor = row_index(&app, &target);
        app.sort_children();
        app.undo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
    }

    /// Covers operations that need an EditState to drive interactively (commit_value,
    /// do_rename, do_wrap, run_plugin, insert_child/insert_sibling) — the bookkeeping mechanism
    /// they all share is push_undo(&target) + mutate + refresh_at(&target, true), identical
    /// regardless of which forward function triggers it.
    #[test]
    fn generic_push_undo_and_refresh_at_roundtrip() {
        let mut app = test_app(serde_json::json!({"nested": {"value": "old"}}));
        let target: JPath = vec![field("nested"), field("value")];

        app.push_undo(&target);
        set_node_at_path(&mut app.root, &target, JNode::Scalar(JScalar::String("new".into())));
        app.refresh_at(&target, true);
        assert_eq!(get_node_at_path(&app.root, &target), Some(&JNode::Scalar(JScalar::String("new".into()))));

        app.undo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
        assert_eq!(get_node_at_path(&app.root, &target), Some(&JNode::Scalar(JScalar::String("old".into()))));

        app.redo();
        assert_eq!(app.flat, flatten(&app.root));
        assert_eq!(app.annotated, annotate(&app.root));
        assert_eq!(app.lint_warnings, lint(&app.root));
        assert_eq!(get_node_at_path(&app.root, &target), Some(&JNode::Scalar(JScalar::String("new".into()))));
    }

    #[test]
    fn undo_stack_capped_at_50() {
        let mut app = test_app(serde_json::json!({"a": 1}));
        let target: JPath = vec![field("a")];
        for i in 0..55 {
            app.push_undo(&target);
            set_node_at_path(&mut app.root, &target, JNode::Scalar(JScalar::Number(i.to_string())));
        }
        assert_eq!(app.undo_stack.len(), 50);
    }

    #[test]
    fn nothing_to_undo_or_redo_status_messages() {
        let mut app = test_app(serde_json::json!({"a": 1}));
        app.undo();
        assert_eq!(app.status, "nothing to undo");
        app.redo();
        assert_eq!(app.status, "nothing to redo");
    }
}

/// Permanent, reproducible benchmark for `undo()`'s patched cost vs. a full-rebuild-equivalent
/// (`refresh_flat()`), measured directly on the same `App` state right after. Lives here (not in
/// `src/bench.rs`) because `push_undo`/`refresh_at`/`refresh_flat`/`undo` are private to `App` —
/// only accessible from within this module or its descendants, same reasoning as
/// `undo_redo_tests` above. See BENCHMARK.md for methodology and reference results.
///
/// Run: `cargo test --release app::bench -- --ignored --nocapture`
#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    fn synthetic(items: usize) -> serde_json::Value {
        let arr: Vec<serde_json::Value> = (0..items)
            .map(|i| {
                serde_json::json!({
                    "id": i,
                    "name": format!("item-{}", i),
                    "active": i % 2 == 0,
                    "score": i as f64 * 1.5,
                    "tags": ["a", "b", "c"],
                    "meta": { "created": "2026-07-02", "owner": "svc" },
                    "note": "some representative string payload for a field",
                    "count": i * 3,
                })
            })
            .collect();
        serde_json::Value::Array(arr)
    }

    #[test]
    #[ignore]
    fn bench_undo_patched_vs_full_rebuild() {
        println!(
            "{:>7}  {:>16}  {:>22}  {:>8}",
            "items", "undo (patched)", "full_rebuild_equiv", "speedup"
        );
        for &items in &[1_000usize, 10_000, 50_000, 100_000] {
            let mut app = App::new(None, Some(synthetic(items).to_string())).unwrap();
            let target: JPath = vec![JKey::Index(items - 1), JKey::Field(Rc::from("name"))];

            app.push_undo(&target);
            set_node_at_path(&mut app.root, &target, JNode::Scalar(JScalar::String("edited".into())));
            app.refresh_at(&target, true);

            let t0 = Instant::now();
            app.undo();
            let patched_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // What a full-rebuild undo would have cost, measured directly on the same document
            // right after — the fairest possible A/B comparison (same data, same machine, back
            // to back).
            let t1 = Instant::now();
            app.refresh_flat();
            let full_ms = t1.elapsed().as_secs_f64() * 1000.0;

            println!(
                "{:>7}  {:>14.3}ms  {:>20.2}ms  {:>7.0}x",
                items, patched_ms, full_ms, full_ms / patched_ms.max(0.001)
            );
        }
    }
}
