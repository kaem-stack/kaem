//! The encrypted flood-relay mesh: chatroom pairing plus a bytes-in/bytes-out
//! relay. It knows nothing about chat — it seals and opens opaque payloads and
//! floods envelopes across hops. A binary composes it with a chat core and a
//! link: outgoing payloads are [`seal`](MeshNode::seal)ed here and transmitted;
//! inbound frames go through [`on_frame`](MeshNode::on_frame), which hands back
//! any plaintext for the chat layer to fold in and any envelope to relay.
//!
//! Nodes start as strangers. [`MeshNode::begin_pairing`] /
//! [`MeshNode::finish_pairing`] mint a shared chatroom key via the pairing
//! handshake. [`MeshNode::seal`] wraps an opaque payload in an [`Envelope`]
//! under the chatroom key; [`MeshNode::on_frame`] decodes any envelope it
//! hears, decrypts it if it recognizes the chatroom, and — regardless — reports
//! a decremented-TTL relay so traffic can cross nodes that can't read it.

use std::collections::{HashSet, VecDeque};

mod crypto_ops;
mod envelope;
pub mod pairing;
mod store_ops;
#[cfg(test)]
mod test_support;

pub use crypto_ops::{CryptoOps, KeyPair};
pub use store_ops::ChatroomStore;

use envelope::{Envelope, decode_envelope, encode_envelope};
use pairing::{Chatroom, Identity, generate_identity, handshake};

/// Hop budget for a freshly sealed message. Effectively unbounded: dedup via
/// `seen` (each node relays a given `message_id` at most once) already
/// prevents relay loops/storms regardless of TTL, so the hop count itself
/// shouldn't be the thing that lets a message die before it has reached
/// every reachable node — `u8::MAX` hops is far beyond any mesh diameter
/// this protocol will see.
const DEFAULT_TTL: u8 = u8::MAX;

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

/// What a node extracts from an inbound frame: the plaintext it could read (if
/// any) and the frame it should rebroadcast (if any). The two are independent —
/// a node relays envelopes it can't decrypt, and never relays one whose hops
/// are spent.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Inbound {
    /// Decrypted payload, if this node holds the chatroom key and the envelope
    /// opened cleanly. Opaque bytes — the caller decodes them.
    pub payload: Option<Vec<u8>>,
    /// The envelope to rebroadcast (same frame, TTL decremented), if it still
    /// had hops left and hadn't been relayed before.
    pub relay: Option<Vec<u8>>,
}

/// A node in the encrypted mesh: an identity keypair, the chatroom store that
/// turns "stranger" into "paired peer", a bounded relay-dedup set, and the
/// crypto backend everything above is sealed with.
pub struct MeshNode {
    identity: Identity,
    store: Box<dyn ChatroomStore>,
    seen: SeenIds,
    crypto: Box<dyn CryptoOps>,
}

impl MeshNode {
    /// Build a node backed by `crypto` for every keygen/seal/open it needs,
    /// and `store` for its chatroom membership. A binary supplies both
    /// implementations (crypto typically backed by `kaem-crypto`, store
    /// typically the SQLite-backed [`pairing::Store`]) — this crate never
    /// depends on a crypto or persistence crate directly.
    pub fn new(crypto: Box<dyn CryptoOps>, store: Box<dyn ChatroomStore>) -> Self {
        let identity = generate_identity(crypto.as_ref()).expect("keygen must succeed");
        Self {
            identity,
            store,
            seen: SeenIds::new(),
            crypto,
        }
    }

    /// This node's ML-KEM-768 public key — handed to a peer to pair against.
    pub fn public_key(&self) -> &[u8] {
        &self.identity.public_key
    }

    /// Whether this node holds a chatroom shared with `peer`.
    pub fn is_paired_with(&self, peer: &str) -> bool {
        self.store.find_by_peer(peer).is_some()
    }

    /// Callsigns of every peer this node has paired with.
    pub fn paired_peers(&self) -> Vec<String> {
        self.store.list().into_iter().map(|c| c.peer).collect()
    }

    /// Mint a new chatroom for pairing with `peer` (identified here by its
    /// public key), insert it into this node's own store under `peer`'s
    /// callsign, and return the bytes sealed for the peer to recover the same
    /// chatroom via [`finish_pairing`](Self::finish_pairing).
    pub fn begin_pairing(&mut self, peer: &str, peer_pubkey: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (chatroom_id, key, sealed) =
            handshake::pair(self.crypto.as_ref(), &self.identity, peer_pubkey)?;
        self.store.insert(&Chatroom {
            id: chatroom_id,
            peer: peer.to_string(),
            key,
        })?;
        Ok(sealed)
    }

    /// Recover the chatroom minted by a peer's
    /// [`begin_pairing`](Self::begin_pairing) call and insert it under `peer`'s
    /// callsign, completing the pairing.
    pub fn finish_pairing(&mut self, peer: &str, sealed: &[u8]) -> anyhow::Result<()> {
        let (chatroom_id, key) = handshake::accept(self.crypto.as_ref(), &self.identity, sealed)?;
        self.store.insert(&Chatroom {
            id: chatroom_id,
            peer: peer.to_string(),
            key,
        })?;
        Ok(())
    }

    /// Seal an opaque `payload` for paired `peer` into a relayable envelope
    /// frame. `None` if `peer` isn't paired — there's no key to seal under.
    pub fn seal(&self, to: &str, payload: &[u8]) -> Option<Vec<u8>> {
        let chatroom = self.store.find_by_peer(to)?;
        let envelope = Envelope {
            chatroom_id: chatroom.id,
            message_id: rand::random(),
            ttl: DEFAULT_TTL,
            ciphertext: self.crypto.symmetric_seal(&chatroom.key, payload),
        };
        Some(encode_envelope(&envelope))
    }

    /// Process an inbound frame: decrypt it if it's addressed to a chatroom we
    /// hold, and report a decremented-TTL relay if it still has hops and we
    /// haven't relayed it before. A frame we've already seen — or that isn't a
    /// valid envelope — yields an empty [`Inbound`].
    pub fn on_frame(&mut self, frame: &[u8]) -> Inbound {
        let Ok(envelope) = decode_envelope(frame) else {
            return Inbound::default(); // not a valid envelope frame; drop it
        };
        if self.seen.contains(envelope.message_id) {
            return Inbound::default(); // already relayed this one; stop the flood
        }
        self.seen.record(envelope.message_id);

        let payload = self.try_open(&envelope);

        // Relay regardless of whether we could read it, as long as hops remain.
        let relay = (envelope.ttl > 0).then(|| {
            encode_envelope(&Envelope {
                chatroom_id: envelope.chatroom_id,
                message_id: envelope.message_id,
                ttl: envelope.ttl - 1,
                ciphertext: envelope.ciphertext,
            })
        });

        Inbound { payload, relay }
    }

    /// Decrypt an envelope addressed to a chatroom we hold; `None` if it isn't
    /// ours or the ciphertext doesn't open under our key.
    fn try_open(&self, envelope: &Envelope) -> Option<Vec<u8>> {
        let chatroom = self.store.lookup(envelope.chatroom_id)?;
        self.crypto
            .symmetric_open(&chatroom.key, &envelope.ciphertext)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: &mut MeshNode, a_name: &str, b: &mut MeshNode, b_name: &str) {
        let b_pubkey = b.public_key().to_vec();
        let sealed = a.begin_pairing(b_name, &b_pubkey).expect("mint chatroom");
        b.finish_pairing(a_name, &sealed).expect("accept chatroom");
    }

    /// A node backed by the test crypto stub and a fresh in-memory store —
    /// what every test below needs and nothing more.
    fn test_node() -> MeshNode {
        MeshNode::new(
            Box::new(crate::test_support::TestCrypto),
            Box::new(pairing::Store::open_in_memory().expect("in-memory sqlite must open")),
        )
    }

    #[test]
    fn unpaired_seal_yields_nothing() {
        let alice = test_node();
        assert!(alice.seal("charlie", b"hi").is_none());
    }

    #[test]
    fn pairing_is_mutual() {
        let mut alice = test_node();
        let mut charlie = test_node();
        pair(&mut alice, "alice", &mut charlie, "charlie");

        assert!(alice.is_paired_with("charlie"));
        assert!(charlie.is_paired_with("alice"));
        assert_eq!(charlie.paired_peers(), vec!["alice".to_string()]);
    }

    #[test]
    fn paired_peer_opens_what_we_seal() {
        let mut alice = test_node();
        let mut charlie = test_node();
        pair(&mut alice, "alice", &mut charlie, "charlie");

        let frame = alice
            .seal("charlie", b"meet at the repeater")
            .expect("paired");
        let inbound = charlie.on_frame(&frame);
        assert_eq!(
            inbound.payload.as_deref(),
            Some(&b"meet at the repeater"[..])
        );
    }

    #[test]
    fn relays_through_a_node_that_cannot_read_it() {
        let mut alice = test_node();
        let mut bob = test_node(); // never paired with anyone
        let mut charlie = test_node();
        pair(&mut alice, "alice", &mut charlie, "charlie");

        // alice -> charlie, but bob is the only one in range first.
        let from_alice = alice.seal("charlie", b"hi").expect("paired");
        let original = decode_envelope(&from_alice).unwrap();

        // bob can't decrypt it, but still relays it with ttl-1.
        let at_bob = bob.on_frame(&from_alice);
        assert!(at_bob.payload.is_none());
        let relay = at_bob.relay.expect("bob relays onward");
        assert_eq!(decode_envelope(&relay).unwrap().ttl, original.ttl - 1);

        // charlie hears bob's relay and opens it.
        let at_charlie = charlie.on_frame(&relay);
        assert_eq!(at_charlie.payload.as_deref(), Some(&b"hi"[..]));
    }

    #[test]
    fn duplicate_envelope_is_relayed_only_once() {
        let mut bob = test_node();
        let frame = encode_envelope(&Envelope {
            chatroom_id: 1,
            message_id: 42,
            ttl: 3,
            ciphertext: b"opaque".to_vec(),
        });

        assert!(bob.on_frame(&frame).relay.is_some());
        assert!(bob.on_frame(&frame).relay.is_none());
    }

    #[test]
    fn zero_ttl_envelope_is_not_relayed() {
        let mut bob = test_node();
        let frame = encode_envelope(&Envelope {
            chatroom_id: 1,
            message_id: 7,
            ttl: 0,
            ciphertext: b"opaque".to_vec(),
        });

        assert!(bob.on_frame(&frame).relay.is_none());
    }
}
