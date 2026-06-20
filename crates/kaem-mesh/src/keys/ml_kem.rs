use anyhow::Result;
use ml_kem::{
    kem::{Generate, KeyExport},
    ml_kem_768,
};
use rand::rngs::SysRng;

use super::algorithm::KemAlgorithm;
use crate::event::KeysGeneratedEvent;

pub struct MlKem768Kem;

impl KemAlgorithm for MlKem768Kem {
    fn generate(&self) -> Result<KeysGeneratedEvent> {
        let mut rng = SysRng;
        let dk = ml_kem_768::DecapsulationKey::try_generate_from_rng(&mut rng)
            .map_err(|e| anyhow::anyhow!("rng error: {e:?}"))?;
        let ek = dk.encapsulation_key();

        Ok(KeysGeneratedEvent {
            public_key: ek.to_bytes().to_vec(),
            secret_key: dk.to_bytes().to_vec(),
        })
    }
}
