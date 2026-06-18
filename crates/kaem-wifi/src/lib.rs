//! WiFi transport.
//!
//! WiFi is, in software terms, just the IP stack riding a wireless NIC — so
//! this adapter moves frames as UDP datagrams over whatever network the host's
//! WiFi is on. Point two nodes at each other by address, or aim at the subnet
//! broadcast to reach every node on the LAN. It shares the plain-datagram
//! approach of the UDP transport; kept as its own crate so it can grow
//! WiFi-specific behaviour (interface binding, discovery, mDNS) without
//! touching the others.

use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

use kaem_transport::{Transport, TransportError};

/// Largest UDP payload (theoretical max for an IPv4 datagram).
const MAX_DATAGRAM: usize = 65_507;

pub struct WifiTransport {
    socket: UdpSocket,
    peer: SocketAddr,
    buf: Vec<u8>,
}

impl WifiTransport {
    /// Bind locally and target `peer` (a host or broadcast address) for
    /// outgoing frames.
    pub fn bind(bind: SocketAddr, peer: SocketAddr) -> Result<Self, TransportError> {
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        // Allow aiming at a subnet broadcast address to reach the whole LAN.
        let _ = socket.set_broadcast(true);
        Ok(Self {
            socket,
            peer,
            buf: vec![0; MAX_DATAGRAM],
        })
    }

    /// The address actually bound (useful when the caller passed port 0).
    #[allow(dead_code)] // used in tests; useful for a future status line
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl Transport for WifiTransport {
    fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        if frame.len() > MAX_DATAGRAM {
            return Err(TransportError::FrameTooLarge(frame.len()));
        }
        self.socket.send_to(frame, self.peer)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        match self.socket.recv_from(&mut self.buf) {
            Ok((n, _)) => Ok(Some(self.buf[..n].to_vec())),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn any_local() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    #[test]
    fn frame_crosses_the_link() {
        let mut rx = WifiTransport::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let mut tx = WifiTransport::bind(any_local(), rx_addr).unwrap();

        let frame = b"mesh node online";
        tx.send(frame).unwrap();

        let mut got = None;
        for _ in 0..200 {
            if let Some(frame) = rx.recv().unwrap() {
                got = Some(frame);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got.as_deref(), Some(&frame[..]));
    }
}
