//! The chat wire protocol: turns a [`WireMessage`] into a byte frame and back.
//!
//! This is the chat core's own application-layer encoding — *what* a message
//! looks like on the wire. It owns nothing about *how* the frame travels; a
//! link carries the opaque bytes. Frame layout (all integers big-endian):
//!
//! ```text
//! +--------+---------+----------+----------+--------+--------+----------+------+-------+
//! | MAGIC  | VERSION | from_len | from ... | to_len | to ... | body_len | body | crc16 |
//! | 2 "KM" | 1       | 1        | n        | 1      | n      | 2        | n    | 2     |
//! +--------+---------+----------+----------+--------+--------+----------+------+-------+
//! ```
//!
//! `crc16` is CRC-16/CCITT-FALSE over every preceding byte.

const MAGIC: [u8; 2] = *b"KM";
const VERSION: u8 = 1;

/// A decoded chat message exchanged between nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireMessage {
    /// Callsign of the sender.
    pub from: String,
    /// Callsign of the intended recipient (free-form; mesh nodes may overhear).
    pub to: String,
    /// Message text.
    pub body: String,
}

/// Errors produced while decoding a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// Frame is shorter than the fixed header requires.
    TooShort,
    /// Magic bytes did not match — not a kaem chat frame.
    BadMagic,
    /// Protocol version is not understood by this build.
    BadVersion(u8),
    /// A length field ran past the end of the buffer.
    Truncated,
    /// Trailing CRC did not match the payload.
    BadCrc,
    /// A string field was not valid UTF-8.
    NonUtf8,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::TooShort => write!(f, "frame too short"),
            CodecError::BadMagic => write!(f, "bad magic"),
            CodecError::BadVersion(v) => write!(f, "unsupported version {v}"),
            CodecError::Truncated => write!(f, "frame truncated"),
            CodecError::BadCrc => write!(f, "crc mismatch"),
            CodecError::NonUtf8 => write!(f, "non-utf8 field"),
        }
    }
}

impl std::error::Error for CodecError {}

/// Serialize a [`WireMessage`] into a self-describing byte frame.
pub fn encode(message: &WireMessage) -> Vec<u8> {
    let from = message.from.as_bytes();
    let to = message.to.as_bytes();
    let body = message.body.as_bytes();

    let from = &from[..clamp_len(from.len())];
    let to = &to[..clamp_len(to.len())];
    let body = &body[..body.len().min(u16::MAX as usize)];

    let mut buf = Vec::with_capacity(8 + from.len() + to.len() + body.len() + 2);
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(from.len() as u8);
    buf.extend_from_slice(from);
    buf.push(to.len() as u8);
    buf.extend_from_slice(to);
    buf.extend_from_slice(&(body.len() as u16).to_be_bytes());
    buf.extend_from_slice(body);

    let checksum = crc16(&buf);
    buf.extend_from_slice(&checksum.to_be_bytes());
    buf
}

/// Parse a byte frame back into a [`WireMessage`], validating magic and CRC.
pub fn decode(frame: &[u8]) -> Result<WireMessage, CodecError> {
    // Smallest possible frame: magic + version + two zero-length strings +
    // zero-length body + crc = 2 + 1 + 1 + 1 + 2 + 2 = 9 bytes.
    if frame.len() < 9 {
        return Err(CodecError::TooShort);
    }

    let (payload, checksum) = frame.split_at(frame.len() - 2);
    let expected = u16::from_be_bytes([checksum[0], checksum[1]]);
    if crc16(payload) != expected {
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

    let from = r.string_u8()?;
    let to = r.string_u8()?;
    let body = r.string_u16()?;

    Ok(WireMessage { from, to, body })
}

fn clamp_len(len: usize) -> usize {
    len.min(u8::MAX as usize)
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

    fn u16(&mut self) -> Result<u16, CodecError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn string_u8(&mut self) -> Result<String, CodecError> {
        let len = self.u8()? as usize;
        self.string(len)
    }

    fn string_u16(&mut self) -> Result<String, CodecError> {
        let len = self.u16()? as usize;
        self.string(len)
    }

    fn string(&mut self, len: usize) -> Result<String, CodecError> {
        let bytes = self.take(len)?;
        str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| CodecError::NonUtf8)
    }
}

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF, no reflection, xorout 0x0000).
/// Kept local so the chat codec owns its own integrity check.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WireMessage {
        WireMessage {
            from: "alice".into(),
            to: "bob".into(),
            body: "relay node is up on channel 7".into(),
        }
    }

    #[test]
    fn round_trip() {
        let msg = sample();
        let frame = encode(&msg);
        assert_eq!(decode(&frame), Ok(msg));
    }

    #[test]
    fn empty_fields_round_trip() {
        let msg = WireMessage {
            from: String::new(),
            to: String::new(),
            body: String::new(),
        };
        assert_eq!(decode(&encode(&msg)), Ok(msg));
    }

    #[test]
    fn unicode_body_round_trip() {
        let msg = WireMessage {
            from: "n0de".into(),
            to: "mesh".into(),
            body: "signal clean ✓ — 73".into(),
        };
        assert_eq!(decode(&encode(&msg)), Ok(msg));
    }

    #[test]
    fn corruption_is_rejected() {
        let mut frame = encode(&sample());
        frame[10] ^= 0xFF;
        assert_eq!(decode(&frame), Err(CodecError::BadCrc));
    }

    #[test]
    fn truncation_is_rejected() {
        let frame = encode(&sample());
        assert!(decode(&frame[..frame.len() - 1]).is_err());
    }

    #[test]
    fn noise_fails_crc() {
        // Arbitrary bytes are overwhelmingly unlikely to carry a valid CRC.
        assert_eq!(decode(b"not a kaem frame!!"), Err(CodecError::BadCrc));
    }

    #[test]
    fn wrong_magic_is_rejected() {
        // A well-formed frame (valid CRC) whose magic bytes are not "KM".
        let mut payload = vec![b'X', b'X', VERSION, 0, 0, 0, 0];
        let checksum = crc16(&payload);
        payload.extend_from_slice(&checksum.to_be_bytes());
        assert_eq!(decode(&payload), Err(CodecError::BadMagic));
    }

    #[test]
    fn crc16_known_vector() {
        // CRC-16/CCITT-FALSE of "123456789" is 0x29B1.
        assert_eq!(crc16(b"123456789"), 0x29B1);
        assert_eq!(crc16(b""), 0xFFFF);
    }
}
