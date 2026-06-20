use anyhow::Result;

use crate::KeyPair;

/// A pluggable hybrid scheme: a key-encapsulation mechanism paired with an
/// authenticated cipher, plus the keygen that backs it. Each implementation
/// defines its own wire frame between the KEM and the AEAD step.
pub trait Scheme {
    fn generate_keypair(&self) -> Result<KeyPair>;
    fn encrypt(&self, public_key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&self, secret_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>>;
}
