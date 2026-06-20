/// The cryptographic scheme to generate keys for. ML-KEM-768 is the only one
/// today; the enum exists so others can join without breaking callers.
#[derive(Debug, Clone, Default)]
pub enum Algorithm {
    #[default]
    MlKem768,
}

/// Which algorithm to generate a keypair for.
#[derive(Debug, Clone, Default)]
pub struct KeyGenConfig {
    pub algorithm: Algorithm,
}
