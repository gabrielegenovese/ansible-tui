mod ansible;
mod app;
mod keys;
mod ui;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use std::{
    env,
    io::{self, Stdout},
    path::PathBuf,
    time::Duration,
};

use app::App;

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);

    let root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    let root = root
        .canonicalize()
        .with_context(|| format!("invalid Ansible project: {}", root.display()))?;

    // Second, optional argument: which playbook to read tags from and run.
    // Usage: ansible_tui [PROJECT_DIR] [PLAYBOOK]
    let playbook = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("playbook.yaml"));

    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, root, playbook);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<ratatui::backend::CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(ratatui::backend::CrosstermBackend::new(
        stdout,
    ))?)
}

fn restore_terminal(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
    root: PathBuf,
    playbook: PathBuf,
) -> Result<()> {
    let mut app = App::new(root, playbook)?;

    loop {
        app.poll_process();
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && keys::handle_key(&mut app, key)?
        {
            break;
        }
    }

    Ok(())
}
