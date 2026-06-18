use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    Me,
    Them,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub author: Author,
    pub timestamp: DateTime<Utc>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct Contact {
    pub name: String,
    pub unread: u32,
    pub last_message: String,
    pub history: Vec<Message>,
}
