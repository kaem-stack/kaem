use color_eyre::Result;
use ratatui::DefaultTerminal;

use crate::datetime;
use crate::tui::{events, render};

/// Who authored a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    Me,
    Them,
}

/// A single chat message within a conversation.
#[derive(Debug, Clone)]
pub struct Message {
    pub author: Author,
    pub timestamp: i64, // Unix seconds UTC
    pub body: String,
}

/// A peer on the mesh and the conversation we hold with them.
#[derive(Debug, Clone)]
pub struct Contact {
    pub name: String,
    pub unread: u32,
    pub last_message: String,
    pub history: Vec<Message>,
}

/// Top-level application state. Everything the UI renders is derived from here.
pub struct App {
    pub running: bool,
    pub contacts: Vec<Contact>,
    pub selected: usize,
    pub input: String,
    pub encrypted: bool,
}

impl App {
    /// Build the app pre-seeded with a handful of demo contacts.
    pub fn new() -> Self {
        // Anchor demo timestamps to actual today/yesterday so date labels are always live.
        let now = datetime::now();
        let day = (now / 86_400) * 86_400; // today midnight UTC
        let yest = day - 86_400; // yesterday midnight UTC

        // Helpers: hhmm offset → unix seconds
        let t = |base: i64, h: i64, m: i64| base + h * 3600 + m * 60;

        let contacts = vec![
            Contact {
                name: "alice".into(),
                unread: 0,
                last_message: "nice, ttyl".into(),
                history: vec![
                    // ── yesterday ────────────────────────────────────────
                    Message {
                        author: Author::Them,
                        timestamp: t(yest, 10, 30),
                        body: "hey, are you on the new repeater?".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: t(yest, 10, 31),
                        body: "yep, just hopped on. signal's clean".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: t(yest, 10, 32),
                        body: "how's the mesh holding up?".into(),
                    },
                    // quick burst — these three merge into one block
                    Message {
                        author: Author::Me,
                        timestamp: t(yest, 10, 33),
                        body: "solid".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: t(yest, 10, 33),
                        body: "way better than the old node".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: t(yest, 10, 34),
                        body: "we should drop a third repeater by the ridge".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: t(yest, 10, 36),
                        body: "agreed".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: t(yest, 10, 36),
                        body: "i'll bring the hardware tomorrow".into(),
                    },
                    // ── today ─────────────────────────────────────────────
                    Message {
                        author: Author::Them,
                        timestamp: t(day, 9, 15),
                        body: "did you get the hardware set up?".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: t(day, 9, 17),
                        body: "working on it now".into(),
                    },
                    Message {
                        author: Author::Me,
                        timestamp: t(day, 9, 17),
                        body: "connector was the wrong gauge but i rigged it".into(),
                    },
                    Message {
                        author: Author::Them,
                        timestamp: t(day, 9, 20),
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
                    timestamp: t(yest, 9, 58),
                    body: "heading off-grid, ttyl".into(),
                }],
            },
            Contact {
                name: "carol".into(),
                unread: 5,
                last_message: "ping me when you see this".into(),
                history: vec![Message {
                    author: Author::Them,
                    timestamp: t(day, 10, 40),
                    body: "ping me when you see this".into(),
                }],
            },
            Contact {
                name: "dave".into(),
                unread: 2,
                last_message: "relay node is up".into(),
                history: vec![Message {
                    author: Author::Them,
                    timestamp: t(day, 8, 12),
                    body: "relay node is up on channel 7".into(),
                }],
            },
        ];

        Self {
            running: true,
            contacts,
            selected: 0,
            input: String::new(),
            encrypted: true,
        }
    }

    /// Drive the draw/event loop until the user quits.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.running {
            terminal.draw(|frame| render::render(&self, frame))?;
            events::handle(&mut self)?;
        }
        Ok(())
    }

    /// The contact whose conversation is currently open.
    pub fn selected_contact(&self) -> &Contact {
        &self.contacts[self.selected]
    }

    /// Move the selection to the next contact, wrapping around.
    pub fn next_contact(&mut self) {
        if self.contacts.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.contacts.len();
        self.mark_read();
    }

    /// Move the selection to the previous contact, wrapping around.
    pub fn previous_contact(&mut self) {
        if self.contacts.is_empty() {
            return;
        }
        let len = self.contacts.len();
        self.selected = (self.selected + len - 1) % len;
        self.mark_read();
    }

    /// Send the buffered input as a message to the selected contact.
    pub fn send_message(&mut self) {
        let body = self.input.trim().to_string();
        if body.is_empty() {
            return;
        }
        if let Some(contact) = self.contacts.get_mut(self.selected) {
            contact.history.push(Message {
                author: Author::Me,
                timestamp: datetime::now(),
                body: body.clone(),
            });
            contact.last_message = body;
        }
        self.input.clear();
    }

    /// Stop the event loop.
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Clear the unread badge on the currently selected contact.
    fn mark_read(&mut self) {
        if let Some(contact) = self.contacts.get_mut(self.selected) {
            contact.unread = 0;
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

