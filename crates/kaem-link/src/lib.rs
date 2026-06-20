//! The link layer: the [`Transport`] port every link speaks, plus development
//! scaffolding that skips the radio entirely.
//!
//! This crate is self-contained — it depends on no other `kaem` crate. The
//! real radio signal path (fragmentation + the FSK modem + a channel) now
//! lives in its own pure capability crates — `kaem-fragment`, `kaem-modem`,
//! `kaem-channel` — composed into a [`Transport`] by an orchestrator binary
//! (see `kaem-radio-pipeline`), not by this crate.
//!
//! * [`Transport`] — the port every link speaks: `send` a frame, `recv` the
//!   next one (non-blocking).
//! * [`UdpTransport`] / [`Loopback`] — development scaffolding that skips the
//!   modem: raw datagrams, and an in-process echo for running solo.

#[cfg(feature = "dev-transports")]
mod loopback;
mod transport;
#[cfg(feature = "dev-transports")]
mod udp;

#[cfg(feature = "dev-transports")]
pub use loopback::Loopback;
pub use transport::{Transport, TransportError};
#[cfg(feature = "dev-transports")]
pub use udp::UdpTransport;
