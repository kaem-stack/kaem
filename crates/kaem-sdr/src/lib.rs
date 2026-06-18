//! Software-defined-radio transport.
//!
//! This adapter is the real radio signal path: a frame is FSK-modulated into
//! baseband IQ samples by the [`modem`], then those samples are carried to the
//! peer by a [`Channel`](channel::Channel). Today the channel is UDP (simulated
//! airwaves); swap in a SoapySDR-backed channel and the same modem drives real
//! hardware — the [`Transport`] interface never notices.

mod channel;
mod modem;

use std::net::SocketAddr;

use kaem_transport::{Transport, TransportError};

use channel::{Channel, UdpChannel};
use modem::{Modem, ModemParams};

pub struct SdrTransport {
    modem: Modem,
    channel: UdpChannel,
}

impl SdrTransport {
    /// Bind locally and target `peer` for the simulated RF channel.
    pub fn bind(bind: SocketAddr, peer: SocketAddr) -> Result<Self, TransportError> {
        Ok(Self {
            modem: Modem::new(ModemParams::default()),
            channel: UdpChannel::bind(bind, peer)?,
        })
    }

    /// The address actually bound (useful when the caller passed port 0).
    #[allow(dead_code)] // used in tests; useful for a future status line
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.channel.local_addr()
    }
}

impl Transport for SdrTransport {
    fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        let samples = self.modem.modulate(frame);
        self.channel.transmit(&samples)
    }

    fn recv(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        // Drain bursts until one demodulates cleanly or the channel is empty;
        // a burst that fails its CRC is dropped like real-world line noise.
        loop {
            match self.channel.receive()? {
                Some(samples) => {
                    if let Some(frame) = self.modem.demodulate(&samples) {
                        return Ok(Some(frame));
                    }
                }
                None => return Ok(None),
            }
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

    fn pair() -> (SdrTransport, SdrTransport) {
        let rx = SdrTransport::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let tx = SdrTransport::bind(any_local(), rx_addr).unwrap();
        (tx, rx)
    }

    fn recv_blocking(transport: &mut SdrTransport) -> Vec<u8> {
        for _ in 0..400 {
            if let Some(frame) = transport.recv().unwrap() {
                return frame;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no frame demodulated within timeout");
    }

    #[test]
    fn message_survives_the_modulated_link() {
        let (mut tx, mut rx) = pair();
        let frame = b"hey, are you on the new repeater?";
        tx.send(frame).unwrap();
        assert_eq!(recv_blocking(&mut rx), frame);
    }

    #[test]
    fn large_frame_fragments_and_reassembles() {
        let (mut tx, mut rx) = pair();
        // Big enough that the modulated IQ spans several datagrams.
        let frame: Vec<u8> = (0..2000).map(|i| (i * 7 % 256) as u8).collect();
        tx.send(&frame).unwrap();
        assert_eq!(recv_blocking(&mut rx), frame);
    }
}
