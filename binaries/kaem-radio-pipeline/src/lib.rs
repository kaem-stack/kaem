//! `RadioPipeline` — the binary-side composition of the radio signal path:
//! fragmentation + the FSK modem + a [`Channel`]. `kaem-link` only owns the
//! individual pieces ([`Fragmenter`]/[`Reassembler`], [`Modem`], [`Channel`]);
//! composing them into one [`Transport`] is an orchestrator's job, not a
//! library's — see the `Architecture` section of the workspace `CLAUDE.md`.
//!
//! This crate is itself binary-side composition code, not a `crates/*`
//! protocol crate: both `kaem` and `kaem-sandbox` need the identical
//! fragment→modulate→channel pipeline and differ only in which [`Channel`]
//! they hand it (`UdpChannel` vs. a sim adapter), so the glue lives here once
//! rather than being duplicated in each binary.

use std::net::SocketAddr;

use kaem_link::{
    Channel, DEFAULT_MAX_PAYLOAD, Fragmenter, Modem, ModemParams, Reassembler, Transport,
    TransportError, UdpChannel,
};

/// Composes [`Fragmenter`]/[`Reassembler`] + [`Modem`] + a [`Channel`] into a
/// [`Transport`]: the real radio signal path, generic over the channel that
/// actually carries the IQ bursts (UDP today, an SDR or an in-process RF
/// simulation elsewhere).
pub struct RadioPipeline {
    modem: Modem,
    channel: Box<dyn Channel>,
    /// Splits an outbound frame into MTU-sized pieces; reassembles the inbound
    /// ones. The air carries a frame too big for one transmission as several
    /// modulated fragments — invisible to whoever handed the frame down.
    fragmenter: Fragmenter,
    reassembler: Reassembler,
}

impl RadioPipeline {
    /// Build a radio pipeline over any [`Channel`] — UDP today, an SDR device
    /// or an in-process simulation elsewhere — with the default modem
    /// parameters and over-the-air MTU.
    pub fn new(channel: Box<dyn Channel>) -> Self {
        Self::with_mtu(channel, DEFAULT_MAX_PAYLOAD)
    }

    /// Build a radio pipeline with an explicit over-the-air MTU: the most
    /// bytes of a caller's frame carried per modulated fragment. A binary
    /// tunes this to the frame size its real (or simulated) front-end can
    /// push in one burst.
    pub fn with_mtu(channel: Box<dyn Channel>, mtu: usize) -> Self {
        Self {
            modem: Modem::new(ModemParams::default()),
            channel,
            fragmenter: Fragmenter::new(mtu),
            reassembler: Reassembler::new(),
        }
    }

    /// Bind locally and target `peer` over UDP-simulated airwaves.
    pub fn bind(bind: SocketAddr, peer: SocketAddr) -> Result<Self, TransportError> {
        Ok(Self::new(Box::new(UdpChannel::bind(bind, peer)?)))
    }

    /// The address actually bound (useful when the caller passed port 0).
    /// Channels without an address (e.g. the in-process sim) return `None`.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.channel.local_addr()
    }
}

impl Transport for RadioPipeline {
    fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        // Split into MTU-sized fragments and modulate each as its own burst;
        // a frame within one MTU is just a single fragment.
        for fragment in self.fragmenter.fragment(frame) {
            let samples = self.modem.modulate(&fragment);
            self.channel.transmit(&samples)?;
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        // Drain bursts until reassembly completes a whole frame or the channel
        // is empty. A burst that fails its CRC is dropped like real-world line
        // noise; a fragment that arrives feeds the reassembler, which only
        // surfaces a frame once every piece of it is in.
        loop {
            match self.channel.receive()? {
                Some(samples) => {
                    if let Some(fragment) = self.modem.demodulate(&samples)
                        && let Some(frame) = self.reassembler.ingest(&fragment)
                    {
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

    fn pair() -> (RadioPipeline, RadioPipeline) {
        let rx = RadioPipeline::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap();
        let rx_addr = rx.local_addr().expect("udp channel has a local addr");
        let tx = RadioPipeline::bind(any_local(), rx_addr).unwrap();
        (tx, rx)
    }

    fn recv_blocking(transport: &mut RadioPipeline) -> Vec<u8> {
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

    #[test]
    fn frame_past_the_mtu_travels_as_fragments_and_rebuilds() {
        // A deliberately tiny MTU so a modest frame is forced across several
        // over-the-air fragments, each its own modulated burst.
        let rx = RadioPipeline::with_mtu(
            Box::new(UdpChannel::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap()),
            16,
        );
        let rx_addr = rx.local_addr().expect("udp channel has a local addr");
        let mut tx = RadioPipeline::with_mtu(
            Box::new(UdpChannel::bind(any_local(), rx_addr).unwrap()),
            16,
        );
        let mut rx = rx;

        let frame = b"this message is comfortably longer than a sixteen byte mtu";
        tx.send(frame).unwrap();
        assert_eq!(recv_blocking(&mut rx), frame);
    }
}
