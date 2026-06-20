//! The RF front-end a modem hands samples to.
//!
//! [`Channel`] is the seam between digital signal processing and the radio
//! hardware. A real SDR — HackRF, PlutoSDR, USRP via SoapySDR — would implement
//! it by streaming IQ to/from the device. [`UdpChannel`] implements it over UDP
//! so two nodes exchange the very same baseband samples without any hardware,
//! which is how the link is simulated.
//!
//! This crate carries its own [`Iq`] type rather than depending on
//! `kaem-modem`'s — the same convention `kaem-sim` already follows — so it
//! stays a pure capability crate with zero `kaem-*` dependencies. An
//! orchestrator binary converts between the two `Iq` types at the boundary (a
//! trivial copy of two `f32` fields).

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

/// One complex baseband sample. Mirrors `kaem-modem`'s `Iq` field-for-field;
/// an orchestrator converts between the two at the boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iq {
    pub i: f32,
    pub q: f32,
}

/// Errors raised by a channel.
#[derive(Debug)]
pub enum ChannelError {
    /// Underlying I/O failure (socket bind, send, receive).
    Io(std::io::Error),
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::Io(e) => write!(f, "channel io error: {e}"),
        }
    }
}

impl std::error::Error for ChannelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ChannelError::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for ChannelError {
    fn from(e: std::io::Error) -> Self {
        ChannelError::Io(e)
    }
}

/// Carries one burst of baseband IQ samples between nodes.
pub trait Channel {
    fn transmit(&mut self, samples: &[Iq]) -> Result<(), ChannelError>;
    fn receive(&mut self) -> Result<Option<Vec<Iq>>, ChannelError>;

    /// The address actually bound, if this channel has one (e.g. UDP). A
    /// channel without a network address — like the in-process sim — inherits
    /// this default.
    fn local_addr(&self) -> Option<SocketAddr> {
        None
    }
}

const BYTES_PER_SAMPLE: usize = 8; // two little-endian f32s
const HEADER: usize = 8; // stream_id(4) + total(2) + index(2)
// Stay well under the smallest common UDP datagram cap (macOS defaults its
// net.inet.udp.maxdgram to 9216 bytes), so a burst fragments rather than fails.
const MAX_CHUNK: usize = 8_000;
const RECV_BUF: usize = 16_384;
const MAX_PENDING: usize = 16; // cap half-assembled bursts so a lost fragment can't leak memory

/// Streams IQ over UDP. A burst is split into ordered, stream-tagged fragments
/// and reassembled on the far side, so frames larger than one datagram survive.
pub struct UdpChannel {
    socket: UdpSocket,
    peer: SocketAddr,
    buf: Vec<u8>,
    next_stream: u32,
    pending: HashMap<u32, Reassembly>,
    completed: VecDeque<Vec<Iq>>,
}

impl UdpChannel {
    pub fn bind(bind: SocketAddr, peer: SocketAddr) -> Result<Self, ChannelError> {
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        let _ = socket.set_broadcast(true);
        Ok(Self {
            socket,
            peer,
            buf: vec![0; RECV_BUF],
            next_stream: 0,
            pending: HashMap::new(),
            completed: VecDeque::new(),
        })
    }

    /// Fold one received datagram into the reassembly state, surfacing any
    /// burst it completes.
    fn ingest(&mut self, datagram: &[u8]) {
        if datagram.len() < HEADER {
            return;
        }
        let stream_id = u32::from_le_bytes([datagram[0], datagram[1], datagram[2], datagram[3]]);
        let total = u16::from_le_bytes([datagram[4], datagram[5]]) as usize;
        let index = u16::from_le_bytes([datagram[6], datagram[7]]) as usize;
        if total == 0 || index >= total {
            return;
        }

        if self.pending.len() > MAX_PENDING && !self.pending.contains_key(&stream_id) {
            self.pending.clear();
        }

        let entry = self
            .pending
            .entry(stream_id)
            .or_insert_with(|| Reassembly::new(total));
        if entry.total != total {
            return; // inconsistent fragmentation; ignore the stray datagram
        }
        entry.insert(index, datagram[HEADER..].to_vec());

        if entry.is_complete()
            && let Some(reassembly) = self.pending.remove(&stream_id)
        {
            self.completed
                .push_back(deserialize(&reassembly.assemble()));
        }
    }
}

impl Channel for UdpChannel {
    fn transmit(&mut self, samples: &[Iq]) -> Result<(), ChannelError> {
        let bytes = serialize(samples);
        let stream_id = self.next_stream;
        self.next_stream = self.next_stream.wrapping_add(1);

        // Always send at least one (possibly empty) fragment so an empty burst
        // still reassembles on the far side.
        let chunks: Vec<&[u8]> = if bytes.is_empty() {
            vec![&[]]
        } else {
            bytes.chunks(MAX_CHUNK).collect()
        };
        let total = chunks.len() as u16;

        for (index, chunk) in chunks.iter().enumerate() {
            let mut datagram = Vec::with_capacity(HEADER + chunk.len());
            datagram.extend_from_slice(&stream_id.to_le_bytes());
            datagram.extend_from_slice(&total.to_le_bytes());
            datagram.extend_from_slice(&(index as u16).to_le_bytes());
            datagram.extend_from_slice(chunk);
            self.socket.send_to(&datagram, self.peer)?;
        }
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Vec<Iq>>, ChannelError> {
        loop {
            if let Some(burst) = self.completed.pop_front() {
                return Ok(Some(burst));
            }
            match self.socket.recv_from(&mut self.buf) {
                Ok((n, _)) => {
                    let datagram = self.buf[..n].to_vec();
                    self.ingest(&datagram);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.local_addr().ok()
    }
}

/// Partial burst being rebuilt from fragments.
struct Reassembly {
    total: usize,
    parts: Vec<Option<Vec<u8>>>,
    filled: usize,
}

impl Reassembly {
    fn new(total: usize) -> Self {
        Self {
            total,
            parts: vec![None; total],
            filled: 0,
        }
    }

    fn insert(&mut self, index: usize, data: Vec<u8>) {
        if self.parts[index].is_none() {
            self.filled += 1;
        }
        self.parts[index] = Some(data);
    }

    fn is_complete(&self) -> bool {
        self.filled == self.total
    }

    fn assemble(self) -> Vec<u8> {
        let mut out = Vec::new();
        for part in self.parts.into_iter().flatten() {
            out.extend_from_slice(&part);
        }
        out
    }
}

fn serialize(samples: &[Iq]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * BYTES_PER_SAMPLE);
    for s in samples {
        bytes.extend_from_slice(&s.i.to_le_bytes());
        bytes.extend_from_slice(&s.q.to_le_bytes());
    }
    bytes
}

fn deserialize(bytes: &[u8]) -> Vec<Iq> {
    bytes
        .chunks_exact(BYTES_PER_SAMPLE)
        .map(|c| Iq {
            i: f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
            q: f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn any_local() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    fn recv_blocking(channel: &mut UdpChannel) -> Vec<Iq> {
        for _ in 0..200 {
            if let Some(burst) = channel.receive().unwrap() {
                return burst;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no burst received within timeout");
    }

    #[test]
    fn a_burst_crosses_the_link() {
        let mut rx = UdpChannel::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let mut tx = UdpChannel::bind(any_local(), rx_addr).unwrap();

        let burst = vec![Iq { i: 0.5, q: -0.25 }, Iq { i: 1.0, q: 0.0 }];
        tx.transmit(&burst).unwrap();

        assert_eq!(recv_blocking(&mut rx), burst);
    }

    #[test]
    fn a_burst_larger_than_one_datagram_fragments_and_reassembles() {
        let mut rx = UdpChannel::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let mut tx = UdpChannel::bind(any_local(), rx_addr).unwrap();

        // Comfortably larger than MAX_CHUNK once serialized (8 bytes/sample).
        let burst: Vec<Iq> = (0..2000)
            .map(|i| Iq {
                i: i as f32,
                q: -(i as f32),
            })
            .collect();
        tx.transmit(&burst).unwrap();

        assert_eq!(recv_blocking(&mut rx), burst);
    }

    #[test]
    fn an_empty_burst_round_trips() {
        let mut rx = UdpChannel::bind(any_local(), "127.0.0.1:9".parse().unwrap()).unwrap();
        let rx_addr = rx.local_addr().unwrap();
        let mut tx = UdpChannel::bind(any_local(), rx_addr).unwrap();

        tx.transmit(&[]).unwrap();
        assert_eq!(recv_blocking(&mut rx), Vec::<Iq>::new());
    }
}
