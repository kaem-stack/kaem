use chrono::Utc;
use tui_input::Input;

use kaem_codec::{WireMessage, decode, encode};
use kaem_radio::Radio;

use crate::core::model::{Author, Contact, Message};

/// The chat domain. It owns the conversation state and drives the link: it
/// encodes outgoing messages onto the radio and folds decoded incoming frames
/// back into the right conversation. It depends only on the [`Radio`] trait, so
/// the transport underneath can be anything the factory builds.
pub struct Chat {
    pub contacts: Vec<Contact>,
    pub selected: usize,
    pub input: Input,
    pub encrypted: bool,
    callsign: String,
    radio: Box<dyn Radio>,
}

impl Chat {
    pub fn new(contacts: Vec<Contact>, radio: Box<dyn Radio>, callsign: String) -> Self {
        Self {
            contacts,
            selected: 0,
            input: Input::default(),
            encrypted: true,
            callsign,
            radio,
        }
    }

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

    /// Send the current input to the selected contact: record it locally, then
    /// encode and transmit it over the radio.
    pub fn send_message(&mut self) {
        let body = self.input.value().trim().to_string();
        if body.is_empty() {
            return;
        }
        self.input = Input::default();

        let to = self.selected_contact().name.clone();
        if let Some(contact) = self.contacts.get_mut(self.selected) {
            contact.history.push(Message {
                author: Author::Me,
                timestamp: Utc::now(),
                body: body.clone(),
            });
            contact.last_message = body.clone();
        }

        let message = WireMessage {
            from: self.callsign.clone(),
            to,
            body,
        };
        // The message is already on screen; a transmit failure must not take the
        // UI down with it. Surfacing link errors is a job for a later status line.
        let _ = self.radio.send(&encode(&message));
    }

    /// Drain frames the radio has received and fold them into the conversation.
    /// Called once per UI tick.
    pub fn poll(&mut self) {
        while let Ok(Some(frame)) = self.radio.recv() {
            let Ok(message) = decode(&frame) else {
                continue; // not a valid kaem frame; drop it
            };
            if message.from == self.callsign {
                continue; // ignore our own broadcast echoed back
            }
            self.receive(message);
        }
    }

    /// Route an incoming message to the contact it came from, creating one if
    /// this node has not been heard from before.
    fn receive(&mut self, message: WireMessage) {
        let idx = match self.contacts.iter().position(|c| c.name == message.from) {
            Some(i) => i,
            None => {
                self.contacts.push(Contact {
                    name: message.from.clone(),
                    unread: 0,
                    last_message: String::new(),
                    history: Vec::new(),
                });
                self.contacts.len() - 1
            }
        };

        let is_open = idx == self.selected;
        let contact = &mut self.contacts[idx];
        contact.history.push(Message {
            author: Author::Them,
            timestamp: Utc::now(),
            body: message.body.clone(),
        });
        contact.last_message = message.body;
        if !is_open {
            contact.unread += 1;
        }
    }

    fn mark_read(&mut self) {
        if let Some(contact) = self.contacts.get_mut(self.selected) {
            contact.unread = 0;
        }
    }
}
