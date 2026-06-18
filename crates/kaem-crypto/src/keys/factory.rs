use super::{algorithm::KemAlgorithm, config::Algorithm, ml_kem::MlKem768Kem};

pub fn create(algorithm: &Algorithm) -> Box<dyn KemAlgorithm> {
    match algorithm {
        Algorithm::MlKem768 => Box::new(MlKem768Kem),
    }
}
