use crate::keys::Algorithm;

/// Selects which [`EncryptionAlgorithm`](super::EncryptionAlgorithm) to use.
#[derive(Debug, Clone, Default)]
pub struct EncryptConfig {
    pub algorithm: Algorithm,
}
