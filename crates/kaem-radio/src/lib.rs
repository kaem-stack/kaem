//! Radio transport — the network interface the rest of kaem speaks to.
//!
//! Everything above this module deals only in opaque byte frames and the
//! [`Radio`] trait. *How* those frames reach another node — plain UDP, an
//! FSK software-defined radio, a future LoRa link — is decided by [`open`],
//! the factory, and implemented in the private [`backends`] submodule. Swap
//! the protocol by adding a backend and a [`Config`] variant; callers never
//! change.

mod backends;

use std::net::SocketAddr;

/// A bidirectional link that moves opaque byte frames between nodes.
///
/// This trait is the entire public surface a caller depends on. Implementations
/// are non-blocking: [`recv`](Radio::recv) returns `Ok(None)` when nothing has
/// arrived rather than waiting.
pub trait Radio {
    /// Transmit one frame. Any framing or modulation is the backend's concern.
    fn send(&mut self, frame: &[u8]) -> Result<(), RadioError>;

    /// Return the next received frame, or `None` if none is ready.
    fn recv(&mut self) -> Result<Option<Vec<u8>>, RadioError>;
}

/// Address pair describing a point-to-point link over a socket-based medium.
#[derive(Debug, Clone, Copy)]
pub struct Link {
    /// Local address to bind and receive on.
    pub bind: SocketAddr,
    /// Remote address frames are transmitted to.
    pub peer: SocketAddr,
}

/// Selects which transport [`open`] builds. Add a variant to add a protocol.
#[derive(Debug, Clone, Copy)]
pub enum Config {
    /// In-process self-loop. No I/O — handy for tests and solo runs.
    Loopback,
    /// Plain datagrams: frames go straight onto the wire, unmodulated.
    Udp(Link),
    /// Software-defined radio: frames are FSK-modulated to IQ samples and
    /// carried over the channel, the way real RF hardware would see them.
    Sdr(Link),
}

/// Factory: establish a transport from a [`Config`].
///
/// This is the single place a radio connection is opened, which keeps all
/// connection logic out of `main` and lets the protocol be chosen at runtime.
pub fn open(config: Config) -> Result<Box<dyn Radio>, RadioError> {
    match config {
        Config::Loopback => Ok(Box::new(backends::loopback::Loopback::new())),
        Config::Udp(link) => Ok(Box::new(backends::udp::UdpRadio::bind(link)?)),
        Config::Sdr(link) => Ok(Box::new(backends::sdr::SdrRadio::bind(link)?)),
    }
}

/// Errors raised by a transport.
#[derive(Debug)]
pub enum RadioError {
    /// Underlying I/O failure (socket bind, send, receive).
    Io(std::io::Error),
    /// The frame was larger than the medium can carry in one transmission.
    FrameTooLarge(usize),
}

impl std::fmt::Display for RadioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RadioError::Io(e) => write!(f, "radio io error: {e}"),
            RadioError::FrameTooLarge(n) => write!(f, "frame too large: {n} bytes"),
        }
    }
}

impl std::error::Error for RadioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RadioError::Io(e) => Some(e),
            RadioError::FrameTooLarge(_) => None,
        }
    }
}

impl From<std::io::Error> for RadioError {
    fn from(e: std::io::Error) -> Self {
        RadioError::Io(e)
    }
}
