//! The flood-relay envelope: a frame format independent of [`crate::WireMessage`]
//! used to carry an encrypted payload across the mesh.
//!
//! Only `chatroom_id`, `message_id`, and `ttl` are ever visible in cleartext —
//! `ciphertext` is opaque to any node that doesn't hold the chatroom's
//! symmetric key. A node that can't decrypt it can still relay it: that's the
//! whole point of the envelope existing as its own frame, distinct from
//! `WireMessage`, with its own magic bytes so the two can never be confused
//! while decoding.
//!
//! Frame layout (all integers big-endian):
//!
//! ```text
//! +--------+---------+--------------+-------------+-----+----------------+------------+-------+
//! | MAGIC  | VERSION | chatroom_id  | message_id  | ttl | ciphertext_len | ciphertext | crc16 |
//! | 2 "KE" | 1       | 8            | 8           | 1   | 4              | n          | 2     |
//! +--------+---------+--------------+-------------+-----+----------------+------------+-------+
//! ```
//!
//! `crc16` is CRC-16/CCITT-FALSE over every preceding byte, same as
//! [`crate::WireMessage`]'s frame.

use crate::CodecError;
use crate::crc;

const MAGIC: [u8; 2] = *b"KE";
const VERSION: u8 = 1;

/// A relayable, partially-opaque mesh frame: public routing metadata wrapped
/// around an encrypted payload only chatroom members can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Public identifier of the chatroom this message belongs to.
    pub chatroom_id: u64,
    /// Public identifier for relay dedup — nodes drop frames they've already
    /// relayed once.
    pub message_id: u64,
    /// Public hop budget; decremented by one hop, dropped at zero.
    pub ttl: u8,
    /// Opaque to anyone without the chatroom key.
    pub ciphertext: Vec<u8>,
}

/// Serialize an [`Envelope`] into a self-describing byte frame.
pub fn encode_envelope(envelope: &Envelope) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + 1 + 8 + 8 + 1 + 4 + envelope.ciphertext.len() + 2);
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.extend_from_slice(&envelope.chatroom_id.to_be_bytes());
    buf.extend_from_slice(&envelope.message_id.to_be_bytes());
    buf.push(envelope.ttl);
    buf.extend_from_slice(&(envelope.ciphertext.len() as u32).to_be_bytes());
    buf.extend_from_slice(&envelope.ciphertext);

    let checksum = crc::crc16(&buf);
    buf.extend_from_slice(&checksum.to_be_bytes());
    buf
}

/// Parse a byte frame back into an [`Envelope`], validating magic and CRC.
pub fn decode_envelope(frame: &[u8]) -> Result<Envelope, CodecError> {
    // Smallest possible frame: magic + version + chatroom_id + message_id +
    // ttl + zero-length ciphertext_len + crc = 2 + 1 + 8 + 8 + 1 + 4 + 2 = 26.
    if frame.len() < 26 {
        return Err(CodecError::TooShort);
    }

    let (payload, checksum) = frame.split_at(frame.len() - 2);
    let expected = u16::from_be_bytes([checksum[0], checksum[1]]);
    if crc::crc16(payload) != expected {
        return Err(CodecError::BadCrc);
    }

    let mut r = Reader::new(payload);
    if r.take(2)? != MAGIC {
        return Err(CodecError::BadMagic);
    }
    let version = r.u8()?;
    if version != VERSION {
        return Err(CodecError::BadVersion(version));
    }

    let chatroom_id = r.u64()?;
    let message_id = r.u64()?;
    let ttl = r.u8()?;
    let ciphertext_len = r.u32()? as usize;
    let ciphertext = r.take(ciphertext_len)?.to_vec();

    Ok(Envelope {
        chatroom_id,
        message_id,
        ttl,
        ciphertext,
    })
}

/// Minimal forward-only cursor over a byte slice.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let end = self.pos.checked_add(n).ok_or(CodecError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(CodecError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, CodecError> {
        let b = self.take(8)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Envelope {
        Envelope {
            chatroom_id: 0xDEAD_BEEF_0011_2233,
            message_id: 0x0123_4567_89AB_CDEF,
            ttl: 8,
            ciphertext: b"opaque sealed bytes".to_vec(),
        }
    }

    #[test]
    fn round_trip() {
        let envelope = sample();
        let frame = encode_envelope(&envelope);
        assert_eq!(decode_envelope(&frame), Ok(envelope));
    }

    #[test]
    fn empty_ciphertext_round_trips() {
        let envelope = Envelope {
            chatroom_id: 1,
            message_id: 2,
            ttl: 0,
            ciphertext: Vec::new(),
        };
        assert_eq!(decode_envelope(&encode_envelope(&envelope)), Ok(envelope));
    }

    #[test]
    fn corruption_is_rejected() {
        let mut frame = encode_envelope(&sample());
        let last = frame.len() - 3;
        frame[last] ^= 0xFF;
        assert_eq!(decode_envelope(&frame), Err(CodecError::BadCrc));
    }

    #[test]
    fn truncation_is_rejected() {
        let frame = encode_envelope(&sample());
        assert!(decode_envelope(&frame[..frame.len() - 1]).is_err());
    }

    #[test]
    fn wrong_magic_is_rejected() {
        // A well-formed frame (valid CRC) whose magic bytes are not "KE".
        let mut payload = vec![b'X', b'X', VERSION];
        payload.extend_from_slice(&1u64.to_be_bytes());
        payload.extend_from_slice(&2u64.to_be_bytes());
        payload.push(0);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let checksum = crc::crc16(&payload);
        payload.extend_from_slice(&checksum.to_be_bytes());
        assert_eq!(decode_envelope(&payload), Err(CodecError::BadMagic));
    }

    #[test]
    fn too_short_is_rejected() {
        assert_eq!(decode_envelope(b"short"), Err(CodecError::TooShort));
    }

    #[test]
    fn does_not_collide_with_wire_message_decoding() {
        // An envelope frame must never successfully decode as a WireMessage,
        // and vice versa, since they use distinct magic bytes.
        let envelope_frame = encode_envelope(&sample());
        assert!(crate::decode(&envelope_frame).is_err());

        let wire_frame = crate::encode(&crate::WireMessage {
            from: "alice".into(),
            to: "bob".into(),
            body: "hi".into(),
        });
        assert!(decode_envelope(&wire_frame).is_err());
    }
}
