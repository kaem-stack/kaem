use anyhow::Result;

use crate::event::KeysGeneratedEvent;

/// A pluggable key-encapsulation mechanism. Implementations know how to produce
/// a keypair in their native byte encoding.
pub trait KemAlgorithm {
    fn generate(&self) -> Result<KeysGeneratedEvent>;
}
