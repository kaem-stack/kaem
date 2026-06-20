//! The link layer: everything that moves opaque byte frames between nodes.
//!
//! This crate is self-contained — it depends on no other `kaem` crate — and
//! gathers every way a frame can travel:
//!
//! * [`Transport`] — the port every link speaks: `send` a frame, `recv` the
//!   next one (non-blocking). Selecting *which* link to build, and composing
//!   the radio's pieces below into one, is the job of a binary; this crate
//!   only provides the contract and the individual building blocks.
//! * [`Modem`] — the DSP core: an FSK modem that turns a frame into baseband
//!   [`Iq`] samples that a [`Channel`] carries to the peer. [`UdpChannel`] is
//!   the over-UDP "airwaves"; a binary can carry the same samples across an
//!   in-process RF simulation (`kaem-sim`) by adapting it to [`Channel`] —
//!   this crate has no opinion on the sim, only on the seam a simulated
//!   channel must satisfy.
//! * [`Fragmenter`] / [`Reassembler`] — split a frame too big for one
//!   over-the-air transmission into ordered pieces, and rebuild it on the far
//!   side. A binary composes these with the modem and a channel into its own
//!   radio pipeline (see `kaem-radio-pipeline`); this crate only owns the
//!   pieces, not their composition.
//! * [`UdpTransport`] / [`Loopback`] — development scaffolding that skips the
//!   modem: raw datagrams, and an in-process echo for running solo.
//!
//! A real SDR becomes one more [`Channel`] implementation; the modem and the
//! [`Transport`] surface above it never change.

mod channel;
mod fragment;
#[cfg(feature = "dev-transports")]
mod loopback;
mod modem;
mod transport;
#[cfg(feature = "dev-transports")]
mod udp;

pub use channel::{Channel, UdpChannel};
pub use fragment::{DEFAULT_MAX_PAYLOAD, Fragmenter, Reassembler};
#[cfg(feature = "dev-transports")]
pub use loopback::Loopback;
pub use modem::{Iq, Modem, ModemParams};
pub use transport::{Transport, TransportError};
#[cfg(feature = "dev-transports")]
pub use udp::UdpTransport;
