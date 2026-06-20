//! Direct symmetric sealing against an already-agreed 32-byte key.
//!
//! [`crate::crypto`] is for first contact: it does a fresh ML-KEM
//! encapsulation per call to agree a key, which makes it unsuitable as a
//! reusable key store. Once two parties already share a key — e.g. a chatroom
//! key minted once by a pairing handshake — there's no KEM step left to do;
//! this module just runs the AEAD directly.
//!
//! # Wire format
//!
//! ```text
//! [ ChaCha20-Poly1305 nonce: 12 bytes ]
//! [ encrypted payload + 16-byte auth tag ]
//! ```

use anyhow::{Result, anyhow};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};

const NONCE_SIZE: usize = 12;

/// Seal `plaintext` under `key`, returning `nonce(12) || ciphertext+tag`.
pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).expect("key is exactly 32 bytes");

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    getrandom::fill(&mut nonce_bytes).expect("system rng must be available");

    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .expect("chacha20poly1305 encryption is infallible for valid inputs");

    let mut sealed = Vec::with_capacity(NONCE_SIZE + encrypted.len());
    sealed.extend_from_slice(&nonce_bytes);
    sealed.extend_from_slice(&encrypted);
    sealed
}

/// Recover the plaintext sealed by [`seal`] under the same `key`.
pub fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < NONCE_SIZE {
        return Err(anyhow!("sealed payload too short to contain a nonce"));
    }
    let (nonce_bytes, encrypted) = sealed.split_at(NONCE_SIZE);

    let cipher = ChaCha20Poly1305::new_from_slice(key).expect("key is exactly 32 bytes");
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), encrypted)
        .map_err(|_| anyhow!("decryption failed: wrong key or corrupted ciphertext"))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).unwrap();
        k
    }

    #[test]
    fn round_trip() {
        let key = key();
        let msg = b"chatroom established, key 0x42";
        let sealed = seal(&key, msg);
        let opened = open(&key, &sealed).expect("open");
        assert_eq!(opened, msg);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = key();
        let mut sealed = seal(&key, b"top secret");
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(open(&key, &sealed).is_err());
    }

    #[test]
    fn wrong_key_cannot_decrypt() {
        let a = key();
        let b = key();
        let sealed = seal(&a, b"for a only");
        assert!(open(&b, &sealed).is_err());
    }

    #[test]
    fn empty_message_round_trips() {
        let key = key();
        let sealed = seal(&key, b"");
        let opened = open(&key, &sealed).expect("open");
        assert!(opened.is_empty());
    }

    #[test]
    fn short_sealed_payload_is_rejected() {
        let key = key();
        assert!(open(&key, b"short").is_err());
    }
}
