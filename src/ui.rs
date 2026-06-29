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
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let content = main_chunks[0];

    let (left_area, right_area) = if app.show_left {
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
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
}

// ── Left panel: annotated JSON source ───────────────────────────────────────

fn render_left(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Source ", Style::default().fg(Color::DarkGray)));
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
        .title(Span::styled(" Explorer ", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    let w = inner.width as usize;
    let key_w = (w * 42 / 100).max(15);
    let type_w = 15usize;
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
        .map(|(i, row)| render_row(row, app.scroll + i == app.cursor, key_w, type_w, val_w))
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

    let lines: Vec<Line> = EDIT_TYPES.iter().enumerate()
        .take(inner.height as usize)
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

fn render_row(row: &FlatRow, selected: bool, key_w: usize, type_w: usize, val_w: usize) -> Line<'static> {
    let bg = if selected { Color::Indexed(25) } else { Color::Reset };

    let toggle = match &row.node {
        JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => {
            if *collapsed { "▶ " } else { "▼ " }
        }
        _ => "  ",
    };
    let (icon, _icon_col) = node_icon(&row.node);

    let key_name = match (&row.key, row.index) {
        (Some(k), _) => k.clone(),
        (None, Some(i)) => format!("Item[{}]", i),
        (None, None) => "<root>".to_string(),
    };

    let prefix = format!("{}{}{} ", "  ".repeat(row.depth), toggle, icon);
    let avail = key_w.saturating_sub(prefix.chars().count());
    let name_trunc: String = key_name.chars().take(avail).collect();
    let key_cell = format!("{:<width$}", format!("{}{}", prefix, name_trunc), width = key_w);

    let type_str = node_type_label(&row.node);
    let type_cell = format!("{:<width$}", type_str.chars().take(type_w).collect::<String>(), width = type_w);

    let (val_str, val_col) = node_value_display(&row.node);
    let val_trunc: String = val_str.chars().take(val_w).collect();

    let key_col = match &row.node {
        JNode::Scalar(_) => Color::Cyan,
        _ => Color::White,
    };

    Line::from(vec![
        Span::styled(key_cell, Style::default().fg(key_col).bg(bg)),
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(type_cell, Style::default().fg(Color::DarkGray).bg(bg)),
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(val_trunc, Style::default().fg(val_col).bg(bg)),
    ]).style(Style::default().bg(bg))
}

fn node_icon(node: &JNode) -> (&'static str, Color) {
    match node {
        JNode::Object { .. }               => ("{}", Color::Yellow),
        JNode::Array { .. }                => ("[]", Color::Cyan),
        JNode::Scalar(JScalar::String(_))  => (" A", Color::Green),
        JNode::Scalar(JScalar::Number(_))  => (" #", Color::Yellow),
        JNode::Scalar(JScalar::Bool(_))    => (" ~", Color::Magenta),
        JNode::Scalar(JScalar::Null)       => (" -", Color::DarkGray),
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
        .title(Span::styled(" Detail ", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    match phase_kind {
        1 => {
            if let Some(ref state) = app.edit {
                if let EditPhase::KeyEdit(ref ta) = state.phase {
                    render_key_editor(f, ta, inner);
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

fn render_key_editor(f: &mut Frame, ta: &tui_textarea::TextArea<'static>, inner: Rect) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " New field — key name",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))),
        parts[0],
    );

    f.render_widget(ta, parts[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            " Enter: confirm  Esc: back to type",
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
        EditMode::AddChild => {
            Line::from(Span::styled(
                format!(" Add {} — value", type_name),
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

// ── Status bar ───────────────────────────────────────────────────────────────

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let modified = if app.modified { " [modified]" } else { "" };
    let cursor_path = app.flat.get(app.cursor).map(|r| r.path.clone()).unwrap_or_default();
    let breadcrumb = build_breadcrumb(&app.root, &cursor_path);

    let hint = if let Some(ref state) = app.edit {
        match (&state.phase, &state.mode) {
            (EditPhase::TypeSelect, EditMode::AddChild) =>
                "  ↑↓: type  Enter: confirm  Esc: cancel add",
            (EditPhase::TypeSelect, EditMode::Edit) =>
                "  ↑↓: type  Enter: confirm  Esc: cancel",
            (EditPhase::KeyEdit(_), _) =>
                "  key name  Enter: confirm  Esc: back to type",
            (EditPhase::ValueEdit(_), _) =>
                "  Enter: confirm  Esc: back to type",
        }
    } else {
        "  e: edit  a: add  d: del  D: dup  y: copy  p/P: paste  K/J: move  s: save  q: quit"
    };

    let text = format!(" {}{}  ·  {}{}", app.status, modified, breadcrumb, hint);
    f.render_widget(
        Paragraph::new(Span::styled(text, Style::default().fg(Color::DarkGray))),
        area,
    );
}

fn build_breadcrumb(root: &JNode, path: &[JKey]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut current = root;

    let root_label = match root {
        JNode::Array { items, .. } => format!("Array({})", items.len()),
        JNode::Object { .. } => "Object".to_string(),
        JNode::Scalar(_) => "root".to_string(),
    };
    parts.push(root_label);

    for key in path {
        match (key, current) {
            (JKey::Field(k), JNode::Object { entries, .. }) => {
                if let Some(child) = entries.get(k) {
                    let label = match child {
                        JNode::Array { items, .. } => format!("{}({})", k, items.len()),
                        _ => k.clone(),
                    };
                    parts.push(label);
                    current = child;
                } else { break; }
            }
            (JKey::Index(i), JNode::Array { items, .. }) => {
                if let Some(child) = items.get(*i) {
                    parts.push(format!("Item[{}]", i));
                    current = child;
                } else { break; }
            }
            _ => break,
        }
    }

    parts.join(" › ")
}
