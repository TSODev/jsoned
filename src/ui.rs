use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{app::App, tree::{FlatRow, JNode, JScalar}};

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    render_tree(f, app, chunks[0]);
    render_status(f, app, chunks[1]);
}

fn render_tree(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" jsoned ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height as usize;
    let rows: Vec<Line> = app.flat.iter()
        .skip(app.scroll)
        .take(visible)
        .enumerate()
        .map(|(i, row)| render_row(row, app.scroll + i == app.cursor, inner.width))
        .collect();

    let para = Paragraph::new(rows);
    f.render_widget(para, inner);
}

fn render_row(row: &FlatRow, selected: bool, _width: u16) -> Line<'static> {
    let indent = "  ".repeat(row.depth);

    let key_span = match (&row.key, row.index) {
        (Some(k), _) => Span::styled(
            format!("{}\"{}\": ", indent, k),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        (None, Some(i)) => Span::styled(
            format!("{}[{}]: ", indent, i),
            Style::default().fg(Color::DarkGray),
        ),
        (None, None) => Span::raw(indent.clone()),
    };

    let (value_str, value_style) = match &row.node {
        JNode::Object { entries, collapsed } => {
            let preview = if *collapsed {
                format!("{{…}} ({} fields)", entries.len())
            } else {
                "{".to_string()
            };
            (preview, Style::default().fg(Color::Yellow))
        }
        JNode::Array { items, collapsed } => {
            let preview = if *collapsed {
                format!("[…] ({} items)", items.len())
            } else {
                "[".to_string()
            };
            (preview, Style::default().fg(Color::Yellow))
        }
        JNode::Scalar(s) => scalar_display(s),
    };

    let type_label = Span::styled(
        format!(" [{}]", row.node.type_label()),
        Style::default().fg(Color::DarkGray),
    );

    let value_span = Span::styled(value_str, value_style);

    let bg = if selected { Color::Indexed(236) } else { Color::Reset };
    let line_style = Style::default().bg(bg);

    Line::styled(
        String::new(),
        line_style,
    ).spans(vec![key_span, value_span, type_label])
}

fn scalar_display(s: &JScalar) -> (String, Style) {
    match s {
        JScalar::Null => ("null".to_string(), Style::default().fg(Color::DarkGray)),
        JScalar::Bool(true) => ("true".to_string(), Style::default().fg(Color::Magenta)),
        JScalar::Bool(false) => ("false".to_string(), Style::default().fg(Color::Magenta)),
        JScalar::Number(n) => (n.clone(), Style::default().fg(Color::Yellow)),
        JScalar::String(s) => (format!("\"{}\"", s), Style::default().fg(Color::Green)),
    }
}

fn render_status(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let path_str = if let Some(row) = app.flat.get(app.cursor) {
        row.path.iter().map(|k| match k {
            crate::tree::JKey::Field(s) => s.clone(),
            crate::tree::JKey::Index(i) => i.to_string(),
        }).collect::<Vec<_>>().join(".")
    } else {
        String::new()
    };

    let modified = if app.modified { " [modified]" } else { "" };
    let left = Span::styled(
        format!(" {}{}  ·  {}", app.status, modified, path_str),
        Style::default().fg(Color::DarkGray),
    );
    let hints = Span::styled(
        "  ↑/↓: navigate  Enter: fold/unfold  q: quit ",
        Style::default().fg(Color::DarkGray),
    );

    let para = Paragraph::new(Line::from(vec![left, hints]));
    f.render_widget(para, area);
}
