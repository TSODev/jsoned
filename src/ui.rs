use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{
    app::App,
    pretty::{annotate, Seg, SegColor},
    tree::{get_node_at_path, FlatRow, JKey, JNode, JScalar},
};

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let content = main_chunks[0];

    // Left panel optional
    let (left_area, right_area) = if app.show_left {
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(content);
        (Some(h[0]), h[1])
    } else {
        (None, content)
    };

    // Preview panel optional
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

    if app.editing.is_some() {
        render_edit_popup(f, app, area);
    }
}

// ── Left panel: annotated JSON source with highlight ────────────────────────

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
            let content_bg = if highlighted { Color::Indexed(236) } else { Color::Reset };

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
            // Fill remainder with highlight bg so the whole line is colored
            Line::from(spans).style(Style::default().bg(content_bg))
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn seg_color(c: SegColor) -> Color {
    match c {
        SegColor::Key => Color::Cyan,
        SegColor::Str => Color::Green,
        SegColor::Num => Color::Yellow,
        SegColor::Bool => Color::Magenta,
        SegColor::Null => Color::DarkGray,
        SegColor::Punct => Color::White,
    }
}

// ── Right-top: table (Key / Type / Value) ───────────────────────────────────

fn render_table(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Explorer ", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    let w = inner.width as usize;
    let key_w = (w * 42 / 100).max(15);
    let type_w = 15usize;
    let val_w = w.saturating_sub(key_w + type_w + 4); // 4 = 2 gaps of 2 spaces

    // Header row
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
}

fn render_row(row: &FlatRow, selected: bool, key_w: usize, type_w: usize, val_w: usize) -> Line<'static> {
    let bg = if selected { Color::Indexed(236) } else { Color::Reset };

    let toggle = match &row.node {
        JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => {
            if *collapsed { "> " } else { "v " }
        }
        _ => "  ",
    };
    let (icon, icon_col) = node_icon(&row.node);

    let key_name = match (&row.key, row.index) {
        (Some(k), _) => k.clone(),
        (None, Some(i)) => format!("Item[{}]", i),
        (None, None) => "<root>".to_string(),
    };

    // Key cell: indent + toggle + icon + space + name, truncated and padded to key_w
    let prefix = format!("{}{}{} ", "  ".repeat(row.depth), toggle, icon);
    let avail = key_w.saturating_sub(prefix.chars().count());
    let name_trunc: String = key_name.chars().take(avail).collect();
    let key_cell = format!("{:<width$}", format!("{}{}", prefix, name_trunc), width = key_w);

    // Type cell
    let type_str = node_type_label(&row.node);
    let type_cell = format!("{:<width$}", type_str.chars().take(type_w).collect::<String>(), width = type_w);

    // Value cell
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
        JNode::Object { .. }             => ("{}", Color::Yellow),
        JNode::Array { .. }              => ("[]", Color::Cyan),
        JNode::Scalar(JScalar::String(_)) => (" A", Color::Green),
        JNode::Scalar(JScalar::Number(_)) => (" #", Color::Yellow),
        JNode::Scalar(JScalar::Bool(_))   => (" ~", Color::Magenta),
        JNode::Scalar(JScalar::Null)      => (" -", Color::DarkGray),
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

// ── Right-bottom: JSON preview of selected node ──────────────────────────────

fn render_preview(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Detail ", Style::default().fg(Color::DarkGray)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    let cursor_path = app.flat.get(app.cursor).map(|r| r.path.clone()).unwrap_or_default();

    let title_key = app.flat.get(app.cursor).map(|r| match (&r.key, r.index) {
        (Some(k), _) => k.clone(),
        (None, Some(i)) => format!("Item[{}]", i),
        (None, None) => "<root>".to_string(),
    }).unwrap_or_else(|| "<root>".to_string());

    let node = get_node_at_path(&app.root, &cursor_path);

    // Title bar (1 line)
    let title_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);

    let (icon, icon_col) = node.map(node_icon).unwrap_or((" -", Color::DarkGray));
    let title_line = Line::from(vec![
        Span::styled(format!(" {} ", icon), Style::default().fg(icon_col)),
        Span::styled(title_key, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(title_line), title_area);

    // Content: annotated JSON of the selected subtree
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

// ── Status bar / breadcrumb ──────────────────────────────────────────────────

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let modified = if app.modified { " [modified]" } else { "" };
    let cursor_path = app.flat.get(app.cursor).map(|r| r.path.clone()).unwrap_or_default();
    let breadcrumb = build_breadcrumb(&app.root, &cursor_path);

    let text = format!(
        " {}{}  ·  {}    Space: fold/unfold  e: edit  s: save  [: left  ]: preview  q: quit ",
        app.status, modified, breadcrumb
    );
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

// ── Edit popup (from v0.2, unchanged logic) ──────────────────────────────────

fn render_edit_popup(f: &mut Frame, app: &App, area: Rect) {
    let Some((ref ta, ref path)) = app.editing else { return };

    let path_str: String = path.iter().map(|k| match k {
        JKey::Field(s) => s.clone(),
        JKey::Index(i) => i.to_string(),
    }).collect::<Vec<_>>().join(".");

    let popup_width = area.width.saturating_sub(10).max(40);
    let popup_height = 4u16;
    let x = area.x + area.width.saturating_sub(popup_width) / 2;
    let y = area.y + area.height.saturating_sub(popup_height) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let label = if path_str.is_empty() { "<root>".to_string() } else { path_str };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            format!(" edit · {} ", label),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    f.render_widget(ta, inner_chunks[0]);

    f.render_widget(
        Paragraph::new(Span::styled(
            " Enter: confirm  Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )),
        inner_chunks[1],
    );
}
