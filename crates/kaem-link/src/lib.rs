//! The link layer: everything that moves opaque byte frames between nodes.
//!
//! This crate is self-contained — it depends on no other `kaem` crate — and
//! gathers every way a frame can travel:
//!
//! * [`Transport`] — the port every link speaks: `send` a frame, `recv` the
//!   next one (non-blocking). Selecting *which* link to build is the job of a
//!   binary; this crate only provides the contract and the implementations.
//! * [`RadioTransport`] — the real signal path: an FSK [`modem`](crate::modem)
//!   turns a frame into baseband [`Iq`] samples that a [`Channel`] carries to
//!   the peer. [`UdpChannel`](crate::channel) is the over-UDP "airwaves";
//!   [`SimChannel`] carries the same samples across an in-process [`Medium`].
//! * [`UdpTransport`] / [`Loopback`] — development scaffolding that skips the
//!   modem: raw datagrams, and an in-process echo for running solo.
//!
//! A real SDR becomes one more [`Channel`] implementation; the modem and the
//! [`Transport`] surface above it never change.

mod channel;
mod loopback;
mod modem;
mod radio;
mod sim;
mod transport;
mod udp;

pub use channel::Channel;
pub use loopback::Loopback;
pub use modem::{Iq, ModemParams};
pub use radio::RadioTransport;
pub use sim::{Medium, NodeId, Pos, SimChannel};
pub use transport::{Transport, TransportError};
pub use udp::UdpTransport;
