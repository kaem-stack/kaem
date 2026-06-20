//! Test-only [`CryptoOps`] backed by `kaem-crypto`. Production code never
//! names `kaem-crypto` — a binary supplies the real adapter — but this
//! crate's own tests need a working implementation to exercise pairing and
//! sealing end to end. `kaem-crypto` is a `[dev-dependencies]`-only
//! dependency for exactly this reason: it never appears in this crate's
//! compiled library.

use anyhow::Result;

use crate::crypto_ops::{CryptoOps, KeyPair};

pub struct TestCrypto;

impl CryptoOps for TestCrypto {
    fn generate_keypair(&self) -> Result<KeyPair> {
        let pair = kaem_crypto::generate_keypair()?;
        Ok(KeyPair {
            public_key: pair.public_key,
            secret_key: pair.secret_key,
        })
    }

    fn hybrid_encrypt(&self, public_key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        kaem_crypto::hybrid_encrypt(public_key, plaintext)
    }

    fn hybrid_decrypt(&self, secret_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        kaem_crypto::hybrid_decrypt(secret_key, ciphertext)
    }

    fn symmetric_seal(&self, key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
        kaem_crypto::symmetric_seal(key, plaintext)
    }

    fn symmetric_open(&self, key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>> {
        kaem_crypto::symmetric_open(key, ciphertext)
    }
}
