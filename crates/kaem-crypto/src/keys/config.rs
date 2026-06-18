use std::path::PathBuf;

/// The cryptographic scheme to generate keys for. ML-KEM-768 is the only one
/// today; the enum exists so others can join without breaking callers.
#[derive(Debug, Clone, Default)]
pub enum Algorithm {
    #[default]
    MlKem768,
}

impl Algorithm {
    pub fn name(&self) -> &'static str {
        match self {
            Algorithm::MlKem768 => "ML-KEM-768",
        }
    }
}

/// Where to write a generated keypair and which algorithm to use.
pub struct KeyGenConfig {
    pub algorithm: Algorithm,
    pub out_dir: PathBuf,
}

impl KeyGenConfig {
    pub fn new(out_dir: PathBuf) -> Self {
        Self {
            algorithm: Algorithm::default(),
            out_dir,
        }
    }

    pub fn with_algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }
}
