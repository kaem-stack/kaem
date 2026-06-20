use crate::keys::Algorithm;

use super::{algorithm::EncryptionAlgorithm, kyber_chacha::KyberChaCha};

pub fn create(algorithm: &Algorithm) -> Box<dyn EncryptionAlgorithm> {
    match algorithm {
        Algorithm::MlKem768 => Box::new(KyberChaCha),
    }
}
