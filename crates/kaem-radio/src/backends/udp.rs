//! Plain UDP transport.
//!
//! Frames are written verbatim into datagrams — no modulation, no extra
//! framing. It is the quickest way to put two nodes "on the air": run them on
//! different ports of one host, or broadcast across a LAN. The same byte frames
//! that would ride the SDR link travel here, so it is a faithful stand-in while
//! developing the layers above.

use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

use crate::{Link, Radio, RadioError};

/// Largest UDP payload (theoretical max for an IPv4 datagram).
const MAX_DATAGRAM: usize = 65_507;

pub struct UdpRadio {
    socket: UdpSocket,
    peer: SocketAddr,
    buf: Vec<u8>,
}

impl UdpRadio {
    pub fn bind(link: Link) -> Result<Self, RadioError> {
        let socket = UdpSocket::bind(link.bind)?;
        socket.set_nonblocking(true)?;
        // Best-effort: lets the peer address be a broadcast address on a LAN.
        let _ = socket.set_broadcast(true);
        Ok(Self {
            socket,
            peer: link.peer,
            buf: vec![0; MAX_DATAGRAM],
        })
    }

    /// The address actually bound (useful when the caller passed port 0).
    #[allow(dead_code)] // used in tests; useful for a future status line
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl Radio for UdpRadio {
    fn send(&mut self, frame: &[u8]) -> Result<(), RadioError> {
        if frame.len() > MAX_DATAGRAM {
            return Err(RadioError::FrameTooLarge(frame.len()));
        }
        self.socket.send_to(frame, self.peer)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Vec<u8>>, RadioError> {
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

    fn loopback_link(peer: SocketAddr) -> Link {
        Link {
            bind: "127.0.0.1:0".parse().unwrap(),
            peer,
        }
    }

    fn recv_blocking(radio: &mut UdpRadio) -> Vec<u8> {
        for _ in 0..200 {
            if let Some(frame) = radio.recv().unwrap() {
                return frame;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no frame received within timeout");
    }

    #[test]
    fn frame_crosses_the_link() {
        // Receiver binds first so the sender can target its real port.
        let mut rx = UdpRadio::bind(loopback_link("127.0.0.1:9".parse().unwrap())).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let mut tx = UdpRadio::bind(loopback_link(rx_addr)).unwrap();

        let frame = b"relay node is up on channel 7";
        tx.send(frame).unwrap();

        assert_eq!(recv_blocking(&mut rx), frame);
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let mut tx = UdpRadio::bind(loopback_link("127.0.0.1:9".parse().unwrap())).unwrap();
        let huge = vec![0u8; MAX_DATAGRAM + 1];
        assert!(matches!(
            tx.send(&huge),
            Err(RadioError::FrameTooLarge(_))
        ));
    }
}
