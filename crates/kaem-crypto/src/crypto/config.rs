use crate::keys::Algorithm;

/// Selects which [`EncryptionAlgorithm`](super::EncryptionAlgorithm) to use.
#[derive(Debug, Clone, Default)]
pub struct EncryptConfig {
    pub algorithm: Algorithm,
}

impl EncryptConfig {
    pub fn with_algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }
}
