use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::app::{App, RunState, SPINNER};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(1)])
        .split(area);

    draw_hosts(frame, app, chunks[0]);
    draw_log(frame, app, chunks[1]);
}

fn draw_hosts(frame: &mut Frame, app: &App, area: Rect) {
    let mut cards = Vec::new();
    for host in app.hosts.values() {
        let (symbol, style) = status_style(&host.status);
        cards.push(Line::from(vec![
            Span::styled(format!("{symbol} "), style.add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{:<18}", host.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<12}", host.status), style),
            Span::raw(if host.task.is_empty() {
                "waiting".into()
            } else {
                host.task.clone()
            }),
        ]));
    }
    if cards.is_empty() {
        cards.push(Line::from("No hosts detected from the inventory yet."));
    }

    let title = format!(" Hosts {} ", summary_string(app));
    let border_color = match app.run_state {
        RunState::Running => Color::Yellow,
        RunState::Success => Color::Green,
        RunState::Failed => Color::Red,
        RunState::Idle => Color::DarkGray,
    };

    frame.render_widget(
        Paragraph::new(cards)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let visible = area.height.saturating_sub(2) as usize;
    let total = app.log.len();
    let scroll = app.output_scroll.min(total);
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible.max(1));

    let lines: Vec<Line> = app.log[start..end]
        .iter()
        .map(|s| Line::from(s.as_str()))
        .collect();

    let mut title = match app.run_state {
        RunState::Running => {
            let spinner = SPINNER[(app.tick as usize / 2) % SPINNER.len()];
            let elapsed = app
                .run_started
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);
            format!(" Output {spinner} running · {elapsed:.0}s ")
        }
        _ => " Output ".to_string(),
    };
    if scroll > 0 {
        title.push_str(&format!(
            "· scrolled back {scroll} line(s) - press G to follow again "
        ));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn status_style(status: &str) -> (&'static str, Style) {
    match status {
        "ok" => ("✓", Style::default().fg(Color::Green)),
        "changed" => ("●", Style::default().fg(Color::Yellow)),
        "failed" | "unreachable" => ("✗", Style::default().fg(Color::Red)),
        "skipped" => ("↷", Style::default().fg(Color::DarkGray)),
        _ => ("○", Style::default().fg(Color::Gray)),
    }
}

fn summary_string(app: &App) -> String {
    let counts = app.host_summary();
    let get = |k: &str| *counts.get(k).unwrap_or(&0);
    format!(
        "- {} ok, {} changed, {} failed, {} skipped, {} waiting",
        get("ok"),
        get("changed"),
        get("failed"),
        get("skipped"),
        get("waiting")
    )
}
