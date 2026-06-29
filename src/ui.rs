use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{
    app::{App, EditMode, EditPhase, EditState, EDIT_TYPES},
    pretty::{annotate, SegColor},
    tree::{get_node_at_path, FlatRow, JKey, JNode, JScalar},
};

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    let content = main_chunks[0];

    let (left_area, right_area) = if app.show_left {
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(content);
        (Some(h[0]), h[1])
    } else {
        (None, content)
    };

    let (table_area, preview_area) = if app.show_preview {
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(right_area);
        (v[0], Some(v[1]))
    } else {
        (right_area, None)
    };

    if let Some(la) = left_area {
        render_left(f, app, la);
    }
    render_table(f, app, table_area);
    if let Some(pa) = preview_area {
        render_preview(f, app, pa);
    }
    render_status(f, app, main_chunks[1]);

    if app.save_dialog {
        render_save_dialog(f, area);
    }
}

// ── Left panel: annotated JSON source ───────────────────────────────────────

fn render_left(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Source [[] ", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cursor_path = app.flat.get(app.cursor).map(|r| r.path.clone()).unwrap_or_default();
    let total = app.annotated.len();
    let ln_w = total.to_string().len().max(2);

    let lines: Vec<Line> = app.annotated.iter()
        .skip(app.left_scroll)
        .take(inner.height as usize)
        .enumerate()
        .map(|(i, al)| {
            let ln = app.left_scroll + i + 1;
            let highlighted = al.path.starts_with(cursor_path.as_slice());
            let content_bg = if highlighted { Color::Indexed(25) } else { Color::Reset };

            let mut spans = vec![
                Span::styled(
                    format!("{:>width$} ", ln, width = ln_w),
                    Style::default().fg(Color::Indexed(238)).bg(Color::Reset),
                ),
            ];
            for seg in &al.segs {
                spans.push(Span::styled(
                    seg.text.clone(),
                    Style::default().fg(seg_color(seg.color)).bg(content_bg),
                ));
            }
            Line::from(spans).style(Style::default().bg(content_bg))
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn seg_color(c: SegColor) -> Color {
    match c {
        SegColor::Key   => Color::Cyan,
        SegColor::Str   => Color::Green,
        SegColor::Num   => Color::Yellow,
        SegColor::Bool  => Color::Magenta,
        SegColor::Null  => Color::DarkGray,
        SegColor::Punct => Color::White,
    }
}

// ── Explorer: Key / Type / Value table + type dropdown ──────────────────────

fn render_table(f: &mut Frame, app: &App, area: Rect) {
    let editing = app.edit.is_some();
    let in_type_select = matches!(
        app.edit.as_ref().map(|s| &s.phase),
        Some(EditPhase::TypeSelect)
    );

    let border_col = if editing { Color::Yellow } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(" Explorer [^E]", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    let w = inner.width as usize;
    let key_w = (w * 42 / 100).max(15);
    let type_w = 6usize;
    let val_w = w.saturating_sub(key_w + type_w + 4);

    // Header
    let header_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);

    let header = Line::from(vec![
        Span::styled(
            format!("{:<width$}", "Key", width = key_w),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{:<width$}", "Type", width = type_w),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled("Value", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(header), header_area);

    let rows: Vec<Line> = app.flat.iter()
        .skip(app.scroll)
        .take(content_area.height as usize)
        .enumerate()
        .map(|(i, row)| {
            let row_idx = app.scroll + i;
            let is_match = !app.search_matches.is_empty()
                && app.search_matches.binary_search(&row_idx).is_ok();
            render_row(row, row_idx == app.cursor, is_match, key_w, type_w, val_w)
        })
        .collect();

    f.render_widget(Paragraph::new(rows), content_area);

    // Type-select dropdown anchored below the selected row's Type cell
    if in_type_select {
        if let Some(ref state) = app.edit {
            let row_y = content_area.y + (app.cursor - app.scroll) as u16;
            let type_col_x = inner.x + key_w as u16 + 2;
            let dropdown_w = type_w as u16 + 2;
            let dropdown_h = 8u16; // 6 types + 2 borders

            let dropdown_y = if row_y + 1 + dropdown_h <= inner.y + inner.height {
                row_y + 1
            } else {
                row_y.saturating_sub(dropdown_h)
            };

            let dropdown_area = Rect {
                x: type_col_x,
                y: dropdown_y,
                width: dropdown_w.min(area.right().saturating_sub(type_col_x)),
                height: dropdown_h.min(area.bottom().saturating_sub(dropdown_y)),
            };

            render_type_dropdown(f, state, dropdown_area);
        }
    }
}

fn render_type_dropdown(f: &mut Frame, state: &EditState, area: Rect) {
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let skip = if matches!(state.mode, EditMode::Edit) { 2 } else { 0 };
    let take = if matches!(state.mode, EditMode::Wrap) { 2 } else { inner.height as usize };
    let lines: Vec<Line> = EDIT_TYPES.iter().enumerate()
        .skip(skip)
        .take(take)
        .map(|(i, name)| {
            let selected = i == state.type_cursor;
            let (fg, bg) = if selected {
                (Color::Black, Color::Yellow)
            } else {
                (Color::White, Color::Reset)
            };
            let text = format!("{:<width$}", name, width = inner.width as usize);
            Line::from(Span::styled(text, Style::default().fg(fg).bg(bg)))
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_row(row: &FlatRow, selected: bool, is_match: bool, key_w: usize, type_w: usize, val_w: usize) -> Line<'static> {
    let bg = if selected { Color::Indexed(25) } else if is_match { Color::Indexed(22) } else { Color::Reset };

    let toggle = match &row.node {
        JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => {
            if *collapsed { "▶ " } else { "▼ " }
        }
        _ => "  ",
    };
    let (icon, icon_col) = node_icon(&row.node);

    let key_name = match (&row.key, row.index) {
        (Some(k), _) => k.clone(),
        (None, Some(i)) => format!("Item[{}]", i),
        (None, None) => "<root>".to_string(),
    };

    let key_col = match &row.node {
        JNode::Scalar(_) => Color::Cyan,
        _ => Color::White,
    };

    // Split key column into 3 spans so the icon gets its own color
    let pre = format!("{}{}", "  ".repeat(row.depth), toggle);
    let pre_len   = pre.chars().count();
    let icon_len  = icon.chars().count();
    let avail     = key_w.saturating_sub(pre_len + icon_len + 1);
    let name_trunc: String = key_name.chars().take(avail).collect();
    let post = format!(" {:<width$}", name_trunc, width = avail);

    let type_str = node_type_label(&row.node);
    let type_cell = format!("{:<width$}", type_str.chars().take(type_w).collect::<String>(), width = type_w);

    let (val_str, val_col) = node_value_display(&row.node);
    let val_trunc: String = val_str.chars().take(val_w).collect();

    Line::from(vec![
        Span::styled(pre,      Style::default().fg(key_col).bg(bg)),
        Span::styled(icon,     Style::default().fg(icon_col).bg(bg)),
        Span::styled(post,     Style::default().fg(key_col).bg(bg)),
        Span::styled("  ",     Style::default().bg(bg)),
        Span::styled(type_cell, Style::default().fg(Color::DarkGray).bg(bg)),
        Span::styled("  ",     Style::default().bg(bg)),
        Span::styled(val_trunc, Style::default().fg(val_col).bg(bg)),
    ]).style(Style::default().bg(bg))
}

fn node_icon(node: &JNode) -> (&'static str, Color) {
    match node {
        JNode::Object { .. }               => ("{}", Color::Yellow),
        JNode::Array { .. }                => ("[]", Color::Cyan),
        JNode::Scalar(JScalar::String(_))  => (" \"", Color::Green),
        JNode::Scalar(JScalar::Number(_))  => (" №", Color::Yellow),
        JNode::Scalar(JScalar::Bool(_))    => (" ◆", Color::Magenta),
        JNode::Scalar(JScalar::Null)       => (" ∅", Color::DarkGray),
    }
}

fn node_type_label(node: &JNode) -> String {
    match node {
        JNode::Object { .. }               => "Object".to_string(),
        JNode::Array { items, .. }         => format!("Array ({})", items.len()),
        JNode::Scalar(JScalar::String(_))  => "String".to_string(),
        JNode::Scalar(JScalar::Number(_))  => "Number".to_string(),
        JNode::Scalar(JScalar::Bool(_))    => "Bool".to_string(),
        JNode::Scalar(JScalar::Null)       => "Null".to_string(),
    }
}

fn node_value_display(node: &JNode) -> (String, Color) {
    match node {
        JNode::Object { entries, .. }      => (format!("{} items", entries.len()), Color::DarkGray),
        JNode::Array { items, .. }         => (format!("{} items", items.len()), Color::DarkGray),
        JNode::Scalar(JScalar::Null)       => ("null".to_string(), Color::DarkGray),
        JNode::Scalar(JScalar::Bool(b))    => (b.to_string(), Color::Magenta),
        JNode::Scalar(JScalar::Number(n))  => (n.clone(), Color::Yellow),
        JNode::Scalar(JScalar::String(s))  => (s.clone(), Color::Green),
    }
}

// ── Detail: JSON preview · key editor · value editor ────────────────────────

fn render_preview(f: &mut Frame, app: &App, area: Rect) {
    let phase_kind = match app.edit.as_ref().map(|s| &s.phase) {
        Some(EditPhase::KeyEdit(_))  => 1,
        Some(EditPhase::ValueEdit(_)) => 2,
        _ => 0,
    };

    let border_col = if phase_kind > 0 { Color::Yellow } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(" Detail [] ", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    match phase_kind {
        1 => {
            if let Some(ref state) = app.edit {
                if let EditPhase::KeyEdit(ref ta) = state.phase {
                    let title = if matches!(state.mode, EditMode::Rename) {
                        "Rename key".to_string()
                    } else if matches!(state.mode, EditMode::Wrap) {
                        let type_name = EDIT_TYPES.get(state.type_cursor).copied().unwrap_or("Object");
                        format!("Wrap in {} — key name", type_name)
                    } else {
                        let type_name = EDIT_TYPES.get(state.type_cursor).copied().unwrap_or("field");
                        format!("New {} — key name", type_name)
                    };
                    render_key_editor(f, ta, &title, inner);
                    return;
                }
            }
        }
        2 => {
            if let Some(ref state) = app.edit {
                if let EditPhase::ValueEdit(ref ta) = state.phase {
                    render_value_editor(f, app, ta, state.type_cursor, &state.mode, inner);
                    return;
                }
            }
        }
        _ => {}
    }

    render_preview_normal(f, app, inner);
}

fn render_key_editor(f: &mut Frame, ta: &tui_textarea::TextArea<'static>, title: &str, inner: Rect) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", title),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))),
        parts[0],
    );

    f.render_widget(ta, parts[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            " Enter: confirm  Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )),
        parts[2],
    );
}

fn render_value_editor(
    f: &mut Frame,
    app: &App,
    ta: &tui_textarea::TextArea<'static>,
    type_cursor: usize,
    mode: &EditMode,
    inner: Rect,
) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let type_name = EDIT_TYPES.get(type_cursor).copied().unwrap_or("String");

    let title = match mode {
        EditMode::Edit => {
            let key_name = app.flat.get(app.cursor).map(|r| match (&r.key, r.index) {
                (Some(k), _) => k.clone(),
                (None, Some(i)) => format!("Item[{}]", i),
                (None, None) => "<root>".to_string(),
            }).unwrap_or_default();
            Line::from(vec![
                Span::styled(format!(" {} ", type_name), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(key_name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ])
        }
        EditMode::AddChild | EditMode::Rename => {
            Line::from(Span::styled(
                format!(" Add {} — value", type_name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))
        }
        EditMode::Wrap => {
            Line::from(Span::styled(
                format!(" Wrap in {}", type_name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))
        }
    };

    f.render_widget(Paragraph::new(title), parts[0]);
    f.render_widget(ta, parts[1]);
    f.render_widget(
        Paragraph::new(Span::styled(
            " Enter: confirm  Esc: back to type",
            Style::default().fg(Color::DarkGray),
        )),
        parts[2],
    );
}

fn render_preview_normal(f: &mut Frame, app: &App, inner: Rect) {
    let cursor_path = app.flat.get(app.cursor).map(|r| r.path.clone()).unwrap_or_default();

    let title_key = app.flat.get(app.cursor).map(|r| match (&r.key, r.index) {
        (Some(k), _) => k.clone(),
        (None, Some(i)) => format!("Item[{}]", i),
        (None, None) => "<root>".to_string(),
    }).unwrap_or_else(|| "<root>".to_string());

    let node = get_node_at_path(&app.root, &cursor_path);

    let title_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);

    let (icon, icon_col) = node.map(node_icon).unwrap_or((" -", Color::DarkGray));
    let title_line = Line::from(vec![
        Span::styled(format!(" {} ", icon), Style::default().fg(icon_col)),
        Span::styled(title_key, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(title_line), title_area);

    if let Some(node) = node {
        let preview = annotate(node);
        let ln_w = preview.len().to_string().len().max(1);

        let lines: Vec<Line> = preview.iter()
            .take(content_area.height as usize)
            .enumerate()
            .map(|(i, al)| {
                let mut spans = vec![
                    Span::styled(
                        format!("{:>width$} ", i + 1, width = ln_w),
                        Style::default().fg(Color::Indexed(238)),
                    ),
                ];
                for seg in &al.segs {
                    spans.push(Span::styled(
                        seg.text.clone(),
                        Style::default().fg(seg_color(seg.color)),
                    ));
                }
                Line::from(spans)
            })
            .collect();

        f.render_widget(Paragraph::new(lines), content_area);
    }
}

// ── Save dialog ──────────────────────────────────────────────────────────────

fn render_save_dialog(f: &mut Frame, area: Rect) {
    let w = 46u16;
    let h = 9u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Unsaved changes ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Save before quitting?",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("  [s]    Save and quit", Style::default().fg(Color::Green))),
        Line::from(Span::styled("  [n]    Quit without saving", Style::default().fg(Color::Red))),
        Line::from(Span::styled("  [Esc]  Cancel", Style::default().fg(Color::DarkGray))),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

// ── Status bar ───────────────────────────────────────────────────────────────

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let modified = if app.modified { " [modified]" } else { "" };
    let cursor_path = app.flat.get(app.cursor).map(|r| r.path.clone()).unwrap_or_default();
    let dot_path = build_dot_path(&cursor_path);

    // Line 1: search input OR filename · dot-path [· match X/N]
    let line1 = if app.search_active {
        format!(" / {}_", app.search_query)
    } else {
        let base = if dot_path.is_empty() {
            format!(" {}{}", app.status, modified)
        } else {
            format!(" {}{}  ·  {}", app.status, modified, dot_path)
        };
        if !app.search_matches.is_empty() {
            format!("{}  ·  [{}/{}]", base, app.search_match_cursor + 1, app.search_matches.len())
        } else if !app.search_query.is_empty() {
            format!("{}  ·  [no match]", base)
        } else {
            base
        }
    };

    // Line 2: contextual hints — background Indexed(236), text/bg vary by context
    let hint_line = if app.search_active {
        Line::from(Span::styled(
            "  Enter: go to match  Esc: cancel",
            Style::default().fg(Color::Cyan).bg(Color::Indexed(236)),
        ))
    } else if !app.search_matches.is_empty() {
        Line::from(Span::styled(
            "  n/N: next/prev match  Esc: clear  /: new search",
            Style::default().fg(Color::Cyan).bg(Color::Indexed(236)),
        ))
    } else if app.confirm_quit {
        Line::from(Span::styled(
            "  Press q again to quit  (any other key to cancel)",
            Style::default().fg(Color::White).bg(Color::Indexed(52)),
        ))
    } else if let Some(ref state) = app.edit {
        let h = match (&state.phase, &state.mode) {
            (EditPhase::TypeSelect, EditMode::AddChild) =>
                "  ↑↓: type  Enter: confirm  Esc: cancel",
            (EditPhase::TypeSelect, EditMode::Edit) =>
                "  ↑↓: type  Enter: confirm  Esc: cancel",
            (EditPhase::TypeSelect, EditMode::Wrap) =>
                "  ↑↓: Object / Array  Enter: wrap  Esc: cancel",
            (EditPhase::TypeSelect, EditMode::Rename) => "",
            (EditPhase::KeyEdit(_), EditMode::Rename) =>
                "  Enter: rename  Esc: cancel",
            (EditPhase::KeyEdit(_), EditMode::Wrap) =>
                "  key name for wrapper Object  Enter: wrap  Esc: cancel",
            (EditPhase::KeyEdit(_), _) =>
                "  key name  Enter: confirm  Esc: cancel",
            (EditPhase::ValueEdit(_), _) =>
                "  Enter: confirm  Esc: back to type",
        };
        Line::from(Span::styled(
            h,
            Style::default().fg(Color::Yellow).bg(Color::Indexed(236)),
        ))
    } else {
        Line::from(Span::styled(
            "  e: edit  r: rename  a: add  d: del  D: dup  y: copy  p/P: paste  K/J: move  u: undo  S: sort  E/C: expand/collapse  s: save  q: quit",
            Style::default().fg(Color::Indexed(252)).bg(Color::Indexed(236)),
        ))
    };

    let lines = vec![
        Line::from(Span::styled(line1, Style::default().fg(Color::White))),
        hint_line,
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn build_dot_path(path: &[JKey]) -> String {
    path.iter().map(|k| match k {
        JKey::Field(s) => s.clone(),
        JKey::Index(i) => i.to_string(),
    }).collect::<Vec<_>>().join(".")
}

