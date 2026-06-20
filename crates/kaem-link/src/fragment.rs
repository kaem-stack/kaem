//! Message fragmentation for the radio link.
//!
//! A real over-the-air frame has a small MTU — far smaller than a sealed chat
//! envelope — so a frame that doesn't fit must travel as several smaller,
//! ordered pieces and be rebuilt on the far side. That's a link-layer concern:
//! it lives here, beneath [`Transport`](crate::Transport), so the chat and mesh
//! layers above keep handing whole frames down and never learn the air has a
//! size limit.
//!
//! Wire shape of one fragment (the modem wraps this with its own length + CRC,
//! so a corrupted fragment is dropped before it ever reaches the reassembler):
//!
//! ```text
//! +---------+-------+-------+--------+
//! | msg_id  | total | index | chunk  |
//! | 2       | 2     | 2     | n      |
//! +---------+-------+-------+--------+
//! ```
//!
//! * **msg_id** — ties the fragments of one frame together; a wrapping counter,
//!   distinct enough that two in-flight frames don't collide.
//! * **total / index** — let the receiver size its buffer and place each piece,
//!   so fragments may arrive out of order or duplicated and still reassemble.
//!
//! Reassembly is **best-effort**, matching the rest of the link: a fragment the
//! channel drops simply leaves its message incomplete forever. To keep a never-
//! completed message from leaking memory, the reassembler bounds the number of
//! half-assembled messages it holds ([`MAX_PENDING`]) and evicts the lot once
//! that's exceeded — a count-based cap, so this stays clock-free and honours
//! `Transport`'s non-blocking contract.

use std::collections::HashMap;

/// `msg_id(2) + total(2) + index(2)`.
const HEADER: usize = 6;

/// Default bytes of the caller's frame carried per fragment (the header is on
/// top of this). Small enough that a normal sealed envelope splits into a
/// couple of over-the-air frames, which is the whole point of fragmenting.
pub(crate) const DEFAULT_MAX_PAYLOAD: usize = 64;

/// Cap on half-assembled messages held at once; the oldest set is dropped when
/// exceeded so a fragment lost to the channel can't leak memory. Mirrors the
/// IQ-reassembly cap in [`UdpChannel`](crate::channel).
const MAX_PENDING: usize = 16;

/// Splits whole frames into ordered, sequence-tagged fragments. Owns the
/// wrapping `msg_id` counter so each frame's fragments share an id.
pub(crate) struct Fragmenter {
    max_payload: usize,
    next_msg_id: u16,
}

impl Fragmenter {
    pub(crate) fn new(max_payload: usize) -> Self {
        Self {
            max_payload: max_payload.max(1),
            next_msg_id: 0,
        }
    }

    /// Split `frame` into one or more fragments. An empty frame still yields a
    /// single (empty-chunk) fragment so it reassembles to an empty frame rather
    /// than vanishing.
    pub(crate) fn fragment(&mut self, frame: &[u8]) -> Vec<Vec<u8>> {
        let msg_id = self.next_msg_id;
        self.next_msg_id = self.next_msg_id.wrapping_add(1);

        let chunks: Vec<&[u8]> = if frame.is_empty() {
            vec![&[]]
        } else {
            frame.chunks(self.max_payload).collect()
        };
        let total = chunks.len() as u16;

        chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| {
                let mut out = Vec::with_capacity(HEADER + chunk.len());
                out.extend_from_slice(&msg_id.to_be_bytes());
                out.extend_from_slice(&total.to_be_bytes());
                out.extend_from_slice(&(index as u16).to_be_bytes());
                out.extend_from_slice(chunk);
                out
            })
            .collect()
    }
}

/// Rebuilds whole frames from the fragments it's fed, tolerating out-of-order
/// and duplicate delivery. State persists across calls (one frame's fragments
/// may arrive over several `recv`s).
pub(crate) struct Reassembler {
    pending: HashMap<u16, Partial>,
}

impl Reassembler {
    pub(crate) fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Fold one fragment into the in-progress state, returning the completed
    /// frame if this fragment was the last one missing. Malformed or
    /// inconsistent fragments are ignored (`None`), the same way the modem
    /// drops a frame that fails CRC.
    pub(crate) fn ingest(&mut self, fragment: &[u8]) -> Option<Vec<u8>> {
        if fragment.len() < HEADER {
            return None;
        }
        let msg_id = u16::from_be_bytes([fragment[0], fragment[1]]);
        let total = u16::from_be_bytes([fragment[2], fragment[3]]) as usize;
        let index = u16::from_be_bytes([fragment[4], fragment[5]]) as usize;
        if total == 0 || index >= total {
            return None;
        }

        // Bound memory: once too many distinct messages are half-assembled,
        // drop them all rather than let a never-completed one accumulate.
        if self.pending.len() >= MAX_PENDING && !self.pending.contains_key(&msg_id) {
            self.pending.clear();
        }

        let entry = self
            .pending
            .entry(msg_id)
            .or_insert_with(|| Partial::new(total));
        if entry.total != total {
            return None; // fragments disagree on the frame's shape; ignore
        }
        entry.insert(index, &fragment[HEADER..]);

        if entry.is_complete() {
            self.pending.remove(&msg_id).map(Partial::assemble)
        } else {
            None
        }
    }
}

/// A frame being rebuilt from its fragments.
struct Partial {
    total: usize,
    parts: Vec<Option<Vec<u8>>>,
    filled: usize,
}

impl Partial {
    fn new(total: usize) -> Self {
        Self {
            total,
            parts: vec![None; total],
            filled: 0,
        }
    }

    fn insert(&mut self, index: usize, chunk: &[u8]) {
        if self.parts[index].is_none() {
            self.filled += 1;
        }
        self.parts[index] = Some(chunk.to_vec()); // re-seat on a duplicate; harmless
    }

    fn is_complete(&self) -> bool {
        self.filled == self.total
    }

    fn assemble(self) -> Vec<u8> {
        let mut out = Vec::new();
        for part in self.parts.into_iter().flatten() {
            out.extend_from_slice(&part);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed every fragment of a frame through a fresh reassembler in the given
    /// order, returning whatever it completes.
    fn reassemble(fragments: &[Vec<u8>], order: impl IntoIterator<Item = usize>) -> Option<Vec<u8>> {
        let mut r = Reassembler::new();
        let mut done = None;
        for i in order {
            if let Some(frame) = r.ingest(&fragments[i]) {
                done = Some(frame);
            }
        }
        done
    }

    #[test]
    fn round_trips_a_multi_fragment_frame() {
        let mut f = Fragmenter::new(4);
        let frame = b"radio link is up";
        let fragments = f.fragment(frame);
        assert!(fragments.len() > 1, "frame should have split");
        assert_eq!(reassemble(&fragments, 0..fragments.len()).as_deref(), Some(&frame[..]));
    }

    #[test]
    fn a_frame_within_one_mtu_is_a_single_fragment() {
        let mut f = Fragmenter::new(64);
        let fragments = f.fragment(b"short");
        assert_eq!(fragments.len(), 1);
        assert_eq!(reassemble(&fragments, [0]).as_deref(), Some(&b"short"[..]));
    }

    #[test]
    fn empty_frame_round_trips() {
        let mut f = Fragmenter::new(64);
        let fragments = f.fragment(b"");
        assert_eq!(fragments.len(), 1);
        assert_eq!(reassemble(&fragments, [0]).as_deref(), Some(&b""[..]));
    }

    #[test]
    fn reassembles_out_of_order() {
        let mut f = Fragmenter::new(2);
        let frame = b"abcdefg";
        let fragments = f.fragment(frame);
        let reversed = (0..fragments.len()).rev();
        assert_eq!(reassemble(&fragments, reversed).as_deref(), Some(&frame[..]));
    }

    #[test]
    fn duplicate_fragments_are_idempotent() {
        let mut f = Fragmenter::new(3);
        let frame = b"duplicated";
        let fragments = f.fragment(frame);
        // Every fragment delivered twice.
        let order: Vec<usize> = (0..fragments.len()).chain(0..fragments.len()).collect();
        assert_eq!(reassemble(&fragments, order).as_deref(), Some(&frame[..]));
    }

    #[test]
    fn an_incomplete_message_yields_nothing() {
        let mut f = Fragmenter::new(2);
        let fragments = f.fragment(b"abcdef"); // 3 fragments
        let mut r = Reassembler::new();
        // Deliver all but the last fragment.
        for fragment in &fragments[..fragments.len() - 1] {
            assert_eq!(r.ingest(fragment), None);
        }
    }

    #[test]
    fn two_messages_reassemble_independently_when_interleaved() {
        let mut f = Fragmenter::new(2);
        let one = f.fragment(b"hello");
        let two = f.fragment(b"world!");
        let mut r = Reassembler::new();
        let mut completed = Vec::new();
        // Interleave the two messages' fragments.
        for i in 0..one.len().max(two.len()) {
            if let Some(frag) = one.get(i)
                && let Some(frame) = r.ingest(frag)
            {
                completed.push(frame);
            }
            if let Some(frag) = two.get(i)
                && let Some(frame) = r.ingest(frag)
            {
                completed.push(frame);
            }
        }
        assert!(completed.contains(&b"hello".to_vec()));
        assert!(completed.contains(&b"world!".to_vec()));
    }

    #[test]
    fn a_fragment_shorter_than_the_header_is_ignored() {
        let mut r = Reassembler::new();
        assert_eq!(r.ingest(&[0, 1, 2]), None);
    }

    #[test]
    fn an_index_past_total_is_ignored() {
        let mut r = Reassembler::new();
        // msg_id=1, total=2, index=5 — out of range.
        let bogus = [0u8, 1, 0, 2, 0, 5, b'x'];
        assert_eq!(r.ingest(&bogus), None);
    }

    #[test]
    fn fragments_that_disagree_on_total_are_ignored() {
        let mut r = Reassembler::new();
        // First fragment claims total=2, second (same msg_id) claims total=3.
        let first = [0u8, 7, 0, 2, 0, 0, b'a'];
        let second = [0u8, 7, 0, 3, 0, 1, b'b'];
        assert_eq!(r.ingest(&first), None);
        assert_eq!(r.ingest(&second), None); // stray, ignored — first survives
    }

    #[test]
    fn pending_messages_are_bounded() {
        let mut r = Reassembler::new();
        // Start MAX_PENDING distinct two-fragment messages, each with only its
        // first fragment — none can complete.
        for msg_id in 0..MAX_PENDING as u16 {
            let frag = [(msg_id >> 8) as u8, msg_id as u8, 0, 2, 0, 0, b'x'];
            assert_eq!(r.ingest(&frag), None);
        }
        assert_eq!(r.pending.len(), MAX_PENDING);
        // One more distinct message trips the cap, clearing the backlog.
        let overflow = [0xFFu8, 0xFF, 0, 2, 0, 0, b'x'];
        assert_eq!(r.ingest(&overflow), None);
        assert_eq!(r.pending.len(), 1);
    }
}
