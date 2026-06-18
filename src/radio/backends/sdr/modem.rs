//! A binary-FSK software modem: the digital-signal-processing core of the SDR
//! link. It turns a byte frame into complex baseband IQ samples (the exact
//! samples an SDR would push to its DAC) and recovers the bytes back from
//! samples coming off the air.
//!
//! Over-the-air frame, built around the caller's payload:
//!
//! ```text
//! +-----------+------+-----+---------+-------+
//! | preamble  | sync | len | payload | crc16 |
//! | 0xAA x N  | 2    | 2   | n       | 2     |
//! +-----------+------+-----+---------+-------+
//! ```
//!
//! * **preamble** — alternating bits so the receiver's discriminator settles.
//! * **sync** — a known word the receiver correlates against to find the byte
//!   boundary inside the bit stream.
//! * **len/crc** — frame the payload and detect bit errors the channel adds.
//!
//! Modulation is continuous-phase 2-FSK: bit `1` advances the carrier by
//! `+freq_dev`, bit `0` by `-freq_dev`. Demodulation is a quadrature frequency
//! discriminator followed by per-symbol integration.

use std::f32::consts::{PI, TAU};

const PREAMBLE: u8 = 0xAA;
const PREAMBLE_LEN: usize = 4;
const SYNC: [u8; 2] = [0x2D, 0xD4];

/// One complex baseband sample.
#[derive(Clone, Copy, Debug)]
pub struct Iq {
    pub i: f32,
    pub q: f32,
}

/// Physical-layer parameters. Defaults give 4 samples/symbol with a modulation
/// index of 1.0 — comfortable to demodulate and small enough that a chat-sized
/// frame fits in a handful of datagrams once it becomes IQ.
#[derive(Debug, Clone, Copy)]
pub struct ModemParams {
    pub sample_rate: f32,
    pub baud: f32,
    pub freq_dev: f32,
}

impl Default for ModemParams {
    fn default() -> Self {
        Self {
            sample_rate: 9600.0,
            baud: 2400.0,
            freq_dev: 1200.0,
        }
    }
}

pub struct Modem {
    params: ModemParams,
    sps: usize,
}

impl Modem {
    pub fn new(params: ModemParams) -> Self {
        let sps = (params.sample_rate / params.baud).round().max(1.0) as usize;
        Self { params, sps }
    }

    /// Modulate a payload into baseband IQ samples ready for the channel.
    pub fn modulate(&self, payload: &[u8]) -> Vec<Iq> {
        let frame = build_frame(payload);
        let bits = to_bits(&frame);

        let step = TAU * self.params.freq_dev / self.params.sample_rate;
        let mut phase = 0.0f32;
        let mut out = Vec::with_capacity(bits.len() * self.sps);
        for bit in bits {
            let delta = if bit { step } else { -step };
            for _ in 0..self.sps {
                phase = wrap_phase(phase + delta);
                out.push(Iq {
                    i: phase.cos(),
                    q: phase.sin(),
                });
            }
        }
        out
    }

    /// Recover a payload from a burst of IQ samples, or `None` if no valid
    /// frame is present (silence, noise, or a failed CRC).
    pub fn demodulate(&self, samples: &[Iq]) -> Option<Vec<u8>> {
        if samples.len() < 2 * self.sps {
            return None;
        }
        let freqs = discriminate(samples);
        let bits = self.slice_symbols(&freqs);
        deframe(&bits)
    }

    /// Average the instantaneous frequency across each symbol period and decide
    /// the bit from its sign.
    fn slice_symbols(&self, freqs: &[f32]) -> Vec<bool> {
        let symbols = freqs.len() / self.sps;
        let mut bits = Vec::with_capacity(symbols);
        for k in 0..symbols {
            let window = &freqs[k * self.sps..(k + 1) * self.sps];
            let sum: f32 = window.iter().sum();
            bits.push(sum > 0.0);
        }
        bits
    }
}

/// Quadrature frequency discriminator: the angle of `conj(prev) * cur` is the
/// phase advance between consecutive samples, i.e. the instantaneous frequency.
fn discriminate(samples: &[Iq]) -> Vec<f32> {
    let mut freqs = vec![0.0f32; samples.len()];
    for n in 1..samples.len() {
        let a = samples[n - 1];
        let b = samples[n];
        let cross = a.i * b.q - a.q * b.i;
        let dot = a.i * b.i + a.q * b.q;
        freqs[n] = cross.atan2(dot);
    }
    if samples.len() > 1 {
        freqs[0] = freqs[1];
    }
    freqs
}

fn build_frame(payload: &[u8]) -> Vec<u8> {
    let len = payload.len().min(u16::MAX as usize);
    let payload = &payload[..len];

    // body = len || payload; crc covers the body so the receiver can trust len.
    let mut body = Vec::with_capacity(2 + len);
    body.extend_from_slice(&(len as u16).to_be_bytes());
    body.extend_from_slice(payload);
    let crc = crc16(&body);

    let mut frame = Vec::with_capacity(PREAMBLE_LEN + SYNC.len() + body.len() + 2);
    frame.extend(std::iter::repeat_n(PREAMBLE, PREAMBLE_LEN));
    frame.extend_from_slice(&SYNC);
    frame.extend_from_slice(&body);
    frame.extend_from_slice(&crc.to_be_bytes());
    frame
}

fn deframe(bits: &[bool]) -> Option<Vec<u8>> {
    let sync = to_bits(&SYNC);
    let body_start = find_subsequence(bits, &sync)? + sync.len();

    let len = bits_to_u16(bits.get(body_start..body_start + 16)?);
    let crc_start = body_start + 16 + len as usize * 8;
    let crc = bits_to_u16(bits.get(crc_start..crc_start + 16)?);

    let body = bits_to_bytes(bits.get(body_start..crc_start)?);
    if crc16(&body) != crc {
        return None;
    }
    Some(body[2..].to_vec())
}

fn wrap_phase(mut phase: f32) -> f32 {
    if phase > PI {
        phase -= TAU;
    } else if phase < -PI {
        phase += TAU;
    }
    phase
}

fn to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &byte in bytes {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1 == 1);
        }
    }
    bits
}

fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| chunk.iter().fold(0u8, |acc, &bit| (acc << 1) | bit as u8))
        .collect()
}

fn bits_to_u16(bits: &[bool]) -> u16 {
    bits.iter().fold(0u16, |acc, &bit| (acc << 1) | bit as u16)
}

fn find_subsequence(haystack: &[bool], needle: &[bool]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&start| &haystack[start..start + needle.len()] == needle)
}

/// CRC-16/CCITT-FALSE. Kept local so the radio link layer owns its own
/// integrity check and stays independent of the application codec.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modem() -> Modem {
        Modem::new(ModemParams::default())
    }

    #[test]
    fn round_trips_a_payload() {
        let modem = modem();
        let payload = b"relay node is up on channel 7";
        let samples = modem.modulate(payload);
        assert_eq!(modem.demodulate(&samples).as_deref(), Some(&payload[..]));
    }

    #[test]
    fn round_trips_a_large_payload() {
        let modem = modem();
        let payload: Vec<u8> = (0..1500).map(|i| (i % 251) as u8).collect();
        let samples = modem.modulate(&payload);
        assert_eq!(modem.demodulate(&samples), Some(payload));
    }

    #[test]
    fn round_trips_empty_payload() {
        let modem = modem();
        let samples = modem.modulate(b"");
        assert_eq!(modem.demodulate(&samples).as_deref(), Some(&b""[..]));
    }

    #[test]
    fn survives_mild_noise() {
        let modem = modem();
        let payload = b"signal clean, holding";
        let mut samples = modem.modulate(payload);
        // Deterministic low-amplitude perturbation across every sample.
        for (n, s) in samples.iter_mut().enumerate() {
            let jitter = ((n as f32) * 0.3).sin() * 0.05;
            s.i += jitter;
            s.q -= jitter;
        }
        assert_eq!(modem.demodulate(&samples).as_deref(), Some(&payload[..]));
    }

    #[test]
    fn silence_yields_nothing() {
        let modem = modem();
        let quiet = vec![Iq { i: 1.0, q: 0.0 }; 256];
        assert_eq!(modem.demodulate(&quiet), None);
    }
}
