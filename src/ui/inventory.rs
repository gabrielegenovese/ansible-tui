use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::fs;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let items = app
        .inventories
        .iter()
        .map(|path| ListItem::new(path.file_name().and_then(|x| x.to_str()).unwrap_or("?")));

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" Inventory files ({}) ", app.inventories.len()))
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
    if !app.inventories.is_empty() {
        state.select(Some(app.selected_inventory));
    }
    frame.render_stateful_widget(list, chunks[0], &mut state);

    if let Some(editor) = &mut app.inventory_editor {
        let dirty = if app.inventory_dirty {
            "● unsaved"
        } else {
            "saved"
        };
        editor.set_block(
            Block::default()
                .title(format!(" Editing - Ctrl-S save · Esc close · {dirty} "))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if app.inventory_dirty {
                    Color::Yellow
                } else {
                    Color::Green
                })),
        );
        frame.render_widget(&*editor, chunks[1]);
        return;
    }

    let preview = app
        .selected_inventory_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(|| "No inventory selected".into());

    frame.render_widget(
        Paragraph::new(preview)
            .block(
                Block::default()
                    .title(" Preview · Enter/e to edit ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}
