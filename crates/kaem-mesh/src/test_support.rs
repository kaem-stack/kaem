//! A self-contained, test-only [`CryptoOps`] implementation. `kaem-mesh`
//! never names another `kaem-*` crate — not even in `[dev-dependencies]` —
//! so this duplicates the same ML-KEM-768 + ChaCha20-Poly1305 primitives
//! `kaem-crypto` carries, the same way the wire framing in `envelope.rs` is
//! duplicated rather than shared. A binary is the only place that wires the
//! real `kaem-crypto` crate in.

use anyhow::{Result, anyhow};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use ml_kem::{
    kem::{Decapsulate, Encapsulate, Generate, Key, KeyExport},
    ml_kem_768,
};
use rand::rngs::SysRng;
use zeroize::Zeroizing;

use crate::crypto_ops::{CryptoOps, KeyPair};

const KEM_CT_SIZE: usize = 1088;
const NONCE_SIZE: usize = 12;

pub struct TestCrypto;

impl CryptoOps for TestCrypto {
    fn generate_keypair(&self) -> Result<KeyPair> {
        let mut rng = SysRng;
        let dk = ml_kem_768::DecapsulationKey::try_generate_from_rng(&mut rng)
            .map_err(|e| anyhow!("rng error: {e:?}"))?;
        let ek = dk.encapsulation_key();

        Ok(KeyPair {
            public_key: ek.to_bytes().to_vec(),
            secret_key: dk.to_bytes().to_vec(),
        })
    }

    fn hybrid_encrypt(&self, public_key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        let ek_bytes: &Key<ml_kem_768::EncapsulationKey> = public_key
            .try_into()
            .map_err(|_| anyhow!("invalid public key: expected 1184 bytes for ML-KEM-768"))?;
        let ek = ml_kem_768::EncapsulationKey::new(ek_bytes)
            .map_err(|_| anyhow!("malformed public key"))?;

        let (kem_ct, shared_key) = ek.encapsulate();
        let shared_key = Zeroizing::new(shared_key.to_vec());

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        getrandom::fill(&mut nonce_bytes).map_err(|e| anyhow!("nonce generation failed: {e}"))?;

        let cipher = ChaCha20Poly1305::new_from_slice(&shared_key)
            .map_err(|_| anyhow!("invalid shared key length"))?;
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|e| anyhow!("symmetric encryption failed: {e}"))?;

        let mut ciphertext = kem_ct.to_vec();
        ciphertext.extend_from_slice(&nonce_bytes);
        ciphertext.extend_from_slice(&encrypted);

        Ok(ciphertext)
    }

    fn hybrid_decrypt(&self, secret_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < KEM_CT_SIZE + NONCE_SIZE {
            return Err(anyhow!("ciphertext too short to be valid"));
        }

        let (kem_ct_bytes, rest) = ciphertext.split_at(KEM_CT_SIZE);
        let (nonce_bytes, encrypted) = rest.split_at(NONCE_SIZE);

        let seed: &Key<ml_kem_768::DecapsulationKey> = secret_key
            .try_into()
            .map_err(|_| anyhow!("invalid secret key: expected 64 bytes"))?;
        let dk = ml_kem_768::DecapsulationKey::from_seed(*seed);

        let kem_ct: &ml_kem_768::Ciphertext = kem_ct_bytes
            .try_into()
            .map_err(|_| anyhow!("invalid KEM ciphertext length"))?;
        let shared_key = Zeroizing::new(dk.decapsulate(kem_ct).to_vec());

        let cipher = ChaCha20Poly1305::new_from_slice(&shared_key)
            .map_err(|_| anyhow!("invalid shared key length"))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), encrypted)
            .map_err(|_| anyhow!("decryption failed: wrong key or corrupted ciphertext"))?;

        Ok(plaintext)
    }

    fn symmetric_seal(&self, key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
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

    fn symmetric_open(&self, key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < NONCE_SIZE {
            return Err(anyhow!("sealed payload too short to contain a nonce"));
        }
        let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_SIZE);

        let cipher = ChaCha20Poly1305::new_from_slice(key).expect("key is exactly 32 bytes");
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), encrypted)
            .map_err(|_| anyhow!("decryption failed: wrong key or corrupted ciphertext"))?;

        Ok(plaintext)
    }
}
