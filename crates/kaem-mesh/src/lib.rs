//! The encrypted flood-relay mesh layer: composes around [`kaem_node::Node`]
//! to add chatroom pairing and envelope relay, without changing `Node`'s own
//! command/frame surface (the live `kaem` TUI binary depends on that surface
//! directly and must keep compiling unchanged).
//!
//! Nodes start as strangers. [`MeshNode::begin_pairing`] /
//! [`MeshNode::finish_pairing`] mint a shared chatroom key via
//! [`crate::pairing::handshake`]. [`MeshNode::send`] wraps a [`WireMessage`] in
//! an [`Envelope`] sealed under the chatroom key; [`MeshNode::on_frame`]
//! decodes any envelope it hears, decrypts and folds it in if it recognizes
//! the chatroom, and — regardless — rebroadcasts it with a decremented TTL so
//! traffic can cross multiple hops through nodes that can't read it at all.

use std::collections::{HashSet, VecDeque};

use kaem_node::{Contact, Node, Outbound, Time, WireMessage, decode, encode};

mod crypto;
mod envelope;
mod event;
mod keys;
mod pairing;
mod symmetric;

use crate::envelope::{Envelope, decode_envelope, encode_envelope};
use crate::pairing::{Chatroom, Identity, Store, generate_identity, handshake};

/// Hop budget for a freshly sent message.
const DEFAULT_TTL: u8 = 8;

/// How many recently relayed message ids to remember for dedup.
const SEEN_CAPACITY: usize = 256;

/// A bounded "recently relayed" set: a `VecDeque` for eviction order plus a
/// `HashSet` for O(1) membership checks.
struct SeenIds {
    order: VecDeque<u64>,
    set: HashSet<u64>,
}

impl SeenIds {
    fn new() -> Self {
        Self {
            order: VecDeque::with_capacity(SEEN_CAPACITY),
            set: HashSet::with_capacity(SEEN_CAPACITY),
        }
    }

    fn contains(&self, id: u64) -> bool {
        self.set.contains(&id)
    }

    fn record(&mut self, id: u64) {
        if self.set.contains(&id) {
            return;
        }
        if self.order.len() >= SEEN_CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.set.remove(&oldest);
        }
        self.order.push_back(id);
        self.set.insert(id);
    }
}

/// A node in the encrypted mesh: a [`kaem_node::Node`] chat core, an identity
/// keypair, and the chatroom store that turns "stranger" into "paired peer".
pub struct MeshNode {
    inner: Node,
    identity: Identity,
    store: Store,
    seen: SeenIds,
}

impl MeshNode {
    pub fn new(callsign: impl Into<String>) -> Self {
        Self {
            inner: Node::new(callsign),
            identity: generate_identity().expect("ml-kem-768 keygen must succeed"),
            store: Store::open_in_memory().expect("in-memory sqlite must open"),
            seen: SeenIds::new(),
        }
    }

    pub fn public_key(&self) -> &[u8] {
        &self.identity.public_key
    }

    pub fn callsign(&self) -> &str {
        self.inner.callsign()
    }

    pub fn contacts(&self) -> &[Contact] {
        self.inner.contacts()
    }

    pub fn is_paired_with(&self, peer: &str) -> bool {
        self.store.find_by_peer(peer).is_some()
    }

    /// Callsigns of every peer this node has paired with — the contact list.
    pub fn paired_peers(&self) -> Vec<String> {
        self.store.list().into_iter().map(|c| c.peer).collect()
    }

    /// Mint a new chatroom for pairing with `peer` (identified here by its
    /// public key), insert it into this node's own store under `peer`'s
    /// callsign, and return the bytes sealed for the peer to recover the
    /// same chatroom via [`finish_pairing`].
    pub fn begin_pairing(&mut self, peer: &str, peer_pubkey: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (chatroom_id, key, sealed) = handshake::pair(&self.identity, peer_pubkey)?;
        self.store.insert(&Chatroom {
            id: chatroom_id,
            peer: peer.to_string(),
            key,
        })?;
        Ok(sealed)
    }

    /// Recover the chatroom minted by a peer's [`begin_pairing`] call and
    /// insert it into this node's store under `peer`'s callsign, completing
    /// the pairing.
    pub fn finish_pairing(&mut self, peer: &str, sealed: &[u8]) -> anyhow::Result<()> {
        let (chatroom_id, key) = handshake::accept(&self.identity, sealed)?;
        self.store.insert(&Chatroom {
            id: chatroom_id,
            peer: peer.to_string(),
            key,
        })?;
        Ok(())
    }

    /// Seal `body` for `to` under their shared chatroom key and return the
    /// envelope to transmit. A no-op (empty result) if `to` isn't paired.
    pub fn send(&mut self, to: &str, body: String, now: Time) -> Vec<Outbound> {
        let Some(chatroom) = self.store.find_by_peer(to) else {
            return Vec::new(); // unpaired — nothing we can encrypt this under
        };

        let message = WireMessage {
            from: self.callsign().to_string(),
            to: to.to_string(),
            body: body.clone(),
        };
        let plaintext = encode(&message);
        let ciphertext = crate::symmetric::seal(&chatroom.key, &plaintext);

        let envelope = Envelope {
            chatroom_id: chatroom.id,
            message_id: rand::random(),
            ttl: DEFAULT_TTL,
            ciphertext,
        };

        self.inner.record_sent(to, body, now);

        vec![Outbound(encode_envelope(&envelope))]
    }

    /// Fold an incoming envelope into this node's state if it's addressed to
    /// a chatroom this node recognizes, and — regardless of whether it could
    /// be read — relay it onward with a decremented TTL, as long as hops
    /// remain and it hasn't already been relayed.
    pub fn on_frame(&mut self, frame: &[u8], now: Time) -> Vec<Outbound> {
        let Ok(envelope) = decode_envelope(frame) else {
            return Vec::new(); // not a valid envelope frame; drop it
        };

        if self.seen.contains(envelope.message_id) {
            return Vec::new(); // already relayed this one; stop the flood
        }
        self.seen.record(envelope.message_id);

        self.try_decrypt_and_fold(&envelope, now);

        if envelope.ttl == 0 {
            return Vec::new();
        }

        let relayed = Envelope {
            chatroom_id: envelope.chatroom_id,
            message_id: envelope.message_id,
            ttl: envelope.ttl - 1,
            ciphertext: envelope.ciphertext,
        };
        vec![Outbound(encode_envelope(&relayed))]
    }

    fn try_decrypt_and_fold(&mut self, envelope: &Envelope, now: Time) {
        let Some(chatroom) = self.store.lookup(envelope.chatroom_id) else {
            return; // not our chatroom — can't read it, nothing to fold
        };
        let Ok(plaintext) = crate::symmetric::open(&chatroom.key, &envelope.ciphertext) else {
            return; // wrong key or corrupted — drop silently
        };
        let Ok(message) = decode(&plaintext) else {
            return; // not a valid WireMessage inside — drop silently
        };
        if message.from == self.callsign() {
            return; // ignore our own message echoed back
        }
        self.inner.record_received(message.from, message.body, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: &mut MeshNode, b: &mut MeshNode) {
        let a_name = a.callsign().to_string();
        let b_name = b.callsign().to_string();
        let b_pubkey = b.public_key().to_vec();

        let sealed = a
            .begin_pairing(&b_name, &b_pubkey)
            .expect("alice mints the chatroom");
        b.finish_pairing(&a_name, &sealed).expect("bob accepts");
    }

    #[test]
    fn unpaired_send_is_a_no_op() {
        let mut alice = MeshNode::new("alice");
        let outbound = alice.send("charlie", "hi".to_string(), 0);
        assert!(outbound.is_empty());
    }

    #[test]
    fn pairing_is_mutual() {
        let mut alice = MeshNode::new("alice");
        let mut charlie = MeshNode::new("charlie");
        pair(&mut alice, &mut charlie);

        assert!(alice.is_paired_with("charlie"));
        assert!(charlie.is_paired_with("alice"));
    }

    #[test]
    fn alice_to_charlie_via_bob_relay_scenario() {
        let mut alice = MeshNode::new("alice");
        let mut bob = MeshNode::new("bob"); // never paired with anyone
        let mut charlie = MeshNode::new("charlie");

        pair(&mut alice, &mut charlie);

        // alice -> charlie, but bob is the only one in range, so the frame
        // physically goes to bob first.
        let outbound = alice.send("charlie", "hi".to_string(), 0);
        assert_eq!(outbound.len(), 1);
        let envelope_from_alice = &outbound[0].0;

        // bob can't decrypt it (never paired), but still relays it with ttl-1.
        let relayed_by_bob = bob.on_frame(envelope_from_alice, 1);
        assert_eq!(relayed_by_bob.len(), 1);
        assert!(bob.contacts().is_empty());

        let decoded = decode_envelope(&relayed_by_bob[0].0).expect("valid envelope");
        let original = decode_envelope(envelope_from_alice).expect("valid envelope");
        assert_eq!(decoded.ttl, original.ttl - 1);

        // charlie hears bob's relay, decrypts it, and folds "hi" in.
        let from_charlie = charlie.on_frame(&relayed_by_bob[0].0, 2);
        let contact = charlie
            .contacts()
            .iter()
            .find(|c| c.name == "alice")
            .expect("alice contact created");
        assert_eq!(contact.history.last().unwrap().body, "hi");

        // DEFAULT_TTL is 8: alice sends ttl=8, bob relays ttl=7, so charlie's
        // copy still has hops left and charlie itself relays it onward.
        assert_eq!(decoded.ttl, 7);
        assert_eq!(from_charlie.len(), 1);
    }

    #[test]
    fn duplicate_envelope_is_relayed_only_once() {
        let mut bob = MeshNode::new("bob");
        let envelope = Envelope {
            chatroom_id: 1,
            message_id: 42,
            ttl: 3,
            ciphertext: b"opaque".to_vec(),
        };
        let frame = encode_envelope(&envelope);

        let first = bob.on_frame(&frame, 0);
        assert_eq!(first.len(), 1);

        let second = bob.on_frame(&frame, 1);
        assert!(second.is_empty());
    }

    #[test]
    fn zero_ttl_envelope_is_not_relayed() {
        let mut bob = MeshNode::new("bob");
        let envelope = Envelope {
            chatroom_id: 1,
            message_id: 7,
            ttl: 0,
            ciphertext: b"opaque".to_vec(),
        };
        let frame = encode_envelope(&envelope);

        let outbound = bob.on_frame(&frame, 0);
        assert!(outbound.is_empty());
    }

    #[test]
    fn send_records_own_history_immediately() {
        let mut alice = MeshNode::new("alice");
        let mut charlie = MeshNode::new("charlie");
        pair(&mut alice, &mut charlie);

        alice.send("charlie", "hello".to_string(), 0);

        let contact = alice
            .contacts()
            .iter()
            .find(|c| c.name == "charlie")
            .expect("charlie contact created by send");
        assert_eq!(contact.history.last().unwrap().body, "hello");
    }
}
