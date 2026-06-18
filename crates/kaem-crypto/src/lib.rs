//! kaem-crypto: post-quantum hybrid encryption for kaem.
//!
//! This is khyber's scheme packaged as a standalone library: CRYSTALS-Kyber
//! (ML-KEM-768) for key encapsulation paired with ChaCha20-Poly1305 for
//! symmetric encryption. The KEM protects a fresh shared key for every message;
//! that key encrypts the payload under an AEAD that also authenticates it.
//!
//! The crate is split the same way khyber is — [`keys`] generates and persists
//! keypairs, [`crypto`] encrypts and decrypts against them — with both sides
//! hidden behind a small algorithm trait so a future scheme can slot in beside
//! ML-KEM-768 without touching callers.
//!
//! # Wire format
//!
//! [`crypto::encrypt`] produces a self-describing frame; no length prefix is
//! needed because the KEM ciphertext is fixed size:
//!
//! ```text
//! [ ML-KEM-768 ciphertext: 1088 bytes ]
//! [ ChaCha20-Poly1305 nonce:  12 bytes ]
//! [ encrypted payload + 16-byte auth tag ]
//! ```
//!
//! # Example
//!
//! ```
//! use kaem_crypto::{crypto, keys};
//!
//! let pair = keys::generate(&keys::KeyGenConfig::new(".".into())).unwrap();
//!
//! let cfg = crypto::EncryptConfig::default();
//! let sealed = crypto::encrypt(&cfg, &pair.public_key, b"relay node is up").unwrap();
//! let opened = crypto::decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).unwrap();
//!
//! assert_eq!(opened.plaintext, b"relay node is up");
//! ```

pub mod crypto;
pub mod keys;

mod event;

pub use event::{DecryptedEvent, EncryptedEvent, KeysGeneratedEvent};
