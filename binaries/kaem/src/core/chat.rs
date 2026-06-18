use tui_input::Input;

use kaem_node::{Command, Contact, Node};
use kaem_transport::Transport;

/// The view-layer wrapper around the steppable [`Node`] core. It holds the UI
/// state that has no place in the domain (which contact is open, the
/// in-flight input buffer, the encrypted indicator) plus the transport and
/// the clock — neither of which the node is allowed to own.
pub struct Chat {
    pub node: Node,
    pub selected: usize,
    pub input: Input,
    pub encrypted: bool,
    transport: Box<dyn Transport>,
}

impl Chat {
    pub fn new(contacts: Vec<Contact>, transport: Box<dyn Transport>, callsign: String) -> Self {
        let mut node = Node::new(callsign);
        *node.contacts_mut() = contacts;
        Self {
            node,
            selected: 0,
            input: Input::default(),
            encrypted: true,
            transport,
        }
    }

    pub fn contacts(&self) -> &[Contact] {
        self.node.contacts()
    }

    pub fn selected_contact(&self) -> &Contact {
        &self.node.contacts()[self.selected]
    }

    pub fn next_contact(&mut self) {
        if self.node.contacts().is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.node.contacts().len();
        self.node.mark_read(self.selected);
    }

    pub fn previous_contact(&mut self) {
        if self.node.contacts().is_empty() {
            return;
        }
        let len = self.node.contacts().len();
        self.selected = (self.selected + len - 1) % len;
        self.node.mark_read(self.selected);
    }

    /// Send the current input to the selected contact: fold it into the node
    /// (records it locally and encodes it), then transmit every resulting
    /// frame over the radio.
    pub fn send_message(&mut self) {
        let body = self.input.value().trim().to_string();
        if body.is_empty() {
            return;
        }
        self.input = Input::default();

        let to = self.selected_contact().name.clone();
        let now = now_ms();
        let outbound = self.node.command(Command::Send { to, body }, now);
        for out in outbound {
            // The message is already on screen; a transmit failure must not
            // take the UI down with it. Surfacing link errors is a job for a
            // later status line.
            let _ = self.transport.send(&out.0);
        }
    }

    /// Drain frames the radio has received and fold them into the
    /// conversation. Called once per UI tick. The node always bumps `unread`
    /// on an incoming message; if it landed on the currently open contact,
    /// mark it read immediately so an open conversation never shows unread,
    /// matching the previous behavior.
    pub fn poll(&mut self) {
        let now = now_ms();
        while let Ok(Some(frame)) = self.transport.recv() {
            self.node.on_frame(&frame, now);
        }
        self.node.mark_read(self.selected);
    }
}

fn now_ms() -> kaem_node::Time {
    chrono::Utc::now().timestamp_millis() as u64
}
