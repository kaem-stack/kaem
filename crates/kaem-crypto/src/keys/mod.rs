//! Keypair generation and persistence.

mod algorithm;
mod config;
mod factory;
mod ml_kem;

pub use algorithm::KemAlgorithm;
pub use config::{Algorithm, KeyGenConfig};

pub use crate::event::KeysGeneratedEvent;

use anyhow::Result;
use std::fs;

/// Generate a fresh keypair for the configured algorithm.
pub fn generate(config: &KeyGenConfig) -> Result<KeysGeneratedEvent> {
    factory::create(&config.algorithm).generate()
}

/// Write a keypair to `config.out_dir` as `kaem.pub` (public) and `kaem.key`
/// (secret), creating the directory if needed.
pub fn save(keypair: &KeysGeneratedEvent, config: &KeyGenConfig) -> Result<()> {
    fs::create_dir_all(&config.out_dir)?;
    fs::write(config.out_dir.join("kaem.pub"), &keypair.public_key)?;
    fs::write(config.out_dir.join("kaem.key"), &keypair.secret_key)?;
    Ok(())
}
