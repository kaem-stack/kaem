use color_eyre::Result;
use ratatui::DefaultTerminal;

use kaem_radio::{Config, Radio, open};

use crate::action::Action;
use crate::core::chat::Chat;
use crate::core::seed;
use crate::tui::{events, render};

pub struct Ui {
    pub chat: Chat,
}

pub struct App {
    pub running: bool,
    pub signal: u8,
    pub ui: Ui,
}

impl App {
    pub fn new(radio: Box<dyn Radio>, callsign: String) -> Self {
        Self {
            running: true,
            signal: 75,
            ui: Ui {
                chat: Chat::new(seed::roster(), radio, callsign),
            },
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.running {
            self.ui.chat.poll();
            terminal.draw(|frame| render::render(&self, frame))?;
            if let Some(action) = events::poll()? {
                self.handle(action);
            }
        }
        Ok(())
    }

    pub fn handle(&mut self, action: Action) {
        match action {
            Action::Quit => self.running = false,
            Action::NextContact => self.ui.chat.next_contact(),
            Action::PreviousContact => self.ui.chat.previous_contact(),
            Action::SendMessage => self.ui.chat.send_message(),
            Action::Input(req) => {
                self.ui.chat.input.handle(req);
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let radio = open(Config::Loopback).expect("loopback transport is infallible");
        Self::new(radio, "me".into())
    }
}
