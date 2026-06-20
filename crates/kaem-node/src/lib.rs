//! The steppable chat domain core, with no notion of a transport or a clock.
//!
//! A [`Node`] only knows how to fold a [`Command`] or an incoming frame into
//! its contact list; it hands back the bytes it wants transmitted as
//! [`Outbound`] values and lets the caller decide how (and whether) those
//! bytes actually go out over a link. Time is injected as a virtual
//! millisecond counter ([`Time`]) so the same core can be driven by a live
//! wall clock or a deterministic simulation tick.

mod model;
mod wire;

pub use model::{Author, Contact, Message};
pub use wire::{CodecError, WireMessage, decode, encode};

/// Virtual milliseconds. Callers decide what clock feeds this — a live wall
/// clock for the TUI binary, a stepped counter for a simulation.
pub type Time = u64;

/// A request to fold into the node's state.
pub enum Command {
    Send { to: String, body: String },
}

/// A frame the node wants transmitted. The caller owns the actual transport.
pub struct Outbound(pub Vec<u8>);

/// The chat domain: a callsign and a contact list. Owns no transport, no
/// clock — both are supplied by the caller on every call.
pub struct Node {
    callsign: String,
    contacts: Vec<Contact>,
}

impl Node {
    pub fn new(callsign: impl Into<String>) -> Self {
        Self {
            callsign: callsign.into(),
            contacts: Vec::new(),
        }
    }

    pub fn callsign(&self) -> &str {
        &self.callsign
    }

    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }

    pub fn contacts_mut(&mut self) -> &mut Vec<Contact> {
        &mut self.contacts
    }

    /// Reset unread count for the contact at `idx`, if it exists.
    pub fn mark_read(&mut self, idx: usize) {
        if let Some(contact) = self.contacts.get_mut(idx) {
            contact.unread = 0;
        }
    }

    /// Fold a command into the node's state, returning any frames it wants
    /// transmitted as a result.
    pub fn command(&mut self, cmd: Command, now: Time) -> Vec<Outbound> {
        match cmd {
            Command::Send { to, body } => self.send(to, body, now),
        }
    }

    fn send(&mut self, to: String, body: String, now: Time) -> Vec<Outbound> {
        if body.is_empty() {
            return Vec::new();
        }

        self.record_sent(to.clone(), body.clone(), now);

        let message = WireMessage {
            from: self.callsign.clone(),
            to,
            body,
        };
        vec![Outbound(encode(&message))]
    }

    /// Record a message this node sent in the recipient's history, without
    /// producing any wire frame. Callers that build their own envelope around
    /// the payload (e.g. an encrypted mesh relay) use this to keep the
    /// sender's own chat history in sync, mirroring what [`Node::command`]'s
    /// `Send` arm does internally.
    pub fn record_sent(&mut self, to: impl Into<String>, body: impl Into<String>, now: Time) {
        let to = to.into();
        let body = body.into();

        let idx = self.find_or_create(&to);
        let contact = &mut self.contacts[idx];
        contact.history.push(Message {
            author: Author::Me,
            timestamp: now,
            body: body.clone(),
        });
        contact.last_message = body;
    }

    /// Decode a frame received from the link and fold it into the
    /// conversation it belongs to. Malformed frames and our own echoed
    /// broadcasts are silently dropped.
    pub fn on_frame(&mut self, frame: &[u8], now: Time) {
        let Ok(message) = decode(frame) else {
            return; // not a valid kaem frame; drop it
        };
        if message.from == self.callsign {
            return; // ignore our own broadcast echoed back
        }
        self.receive(message, now);
    }

    fn receive(&mut self, message: WireMessage, now: Time) {
        self.record_received(message.from, message.body, now);
    }

    /// Record a message this node received from `from`, without decoding a
    /// frame itself. Callers that already hold a decoded payload (e.g. a mesh
    /// relay that decrypted an envelope and decoded the `WireMessage` inside
    /// it) use this to fold it into the contact's history directly, mirroring
    /// what [`Node::on_frame`] does internally.
    pub fn record_received(&mut self, from: impl Into<String>, body: impl Into<String>, now: Time) {
        let from = from.into();
        let body = body.into();

        let idx = self.find_or_create(&from);
        let contact = &mut self.contacts[idx];
        contact.history.push(Message {
            author: Author::Them,
            timestamp: now,
            body: body.clone(),
        });
        contact.last_message = body;
        contact.unread += 1;
    }

    fn find_or_create(&mut self, name: &str) -> usize {
        match self.contacts.iter().position(|c| c.name == name) {
            Some(i) => i,
            None => {
                self.contacts.push(Contact {
                    name: name.to_string(),
                    unread: 0,
                    last_message: String::new(),
                    history: Vec::new(),
                });
                self.contacts.len() - 1
            }
        }
    }

    /// Advance the node by one tick. No retransmit/keepalive logic yet — this
    /// is a placeholder for the simulation's per-tick drain.
    pub fn tick(&mut self, _now: Time) -> Vec<Outbound> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_round_trips_into_a_new_contact() {
        let mut alice = Node::new("alice");
        let mut bob = Node::new("bob");

        let outbound = alice.command(
            Command::Send {
                to: "bob".into(),
                body: "hi".into(),
            },
            0,
        );
        assert_eq!(outbound.len(), 1);

        bob.on_frame(&outbound[0].0, 1);

        let contact = bob
            .contacts()
            .iter()
            .find(|c| c.name == "alice")
            .expect("alice contact created");
        assert_eq!(contact.unread, 1);
        let last = contact.history.last().expect("history has an entry");
        assert_eq!(last.author, Author::Them);
        assert_eq!(last.body, "hi");
        assert_eq!(last.timestamp, 1);
    }

    #[test]
    fn own_echo_is_dropped() {
        let mut alice = Node::new("alice");
        let outbound = alice.command(
            Command::Send {
                to: "bob".into(),
                body: "hi".into(),
            },
            0,
        );

        alice.on_frame(&outbound[0].0, 2);

        // No "bob" contact gets created from our own echo, and "bob" (the
        // contact we sent to) shows no incoming history.
        assert!(alice.contacts().iter().all(|c| c.name != "alice"));
        let bob = alice
            .contacts()
            .iter()
            .find(|c| c.name == "bob")
            .expect("bob contact exists from the send");
        assert_eq!(bob.history.len(), 1);
        assert_eq!(bob.history[0].author, Author::Me);
    }

    #[test]
    fn empty_body_produces_no_outbound() {
        let mut alice = Node::new("alice");
        let outbound = alice.command(
            Command::Send {
                to: "bob".into(),
                body: String::new(),
            },
            0,
        );
        assert!(outbound.is_empty());
    }
}
