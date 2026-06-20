/// The cryptographic scheme backing keygen and hybrid encryption. ML-KEM-768
/// is the only one today; the enum exists so others can join without
/// breaking callers.
#[derive(Debug, Clone, Default)]
pub enum Algorithm {
    #[default]
    MlKem768,
}
