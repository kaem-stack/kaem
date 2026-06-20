//! Per-node chatroom membership, persisted in an isolated in-memory SQLite
//! database. Each [`Store`] belongs to exactly one node — there is no sharing
//! across nodes, mirroring how each node only ever knows its own chatroom
//! keys.

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::store_ops::ChatroomStore;

/// One paired chatroom: the shared symmetric key and which peer it was
/// established with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chatroom {
    pub id: u64,
    pub peer: String,
    pub key: [u8; 32],
}

/// A node's chatroom membership table, backed by an isolated SQLite
/// connection.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open a fresh, isolated in-memory store — one per node.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE chatrooms (
                id   INTEGER PRIMARY KEY,
                peer TEXT NOT NULL,
                key  BLOB NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    /// Insert (or replace) a chatroom row.
    pub fn insert(&self, chatroom: &Chatroom) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO chatrooms (id, peer, key) VALUES (?1, ?2, ?3)",
            params![chatroom.id as i64, chatroom.peer, chatroom.key.as_slice()],
        )?;
        Ok(())
    }

    /// Look up a chatroom by its public id.
    pub fn lookup(&self, id: u64) -> Option<Chatroom> {
        self.conn
            .query_row(
                "SELECT id, peer, key FROM chatrooms WHERE id = ?1",
                params![id as i64],
                row_to_chatroom,
            )
            .ok()
    }

    /// Look up a chatroom by peer callsign.
    pub fn find_by_peer(&self, name: &str) -> Option<Chatroom> {
        self.conn
            .query_row(
                "SELECT id, peer, key FROM chatrooms WHERE peer = ?1",
                params![name],
                row_to_chatroom,
            )
            .ok()
    }

    /// All chatrooms this node currently belongs to.
    pub fn list(&self) -> Vec<Chatroom> {
        let mut stmt = match self.conn.prepare("SELECT id, peer, key FROM chatrooms") {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], row_to_chatroom);
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl ChatroomStore for Store {
    fn insert(&self, chatroom: &Chatroom) -> Result<()> {
        Store::insert(self, chatroom)
    }

    fn lookup(&self, id: u64) -> Option<Chatroom> {
        Store::lookup(self, id)
    }

    fn find_by_peer(&self, name: &str) -> Option<Chatroom> {
        Store::find_by_peer(self, name)
    }

    fn list(&self) -> Vec<Chatroom> {
        Store::list(self)
    }
}

fn row_to_chatroom(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chatroom> {
    let id: i64 = row.get(0)?;
    let peer: String = row.get(1)?;
    let key_bytes: Vec<u8> = row.get(2)?;
    let key: [u8; 32] = key_bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "chatroom key must be exactly 32 bytes",
            )),
        )
    })?;
    Ok(Chatroom {
        id: id as u64,
        peer,
        key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: u64, peer: &str) -> Chatroom {
        let mut key = [0u8; 32];
        key[0] = id as u8;
        Chatroom {
            id,
            peer: peer.to_string(),
            key,
        }
    }

    #[test]
    fn insert_then_lookup_round_trips() {
        let store = Store::open_in_memory().expect("open");
        let room = sample(42, "bob");
        store.insert(&room).expect("insert");

        assert_eq!(store.lookup(42), Some(room));
    }

    #[test]
    fn lookup_of_unknown_id_is_none() {
        let store = Store::open_in_memory().expect("open");
        assert_eq!(store.lookup(999), None);
    }

    #[test]
    fn find_by_peer_locates_the_right_room() {
        let store = Store::open_in_memory().expect("open");
        store.insert(&sample(1, "alice")).expect("insert");
        store.insert(&sample(2, "bob")).expect("insert");

        let found = store.find_by_peer("bob").expect("found");
        assert_eq!(found.id, 2);
    }

    #[test]
    fn list_returns_every_chatroom() {
        let store = Store::open_in_memory().expect("open");
        store.insert(&sample(1, "alice")).expect("insert");
        store.insert(&sample(2, "bob")).expect("insert");

        let mut rooms = store.list();
        rooms.sort_by_key(|r| r.id);
        assert_eq!(rooms.len(), 2);
        assert_eq!(rooms[0].peer, "alice");
        assert_eq!(rooms[1].peer, "bob");
    }

    #[test]
    fn stores_are_isolated_per_instance() {
        let a = Store::open_in_memory().expect("open");
        let b = Store::open_in_memory().expect("open");
        a.insert(&sample(1, "alice")).expect("insert");

        assert!(a.lookup(1).is_some());
        assert!(b.lookup(1).is_none());
    }
}
