//! Identity + chatroom membership for the encrypted pairing/mesh layer.
//!
//! Nodes start as strangers. [`handshake::pair`] mints a chatroom (a random
//! id plus a random symmetric key) and seals it for a specific peer's
//! ML-KEM-768 public key, via a real KEM encapsulation
//! ([`kaem_crypto::crypto`]); [`handshake::accept`] is the matching peer-side
//! recovery. Both sides then hold a [`store::Chatroom`] row keyed by the same
//! id — that row is what lets a node recognize and decrypt mesh envelopes
//! addressed to that chatroom, and what `kaem-mesh` queries on every send and
//! every received envelope.

pub mod handshake;
pub mod identity;
pub mod store;

pub use identity::{Identity, generate_identity};
pub use store::{Chatroom, Store};
