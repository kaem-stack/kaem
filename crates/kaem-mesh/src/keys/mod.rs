//! Keypair generation and persistence.

mod algorithm;
mod config;
mod factory;
mod ml_kem;

pub use algorithm::KemAlgorithm;
pub use config::{Algorithm, KeyGenConfig};

pub use crate::event::KeysGeneratedEvent;

use anyhow::Result;

/// Generate a fresh keypair for the configured algorithm.
pub fn generate(config: &KeyGenConfig) -> Result<KeysGeneratedEvent> {
    factory::create(&config.algorithm).generate()
}
