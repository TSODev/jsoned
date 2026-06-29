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
    tree::{flatten, JNode, JPath},
};

pub struct App {
    pub root: JNode,
    pub flat: Vec<crate::tree::FlatRow>,
    pub cursor: usize,       // index into flat
    pub scroll: usize,
    pub file: Option<PathBuf>,
    pub modified: bool,
    pub status: String,
    pub quit: bool,
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
        Ok(Self { root, flat, cursor: 0, scroll: 0, file, modified: false, status, quit: false })
    }

    fn refresh_flat(&mut self) {
        self.flat = flatten(&self.root);
        if self.cursor >= self.flat.len() {
            self.cursor = self.flat.len().saturating_sub(1);
        }
    }

    fn current_path(&self) -> JPath {
        self.flat.get(self.cursor).map(|r| r.path.clone()).unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use KeyCode::*;
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
            _ => {}
        }
    }

    fn toggle_collapse(&mut self) {
        let path = self.current_path();
        toggle_node_collapse(&mut self.root, &path);
        self.refresh_flat();
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
        let visible_height = area.height.saturating_sub(3) as usize; // minus status bars
        // sync scroll
        if app.cursor < app.scroll {
            app.scroll = app.cursor;
        } else if app.cursor >= app.scroll + visible_height {
            app.scroll = app.cursor - visible_height + 1;
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
