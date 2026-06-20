//! The persistence seam: every chatroom-membership operation [`MeshNode`]
//! needs, behind a trait it owns. `kaem-mesh` carries one concrete
//! implementation, the SQLite-backed [`Store`](crate::pairing::Store), but a
//! binary may hand in another (e.g. an on-disk store) the same way it hands
//! in a [`CryptoOps`](crate::CryptoOps) — via [`MeshNode::new`](crate::MeshNode::new).
//! The surface here is deliberately opaque (owned structs, no `rusqlite`
//! types) so a non-sqlite implementation can slot in without this crate
//! noticing.

use anyhow::Result;

use crate::pairing::Chatroom;

pub trait ChatroomStore {
    /// Insert (or replace) a chatroom row.
    fn insert(&self, chatroom: &Chatroom) -> Result<()>;
    /// Look up a chatroom by its public id.
    fn lookup(&self, id: u64) -> Option<Chatroom>;
    /// Look up a chatroom by peer callsign.
    fn find_by_peer(&self, name: &str) -> Option<Chatroom>;
    /// All chatrooms this node currently belongs to.
    fn list(&self) -> Vec<Chatroom>;
}
