use chrono::Utc;
use tui_input::Input;

use crate::core::model::{Author, Contact, Message};

pub struct Chat {
    pub contacts: Vec<Contact>,
    pub selected: usize,
    pub input: Input,
    pub encrypted: bool,
}

impl Chat {
    pub fn new(contacts: Vec<Contact>) -> Self {
        Self {
            contacts,
            selected: 0,
            input: Input::default(),
            encrypted: true,
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
