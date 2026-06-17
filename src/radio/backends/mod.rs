//! Concrete [`Radio`](crate::radio::Radio) implementations.
//!
//! Each submodule is one transport protocol. They are reachable only through
//! [`crate::radio::open`]; nothing outside the radio module names them
//! directly, so a protocol can be replaced without rippling into callers.

pub mod loopback;
pub mod udp;
