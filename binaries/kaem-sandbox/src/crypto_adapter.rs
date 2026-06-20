//! Wires `kaem-mesh`'s [`CryptoOps`] seam to `kaem-crypto`. Neither library
//! crate names the other; this binary is the only place that does.

use anyhow::Result;
use kaem_mesh::{CryptoOps, KeyPair};

pub struct KaemCrypto;

impl CryptoOps for KaemCrypto {
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
