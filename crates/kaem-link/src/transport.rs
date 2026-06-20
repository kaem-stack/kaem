//! The transport port — the single interface every link speaks.
//!
//! Defines *what* a transport must do — move opaque byte frames between nodes —
//! and says nothing about *how*. Each medium in this crate (the radio, raw UDP,
//! in-process loopback) implements [`Transport`]; selecting which one to build
//! is left to the binary that composes the link.

/// A bidirectional link that moves opaque byte frames between nodes.
///
/// Implementations are non-blocking: [`recv`](Transport::recv) returns
/// `Ok(None)` when nothing has arrived rather than waiting.
pub trait Transport {
    /// Transmit one frame. Any framing or modulation is the adapter's concern.
    fn send(&mut self, frame: &[u8]) -> Result<(), TransportError>;

    /// Return the next received frame, or `None` if none is ready.
    fn recv(&mut self) -> Result<Option<Vec<u8>>, TransportError>;
}

/// Errors raised by a transport.
#[derive(Debug)]
pub enum TransportError {
    /// Underlying I/O failure (socket bind, send, receive).
    Io(std::io::Error),
    /// The frame was larger than the medium can carry in one transmission.
    FrameTooLarge(usize),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "transport io error: {e}"),
            TransportError::FrameTooLarge(n) => write!(f, "frame too large: {n} bytes"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransportError::Io(e) => Some(e),
            TransportError::FrameTooLarge(_) => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::Io(e)
    }
}
