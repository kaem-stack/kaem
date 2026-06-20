//! The crypto seam: every cryptographic operation this crate needs, behind a
//! trait it owns. `kaem-mesh` never depends on a crypto crate directly — a
//! binary builds a [`CryptoOps`] impl (typically backed by `kaem-crypto`) and
//! hands it to [`MeshNode::new`](crate::MeshNode::new), the same way
//! `kaem-link` is handed a `Box<dyn Channel>` rather than naming one.

use anyhow::Result;

/// A node's keypair: the public key it hands out to be paired against, and
/// the secret key it uses to accept pairing requests.
pub struct KeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

pub trait CryptoOps {
    /// Generate a fresh identity keypair.
    fn generate_keypair(&self) -> Result<KeyPair>;
    /// Seal `plaintext` for the holder of the secret key matching
    /// `public_key`, via a fresh key-encapsulation.
    fn hybrid_encrypt(&self, public_key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>>;
    /// Recover a message sealed with [`hybrid_encrypt`](Self::hybrid_encrypt)
    /// using the matching `secret_key`.
    fn hybrid_decrypt(&self, secret_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>>;
    /// Seal `plaintext` directly under an already-agreed symmetric `key`.
    fn symmetric_seal(&self, key: &[u8; 32], plaintext: &[u8]) -> Vec<u8>;
    /// Recover the plaintext sealed by
    /// [`symmetric_seal`](Self::symmetric_seal) under the same `key`.
    fn symmetric_open(&self, key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>>;
}
