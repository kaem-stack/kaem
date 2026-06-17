use std::time::Duration;

use color_eyre::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::tui::app::App;

/// How long to wait for an event before yielding back to the draw loop.
const TICK: Duration = Duration::from_millis(100);

/// Poll for a single terminal event and apply it to the app state.
pub fn handle(app: &mut App) -> Result<()> {
    if event::poll(TICK)?
        && let Event::Key(key) = event::read()?
        && key.kind == KeyEventKind::Press
    {
        handle_key(app, key);
    }
    Ok(())
}

/// Translate a key press into a state mutation.
fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.quit(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        KeyCode::Up => app.previous_contact(),
        KeyCode::Down => app.next_contact(),
        KeyCode::Enter => app.send_message(),
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => app.input.push(c),
        _ => {}
    }
}
