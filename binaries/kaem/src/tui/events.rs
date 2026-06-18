use std::time::Duration;

use color_eyre::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use tui_input::InputRequest;

use crate::action::Action;

const TICK: Duration = Duration::from_millis(100);

pub fn poll() -> Result<Option<Action>> {
    if !event::poll(TICK)? {
        return Ok(None);
    }
    Ok(to_action(&event::read()?))
}

fn to_action(event: &Event) -> Option<Action> {
    let Event::Key(key) = event else { return None };
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match key.code {
        KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Up => Some(Action::PreviousContact),
        KeyCode::Down => Some(Action::NextContact),
        KeyCode::Enter => Some(Action::SendMessage),
        KeyCode::Char(c) => Some(Action::Input(InputRequest::InsertChar(c))),
        KeyCode::Backspace => Some(Action::Input(InputRequest::DeletePrevChar)),
        KeyCode::Delete => Some(Action::Input(InputRequest::DeleteNextChar)),
        KeyCode::Left => Some(Action::Input(InputRequest::GoToPrevChar)),
        KeyCode::Right => Some(Action::Input(InputRequest::GoToNextChar)),
        KeyCode::Home => Some(Action::Input(InputRequest::GoToStart)),
        KeyCode::End => Some(Action::Input(InputRequest::GoToEnd)),
        _ => None,
    }
}
