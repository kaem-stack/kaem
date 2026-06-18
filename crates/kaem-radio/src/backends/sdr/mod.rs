//! Software-defined-radio transport.
//!
//! This backend is the real radio signal path: a frame is FSK-modulated into
//! baseband IQ samples by the [`modem`], then those samples are carried to the
//! peer by a [`Channel`]. Today the channel is UDP (simulated airwaves); swap
//! in a SoapySDR-backed channel and the same modem drives real hardware — the
//! [`Radio`] interface above never notices.

mod channel;
mod modem;

use std::net::SocketAddr;

use crate::{Link, Radio, RadioError};
use channel::{Channel, UdpChannel};
use modem::{Modem, ModemParams};

pub struct SdrRadio {
    modem: Modem,
    channel: UdpChannel,
}

impl SdrRadio {
    pub fn bind(link: Link) -> Result<Self, RadioError> {
        Ok(Self {
            modem: Modem::new(ModemParams::default()),
            channel: UdpChannel::bind(link)?,
        })
    }

    /// The address actually bound (useful when the caller passed port 0).
    #[allow(dead_code)] // used in tests; useful for a future status line
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.channel.local_addr()
    }
}

impl Radio for SdrRadio {
    fn send(&mut self, frame: &[u8]) -> Result<(), RadioError> {
        let samples = self.modem.modulate(frame);
        self.channel.transmit(&samples)
    }

    fn recv(&mut self) -> Result<Option<Vec<u8>>, RadioError> {
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

    fn link_to(peer: SocketAddr) -> Link {
        Link {
            bind: "127.0.0.1:0".parse().unwrap(),
            peer,
        }
    }

    fn pair() -> (SdrRadio, SdrRadio) {
        let rx = SdrRadio::bind(link_to("127.0.0.1:9".parse().unwrap())).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let tx = SdrRadio::bind(link_to(rx_addr)).unwrap();
        (tx, rx)
    }

    fn recv_blocking(radio: &mut SdrRadio) -> Vec<u8> {
        for _ in 0..400 {
            if let Some(frame) = radio.recv().unwrap() {
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
