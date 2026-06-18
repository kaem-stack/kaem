//! End-to-end checks over the public API: generate a keypair, then confirm
//! sealing and opening behave — and that tampering or the wrong key is rejected.

use kaem_crypto::{crypto, keys};

fn keypair() -> keys::KeysGeneratedEvent {
    keys::generate(&keys::KeyGenConfig::new(".".into())).expect("keygen")
}

#[test]
fn round_trip() {
    let pair = keypair();
    let cfg = crypto::EncryptConfig::default();
    let msg = b"relay node is up on channel 7";

    let sealed = crypto::encrypt(&cfg, &pair.public_key, msg).expect("encrypt");
    let opened = crypto::decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).expect("decrypt");

    assert_eq!(opened.plaintext, msg);
}

#[test]
fn empty_message_round_trips() {
    let pair = keypair();
    let cfg = crypto::EncryptConfig::default();

    let sealed = crypto::encrypt(&cfg, &pair.public_key, b"").expect("encrypt");
    let opened = crypto::decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).expect("decrypt");

    assert!(opened.plaintext.is_empty());
}

#[test]
fn generated_keys_have_expected_sizes() {
    let pair = keypair();
    assert_eq!(pair.public_key.len(), 1184); // ML-KEM-768 encapsulation key
    assert_eq!(pair.secret_key.len(), 64); // seed-form decapsulation key
}

#[test]
fn tampered_ciphertext_is_rejected() {
    let pair = keypair();
    let cfg = crypto::EncryptConfig::default();

    let mut sealed = crypto::encrypt(&cfg, &pair.public_key, b"top secret").expect("encrypt");
    // Flip a byte in the AEAD payload; the auth tag must catch it.
    let last = sealed.ciphertext.len() - 1;
    sealed.ciphertext[last] ^= 0xFF;

    assert!(crypto::decrypt(&cfg, &pair.secret_key, &sealed.ciphertext).is_err());
}

#[test]
fn wrong_key_cannot_decrypt() {
    let alice = keypair();
    let bob = keypair();
    let cfg = crypto::EncryptConfig::default();

    let sealed = crypto::encrypt(&cfg, &alice.public_key, b"for alice only").expect("encrypt");
    assert!(crypto::decrypt(&cfg, &bob.secret_key, &sealed.ciphertext).is_err());
}

#[test]
fn short_ciphertext_is_rejected() {
    let pair = keypair();
    let cfg = crypto::EncryptConfig::default();
    assert!(crypto::decrypt(&cfg, &pair.secret_key, b"too short").is_err());
}
