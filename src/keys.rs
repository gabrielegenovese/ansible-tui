use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Screen};

/// Handle one key press. Returns `Ok(true)` if the app should quit.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    // The inventory editor, when open, captures all keys except its own shortcuts.
    if let Some(editor) = &mut app.inventory_editor {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                app.inventory_editor = None;
                app.inventory_dirty = false;
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => app.save_inventory()?,
            _ => {
                editor.input(key);
                app.inventory_dirty = true;
            }
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
        KeyCode::Char('1') => app.screen = Screen::Tasks,
        KeyCode::Char('2') => app.screen = Screen::Inventory,
        KeyCode::Char('3') => app.screen = Screen::HostVars,
        KeyCode::Char('4') => app.screen = Screen::Output,
        KeyCode::Tab => app.screen = next_screen(app.screen),
        KeyCode::BackTab => app.screen = prev_screen(app.screen),

        KeyCode::Up | KeyCode::Char('k') => move_selection(app, -1),
        KeyCode::Down | KeyCode::Char('j') => move_selection(app, 1),
        KeyCode::PageUp => {
            if app.screen == Screen::Output {
                app.scroll_output(10);
            }
        }
        KeyCode::PageDown => {
            if app.screen == Screen::Output {
                app.scroll_output(-10);
            }
        }
        KeyCode::Char('g') | KeyCode::Home => {
            if app.screen == Screen::Output {
                app.scroll_output_to_top();
            }
        }
        KeyCode::Char('G') | KeyCode::End => {
            if app.screen == Screen::Output {
                app.scroll_output_to_bottom();
            }
        }

        KeyCode::Enter | KeyCode::Char('r') => match app.screen {
            Screen::Tasks => app.run()?,
            Screen::Inventory => app.open_inventory_editor()?,
            Screen::HostVars | Screen::Output => {}
        },
        KeyCode::Char('e') => {
            if app.screen == Screen::Inventory {
                app.open_inventory_editor()?;
            }
        }
        KeyCode::Char('f') => app.refresh()?,
        _ => {}
    }

    Ok(false)
}

fn next_screen(current: Screen) -> Screen {
    let idx = (current.index() + 1) % Screen::ALL.len();
    Screen::ALL[idx]
}

fn prev_screen(current: Screen) -> Screen {
    let idx = (current.index() + Screen::ALL.len() - 1) % Screen::ALL.len();
    Screen::ALL[idx]
}

fn move_selection(app: &mut App, delta: i64) {
    match app.screen {
        Screen::Tasks => {
            app.selected_tag = shift(app.selected_tag, app.available_tags.len(), delta);
        }
        Screen::Inventory => {
            app.selected_inventory = shift(app.selected_inventory, app.inventories.len(), delta);
        }
        Screen::HostVars => {
            app.selected_host_vars = shift(app.selected_host_vars, app.host_vars.len(), delta);
        }
        Screen::Output => {
            // Up moves further back into history (increases the scroll offset);
            // Down moves toward the live tail (decreases it).
            if delta < 0 {
                app.scroll_output(1);
            } else {
                app.scroll_output(-1);
            }
        }
    }
}

fn shift(current: usize, len: usize, delta: i64) -> usize {
    if len == 0 {
        return 0;
    }
    let next = current as i64 + delta;
    next.clamp(0, len as i64 - 1) as usize
}
