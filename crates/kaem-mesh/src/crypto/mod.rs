//! Message encryption and decryption against an ML-KEM-768 keypair.

mod algorithm;
mod config;
mod factory;
mod kyber_chacha;

pub use algorithm::EncryptionAlgorithm;
pub use config::EncryptConfig;

pub use crate::event::{DecryptedEvent, EncryptedEvent};

use anyhow::Result;

/// Seal `plaintext` for the holder of the secret key matching `public_key`.
pub fn encrypt(
    config: &EncryptConfig,
    public_key: &[u8],
    plaintext: &[u8],
) -> Result<EncryptedEvent> {
    factory::create(&config.algorithm).encrypt(public_key, plaintext)
}

/// Recover a message sealed with [`encrypt`] using the matching `secret_key`.
pub fn decrypt(
    config: &EncryptConfig,
    secret_key: &[u8],
    ciphertext: &[u8],
) -> Result<DecryptedEvent> {
    factory::create(&config.algorithm).decrypt(secret_key, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::{EncryptConfig, decrypt, encrypt};
    use crate::keys::{self, KeyGenConfig, KeysGeneratedEvent};

    fn keypair() -> KeysGeneratedEvent {
        keys::generate(&KeyGenConfig::default()).expect("keygen")
    }

    #[test]
    fn round_trip() {
        let pair = keypair();
        let cfg = EncryptConfig::default();
        let msg = b"relay node is up on channel 7";

        let sealed = encrypt(&cfg, &pair.public_key, msg).expect("encrypt");
        let opened = decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).expect("decrypt");

        assert_eq!(opened.plaintext, msg);
    }

    #[test]
    fn empty_message_round_trips() {
        let pair = keypair();
        let cfg = EncryptConfig::default();

        let sealed = encrypt(&cfg, &pair.public_key, b"").expect("encrypt");
        let opened = decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).expect("decrypt");

        assert!(opened.plaintext.is_empty());
    }

    #[test]
    fn generated_keys_have_expected_sizes() {
        let pair = keypair();
        assert_eq!(pair.public_key.len(), 1184); // ML-KEM-768 encapsulation key
        assert_eq!(pair.secret_key.len(), 64); // seed-form decapsulation key
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let pair = keypair();
        let cfg = EncryptConfig::default();

        let mut sealed = encrypt(&cfg, &pair.public_key, b"top secret").expect("encrypt");
        // Flip a byte in the AEAD payload; the auth tag must catch it.
        let last = sealed.ciphertext.len() - 1;
        sealed.ciphertext[last] ^= 0xFF;

        assert!(decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).is_err());
    }

    #[test]
    fn wrong_key_cannot_decrypt() {
        let alice = keypair();
        let bob = keypair();
        let cfg = EncryptConfig::default();

        let sealed = encrypt(&cfg, &alice.public_key, b"for alice only").expect("encrypt");
        assert!(decrypt(&cfg, &bob.secret_key, &sealed.ciphertext).is_err());
    }

    #[test]
    fn short_ciphertext_is_rejected() {
        let pair = keypair();
        let cfg = EncryptConfig::default();
        assert!(decrypt(&cfg, &pair.secret_key, b"too short").is_err());
    }
}
