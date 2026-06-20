//! A node's ML-KEM-768 identity keypair.

use anyhow::Result;

use crate::keys::{self, KeyGenConfig};

/// A node's identity: the public key it hands out to be paired against, and
/// the secret key it uses to accept pairing requests. Held only in memory —
/// sandbox nodes never persist identity to disk.
#[derive(Debug, Clone)]
pub struct Identity {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

/// Generate a fresh in-memory ML-KEM-768 identity. Never touches disk —
/// sandbox nodes hold their identity only for the lifetime of the process.
pub fn generate_identity() -> Result<Identity> {
    let generated = keys::generate(&KeyGenConfig::default())?;
    Ok(Identity {
        public_key: generated.public_key,
        secret_key: generated.secret_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_an_ml_kem_768_sized_keypair() {
        let identity = generate_identity().expect("generate");
        assert_eq!(identity.public_key.len(), 1184);
        assert_eq!(identity.secret_key.len(), 64);
    }

    #[test]
    fn each_call_produces_a_distinct_keypair() {
        let a = generate_identity().expect("generate");
        let b = generate_identity().expect("generate");
        assert_ne!(a.public_key, b.public_key);
    }
}
