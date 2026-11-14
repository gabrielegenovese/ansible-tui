mod host_vars;
mod inventory;
mod output;
mod tasks;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Tabs},
};

use crate::app::{App, RunState, Screen};

const ACCENT: Color = Color::Cyan;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let root_name = app
        .root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("ansible");
    let playbook_name = app
        .playbook
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("playbook.yaml");

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    let tab_titles: Vec<Line> = Screen::ALL
        .iter()
        .map(|s| Line::from(format!("{} {}", s.index() + 1, s.title())))
        .collect();

    let tabs = Tabs::new(tab_titles)
        .select(app.screen.index())
        .block(
            Block::default()
                .title(format!(" 🍵 Ansible TUI · {root_name} · {playbook_name} "))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT)),
        )
        .highlight_style(
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(" ");

    frame.render_widget(tabs, vertical[0]);

    match app.screen {
        Screen::Tasks => tasks::draw(frame, app, vertical[1]),
        Screen::Inventory => inventory::draw(frame, app, vertical[1]),
        Screen::HostVars => host_vars::draw(frame, app, vertical[1]),
        Screen::Output => output::draw(frame, app, vertical[1]),
    }

    draw_footer(frame, app, vertical[2]);
}

fn draw_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let key =
        |s: &'static str| Span::styled(s, Style::default().add_modifier(Modifier::BOLD).fg(ACCENT));
    let hint = |s: &'static str| Span::raw(s);

    let mut spans = vec![key("↑↓ "), hint("nav  "), key("Tab "), hint("switch  ")];

    match app.screen {
        Screen::Tasks => spans.extend([key("Enter/r "), hint("run  ")]),
        Screen::Inventory if app.inventory_editor.is_some() => {
            spans = vec![key("Ctrl-S "), hint("save  "), key("Esc "), hint("close  ")]
        }
        Screen::Inventory => spans.extend([key("Enter/e "), hint("edit  ")]),
        Screen::HostVars => {}
        Screen::Output => spans.extend([
            key("↑↓/PgUp/PgDn "),
            hint("scroll  "),
            key("g/G "),
            hint("top/bottom  "),
        ]),
    }

    if app.inventory_editor.is_none() {
        spans.extend([key("f "), hint("refresh  "), key("q "), hint("quit  ")]);
    }

    let status_style = match app.run_state {
        RunState::Running => Style::default().fg(Color::Yellow),
        RunState::Success => Style::default().fg(Color::Green),
        RunState::Failed => Style::default().fg(Color::Red),
        RunState::Idle => Style::default(),
    };
    spans.push(Span::styled(
        &app.status,
        status_style.add_modifier(Modifier::ITALIC),
    ));

    frame.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
}
