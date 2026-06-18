//! Result types returned by the [`crypto`](crate::crypto) and [`keys`](crate::keys)
//! modules.
//!
//! In khyber these are the IPC event structs carried by `kaem-sdk`. As a
//! self-contained library kaem-crypto owns equivalents so it stays free of the
//! daemon transport while keeping the same shape.

/// A freshly generated keypair: the public encapsulation key and the secret
/// decapsulation key, as raw bytes.
#[derive(Debug, Clone)]
pub struct KeysGeneratedEvent {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

/// A sealed message — the full wire frame ready to transmit.
#[derive(Debug, Clone)]
pub struct EncryptedEvent {
    pub ciphertext: Vec<u8>,
}

/// A recovered message.
#[derive(Debug, Clone)]
pub struct DecryptedEvent {
    pub plaintext: Vec<u8>,
}
