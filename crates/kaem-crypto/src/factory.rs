use crate::algorithm::Scheme;
use crate::config::Algorithm;
use crate::ml_kem_chacha::MlKem768ChaCha;

pub fn create(algorithm: &Algorithm) -> Box<dyn Scheme> {
    match algorithm {
        Algorithm::MlKem768 => Box::new(MlKem768ChaCha),
    }
}
