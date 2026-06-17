use chrono::{NaiveDate, NaiveTime, Utc};
use color_eyre::Result;
use ratatui::DefaultTerminal;
use tui_input::Input;

use crate::action::Action;
use crate::model::{Author, Contact, Message};
use crate::tui::{events, render};

pub struct Chat {
    pub contacts: Vec<Contact>,
    pub selected: usize,
    pub input: Input,
    pub encrypted: bool,
}

impl Chat {
    pub fn selected_contact(&self) -> &Contact {
        &self.contacts[self.selected]
    }

    pub fn next_contact(&mut self) {
        if self.contacts.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.contacts.len();
        self.mark_read();
    }

    pub fn previous_contact(&mut self) {
        if self.contacts.is_empty() {
            return;
        }
        let len = self.contacts.len();
        self.selected = (self.selected + len - 1) % len;
        self.mark_read();
    }

    pub fn send_message(&mut self) {
        let body = self.input.value().trim().to_string();
        if body.is_empty() {
            return;
        }
        self.input = Input::default();
        if let Some(contact) = self.contacts.get_mut(self.selected) {
            contact.history.push(Message {
                author: Author::Me,
                timestamp: Utc::now(),
                body: body.clone(),
            });
            contact.last_message = body;
        }
    }

    fn mark_read(&mut self) {
        if let Some(contact) = self.contacts.get_mut(self.selected) {
            contact.unread = 0;
        }
    }
}

pub struct Ui {
    pub chat: Chat,
}

pub struct App {
    pub running: bool,
    pub signal: u8,
    pub ui: Ui,
}

impl App {
    pub fn new() -> Self {
        let today = Utc::now().date_naive();
        let yesterday = today.pred_opt().unwrap_or(today);

        let at = |date: NaiveDate, h: u32, m: u32| {
            date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
                .and_utc()
        };

        let contacts = vec![
            Contact {
                name: "alice".into(),
                unread: 0,
                last_message: "nice, ttyl".into(),
                history: vec![
                    Message {
                        author: Author::Them,
                        timestamp: at(yesterday, 10, 30),
                        body: "hey, are you on the new repeater?".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: at(yesterday, 10, 31),
                        body: "yep, just hopped on. signal's clean".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: at(yesterday, 10, 32),
                        body: "how's the mesh holding up?".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: at(yesterday, 10, 33),
                        body: "solid".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: at(yesterday, 10, 33),
                        body: "way better than the old node".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: at(yesterday, 10, 34),
                        body: "we should drop a third repeater by the ridge".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: at(yesterday, 10, 36),
                        body: "agreed".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: at(yesterday, 10, 36),
                        body: "i'll bring the hardware tomorrow".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: at(today, 9, 15),
                        body: "did you get the hardware set up?".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: at(today, 9, 17),
                        body: "working on it now".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: at(today, 9, 17),
                        body: "connector was the wrong gauge but i rigged it".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: at(today, 9, 20),
                        body: "nice, ttyl".into(),
                    },
                ],
            },
            Contact {
                name: "bob".into(),
                unread: 0,
                last_message: "ttyl".into(),
                history: vec![Message {
                    author: Author::Them,
                    timestamp: at(yesterday, 9, 58),
                    body: "heading off-grid, ttyl".into(),
                }],
            },
            Contact {
                name: "carol".into(),
                unread: 5,
                last_message: "ping me when you see this".into(),
                history: vec![Message {
                    author: Author::Them,
                    timestamp: at(today, 10, 40),
                    body: "ping me when you see this".into(),
                }],
            },
            Contact {
                name: "dave".into(),
                unread: 2,
                last_message: "relay node is up".into(),
                history: vec![Message {
                    author: Author::Them,
                    timestamp: at(today, 8, 12),
                    body: "relay node is up on channel 7".into(),
                }],
            },
        ];

        Self {
            running: true,
            signal: 75,
            ui: Ui {
                chat: Chat {
                    contacts,
                    selected: 0,
                    input: Input::default(),
                    encrypted: true,
                },
            },
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.running {
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
            Action::Input(req) => { self.ui.chat.input.handle(req); }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
