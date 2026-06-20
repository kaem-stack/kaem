//! Read-only decoding of the link-level frames the sandbox pushes onto the
//! medium, purely for the operator-facing packet inspector. `kaem-mesh`
//! deliberately doesn't expose its `Envelope` type or `decode_envelope`
//! outside the crate — the mesh's own public surface is bytes-in/bytes-out —
//! so this module duplicates just enough of the documented `KE` frame layout
//! to read the public header fields back out for display. This mirrors the
//! existing pattern elsewhere in the codebase (e.g. kaem-mesh's own tests
//! duplicate a minimal crypto backend instead of importing one) rather than
//! widening any crate's public API for a debugging tool.
//!
//! `kaem-node`'s `WireMessage`/`decode` *are* already public, so the wire
//! frame branch below just calls straight into `kaem_node::decode`.

/// What the inspector shows for one captured frame: either a decoded mesh
/// envelope header (the ciphertext stays opaque — only its length is shown)
/// or a decoded chat wire message, or a frame that didn't parse as either.
pub enum DecodedFrame {
    Envelope {
        chatroom_id: u64,
        message_id: u64,
        ttl: u8,
        ciphertext_len: usize,
    },
    Wire(kaem_node::WireMessage),
    Unknown,
}

/// Try the `KE` envelope header first (mesh frames are what actually travels
/// over the link), then fall back to a `KM` chat wire frame, then give up.
pub fn decode_frame(frame: &[u8]) -> DecodedFrame {
    if let Some(envelope) = decode_envelope_header(frame) {
        return envelope;
    }
    if let Ok(message) = kaem_node::decode(frame) {
        return DecodedFrame::Wire(message);
    }
    DecodedFrame::Unknown
}

/// Parse just the public header of a `kaem-mesh` `Envelope` frame:
///
/// ```text
/// +--------+---------+--------------+-------------+-----+----------------+------------+-------+
/// | MAGIC  | VERSION | chatroom_id  | message_id  | ttl | ciphertext_len | ciphertext | crc16 |
/// | 2 "KE" | 1       | 8            | 8           | 1   | 4              | n          | 2     |
/// +--------+---------+--------------+-------------+-----+----------------+------------+-------+
/// ```
///
/// CRC is not re-verified here (the link already dropped anything that
/// didn't survive demodulation) — this is a best-effort read for display,
/// not a security boundary. `None` if the frame is too short or the magic
/// doesn't match.
fn decode_envelope_header(frame: &[u8]) -> Option<DecodedFrame> {
    const HEADER_LEN: usize = 2 + 1 + 8 + 8 + 1 + 4;
    if frame.len() < HEADER_LEN + 2 || &frame[0..2] != b"KE" {
        return None;
    }
    let chatroom_id = u64::from_be_bytes(frame[3..11].try_into().ok()?);
    let message_id = u64::from_be_bytes(frame[11..19].try_into().ok()?);
    let ttl = frame[19];
    let ciphertext_len = u32::from_be_bytes(frame[20..24].try_into().ok()?) as usize;
    Some(DecodedFrame::Envelope {
        chatroom_id,
        message_id,
        ttl,
        ciphertext_len,
    })
}
