use anyhow::Result;

use crate::event::{DecryptedEvent, EncryptedEvent};

/// A pluggable encryption scheme. Each implementation pairs a key-encapsulation
/// mechanism with an authenticated cipher and defines the wire frame between them.
pub trait EncryptionAlgorithm {
    fn encrypt(&self, public_key: &[u8], plaintext: &[u8]) -> Result<EncryptedEvent>;
    fn decrypt(&self, secret_key: &[u8], ciphertext: &[u8]) -> Result<DecryptedEvent>;
}
