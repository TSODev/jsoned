use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::Backend, backend::CrosstermBackend, Terminal};
use std::path::{Path, PathBuf};

use crate::{
    diff::{diff, load_node, DiffRow, DiffStatus},
    event::{next_event, AppEvent},
};

pub struct DiffApp {
    pub rows: Vec<DiffRow>,
    pub cursor: usize,
    pub scroll: usize,
    pub file_a: PathBuf,
    pub file_b: PathBuf,
    pub only_changes: bool,
    pub quit: bool,
}

impl DiffApp {
    pub fn new(path_a: &Path, path_b: &Path) -> Result<Self> {
        let root_a = load_node(path_a)?;
        let root_b = load_node(path_b)?;
        let rows = diff(Some(&root_a), Some(&root_b));
        Ok(Self {
            rows,
            cursor: 0,
            scroll: 0,
            file_a: path_a.to_path_buf(),
            file_b: path_b.to_path_buf(),
            only_changes: false,
            quit: false,
        })
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        if self.only_changes {
            self.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.status != DiffStatus::Unchanged)
                .map(|(i, _)| i)
                .collect()
        } else {
            (0..self.rows.len()).collect()
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char(']') | KeyCode::Char('n') => self.jump_to_change(1),
            KeyCode::Char('[') | KeyCode::Char('N') => self.jump_to_change(-1),
            KeyCode::Char('o') => self.only_changes = !self.only_changes,
            _ => {}
        }
    }

    fn move_cursor(&mut self, dir: i32) {
        let vis = self.visible_indices();
        if vis.is_empty() {
            return;
        }
        let pos = vis.iter().position(|&i| i == self.cursor).unwrap_or(0);
        let new_pos = (pos as i32 + dir).clamp(0, vis.len() as i32 - 1) as usize;
        self.cursor = vis[new_pos];
    }

    fn jump_to_change(&mut self, dir: i32) {
        let vis = self.visible_indices();
        if vis.is_empty() {
            return;
        }
        let Some(pos) = vis.iter().position(|&i| i == self.cursor) else { return };
        let mut p = pos as i32;
        loop {
            p += dir;
            if p < 0 || p as usize >= vis.len() {
                break;
            }
            let idx = vis[p as usize];
            if self.rows[idx].status != DiffStatus::Unchanged {
                self.cursor = idx;
                break;
            }
        }
    }
}

pub fn run(path_a: &Path, path_b: &Path) -> Result<()> {
    let mut app = DiffApp::new(path_a, path_b)?;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = diff_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn diff_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut DiffApp) -> Result<()> {
    loop {
        let area = terminal.size()?;
        let content_h = (area.height as usize).saturating_sub(2).saturating_sub(3); // status bar + table header/border

        let vis = app.visible_indices();
        if let Some(pos) = vis.iter().position(|&i| i == app.cursor) {
            if pos < app.scroll {
                app.scroll = pos;
            } else if content_h > 0 && pos >= app.scroll + content_h {
                app.scroll = pos.saturating_sub(content_h) + 1;
            }
        }

        terminal.draw(|f| crate::ui::render_diff(f, app))?;

        match next_event(250)? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => {}
        }

        if app.quit {
            break;
        }
    }
    Ok(())
}
