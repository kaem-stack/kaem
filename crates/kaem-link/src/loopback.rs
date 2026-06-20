//! In-process loopback transport.
//!
//! Every frame sent is queued for the same node to read back. There is no peer
//! and no network — it exists so the app can run solo and so higher layers can
//! be tested without opening a socket.

use std::collections::VecDeque;

use crate::transport::{Transport, TransportError};

pub struct Loopback {
    inbox: VecDeque<Vec<u8>>,
}

impl Loopback {
    pub fn new() -> Self {
        Self {
            inbox: VecDeque::new(),
        }
    }
}

impl Default for Loopback {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for Loopback {
    fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.inbox.push_back(frame.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(self.inbox.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_loop_back_in_order() {
        let mut radio = Loopback::new();
        assert!(radio.recv().unwrap().is_none());

        radio.send(b"first").unwrap();
        radio.send(b"second").unwrap();

        assert_eq!(radio.recv().unwrap().as_deref(), Some(&b"first"[..]));
        assert_eq!(radio.recv().unwrap().as_deref(), Some(&b"second"[..]));
        assert!(radio.recv().unwrap().is_none());
    }
}
