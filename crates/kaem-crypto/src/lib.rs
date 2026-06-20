//! Cryptographic primitives backing the mesh's pairing handshake and chatroom
//! encryption: ML-KEM-768 keygen, a hybrid KEM+AEAD scheme for first contact,
//! and direct AEAD sealing once two parties already share a key. This crate
//! knows nothing of chatrooms, identities, or relaying — it only turns keys
//! and bytes into other bytes.

mod algorithm;
mod config;
mod factory;
mod ml_kem_chacha;
mod symmetric;

use anyhow::Result;

pub use config::Algorithm;

/// A freshly generated keypair: the public encapsulation key and the secret
/// decapsulation key, as raw bytes.
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

/// Generate a fresh keypair for the default algorithm.
pub fn generate_keypair() -> Result<KeyPair> {
    factory::create(&Algorithm::default()).generate_keypair()
}

/// Seal `plaintext` for the holder of the secret key matching `public_key`,
/// via a fresh KEM encapsulation.
pub fn hybrid_encrypt(public_key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    factory::create(&Algorithm::default()).encrypt(public_key, plaintext)
}

/// Recover a message sealed with [`hybrid_encrypt`] using the matching
/// `secret_key`.
pub fn hybrid_decrypt(secret_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    factory::create(&Algorithm::default()).decrypt(secret_key, ciphertext)
}

/// Seal `plaintext` directly under an already-agreed symmetric `key`.
pub fn symmetric_seal(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    symmetric::seal(key, plaintext)
}

/// Recover the plaintext sealed by [`symmetric_seal`] under the same `key`.
pub fn symmetric_open(key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>> {
    symmetric::open(key, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_round_trip() {
        let pair = generate_keypair().expect("keygen");
        let msg = b"relay node is up on channel 7";

        let sealed = hybrid_encrypt(&pair.public_key, msg).expect("encrypt");
        let opened = hybrid_decrypt(&pair.secret_key, &sealed).expect("decrypt");

        assert_eq!(opened, msg);
    }

    #[test]
    fn hybrid_empty_message_round_trips() {
        let pair = generate_keypair().expect("keygen");

        let sealed = hybrid_encrypt(&pair.public_key, b"").expect("encrypt");
        let opened = hybrid_decrypt(&pair.secret_key, &sealed).expect("decrypt");

        assert!(opened.is_empty());
    }

    #[test]
    fn generated_keys_have_expected_sizes() {
        let pair = generate_keypair().expect("keygen");
        assert_eq!(pair.public_key.len(), 1184); // ML-KEM-768 encapsulation key
        assert_eq!(pair.secret_key.len(), 64); // seed-form decapsulation key
    }

    #[test]
    fn hybrid_tampered_ciphertext_is_rejected() {
        let pair = generate_keypair().expect("keygen");

        let mut sealed = hybrid_encrypt(&pair.public_key, b"top secret").expect("encrypt");
        // Flip a byte in the AEAD payload; the auth tag must catch it.
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;

        assert!(hybrid_decrypt(&pair.secret_key, &sealed).is_err());
    }

    #[test]
    fn hybrid_wrong_key_cannot_decrypt() {
        let alice = generate_keypair().expect("keygen");
        let bob = generate_keypair().expect("keygen");

        let sealed = hybrid_encrypt(&alice.public_key, b"for alice only").expect("encrypt");
        assert!(hybrid_decrypt(&bob.secret_key, &sealed).is_err());
    }

    #[test]
    fn hybrid_short_ciphertext_is_rejected() {
        let pair = generate_keypair().expect("keygen");
        assert!(hybrid_decrypt(&pair.secret_key, b"too short").is_err());
    }

    #[test]
    fn each_keypair_is_distinct() {
        let a = generate_keypair().expect("keygen");
        let b = generate_keypair().expect("keygen");
        assert_ne!(a.public_key, b.public_key);
    }
}
