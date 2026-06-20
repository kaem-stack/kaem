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
