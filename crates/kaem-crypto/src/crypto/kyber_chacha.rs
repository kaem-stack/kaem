use anyhow::{anyhow, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use ml_kem::{
    kem::{Decapsulate, Encapsulate, Key},
    ml_kem_768,
};
use zeroize::Zeroizing;

use super::algorithm::EncryptionAlgorithm;
use crate::event::{DecryptedEvent, EncryptedEvent};

// Wire format: [KEM ciphertext: 1088 bytes] [nonce: 12 bytes] [AEAD ciphertext + tag]
const KEM_CT_SIZE: usize = 1088;
const NONCE_SIZE: usize = 12;

pub struct KyberChaCha;

impl EncryptionAlgorithm for KyberChaCha {
    fn encrypt(&self, public_key: &[u8], plaintext: &[u8]) -> Result<EncryptedEvent> {
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

        Ok(EncryptedEvent { ciphertext })
    }

    fn decrypt(&self, secret_key: &[u8], ciphertext: &[u8]) -> Result<DecryptedEvent> {
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

        Ok(DecryptedEvent { plaintext })
    }
}
