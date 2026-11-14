use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{ansible::read_host_vars_preview, app::App};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let items = app.host_vars.iter().map(|path| {
        let name = path.file_name().and_then(|x| x.to_str()).unwrap_or("?");
        if path.is_dir() {
            ListItem::new(Span::styled(
                format!("📁 {name}/"),
                Style::default().fg(Color::Cyan),
            ))
        } else {
            ListItem::new(Span::raw(format!("📄 {name}")))
        }
    });

    let title = if app.host_vars.is_empty() {
        " host_vars - none found ".to_string()
    } else {
        format!(" host_vars ({}) ", app.host_vars.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("➤ ");

    let mut state = ListState::default();
    if !app.host_vars.is_empty() {
        state.select(Some(app.selected_host_vars));
    }
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let preview = app
        .selected_host_vars_path()
        .map(read_host_vars_preview)
        .unwrap_or_else(|| {
            "Select a host to view its variables (host_vars/<host>.yml or host_vars/<host>/*.yml)"
                .into()
        });

    frame.render_widget(
        Paragraph::new(preview)
            .block(
                Block::default()
                    .title(" Variables (read-only) ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}
