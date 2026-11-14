use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(err) = &app.tags_error {
        let text = format!(
            "Could not read tags from {}:\n\n{}\n\n Check that Ansible is installed and that the playbook path is correct, then press 'f' to try again.",
            app.playbook.display(),
            err
        );
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::default().fg(Color::Red))
                .block(
                    Block::default()
                        .title(" Tasks - error reading the playbook ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Red)),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let items = app.available_tags.iter().map(|tag| {
        ListItem::new(Line::from(vec![
            Span::styled("▶ ", Style::default().fg(Color::Green)),
            Span::raw(tag.as_str()),
        ]))
    });

    let title = if app.available_tags.is_empty() {
        format!(" Tasks - no tag found in {} ", app.playbook.display())
    } else {
        format!(
            " Tasks from {} ({}) ",
            app.playbook.display(),
            app.available_tags.len()
        )
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
    state.select(
        app.available_tags
            .is_empty()
            .then_some(0)
            .or(Some(app.selected_tag)),
    );
    frame.render_stateful_widget(list, area, &mut state);
}
