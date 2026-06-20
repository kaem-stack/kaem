//! The pairing handshake: mint a chatroom (id + symmetric key) and seal it
//! for a specific peer's identity, using a real key-encapsulation via
//! [`CryptoOps::hybrid_encrypt`]. Whoever holds the matching secret key is
//! the only one who can recover the chatroom id and key from the sealed
//! bytes.

use anyhow::{Result, anyhow};

use super::identity::Identity;
use crate::crypto_ops::CryptoOps;

/// 8 bytes of chatroom id + 32 bytes of chatroom key.
const PLAINTEXT_LEN: usize = 8 + 32;

/// Mint a new chatroom for pairing with whoever holds `peer_pubkey`'s
/// matching secret key. Returns `(chatroom_id, key, sealed_for_peer)` — the
/// caller inserts `(chatroom_id, key)` into its own [`super::Store`]
/// immediately, and hands `sealed_for_peer` to the peer to recover the same
/// pair via [`accept`].
pub fn pair(
    crypto: &dyn CryptoOps,
    local_identity: &Identity,
    peer_pubkey: &[u8],
) -> Result<(u64, [u8; 32], Vec<u8>)> {
    let _ = local_identity; // pairing only needs the peer's public key to seal

    let chatroom_id: u64 = rand::random();
    let mut key = [0u8; 32];
    rand::fill(&mut key);

    let mut plaintext = Vec::with_capacity(PLAINTEXT_LEN);
    plaintext.extend_from_slice(&chatroom_id.to_be_bytes());
    plaintext.extend_from_slice(&key);

    let sealed = crypto.hybrid_encrypt(peer_pubkey, &plaintext)?;

    Ok((chatroom_id, key, sealed))
}

/// Recover the `(chatroom_id, key)` pair sealed by [`pair`], using
/// `local_identity`'s secret key.
pub fn accept(
    crypto: &dyn CryptoOps,
    local_identity: &Identity,
    sealed: &[u8],
) -> Result<(u64, [u8; 32])> {
    let plaintext = crypto.hybrid_decrypt(&local_identity.secret_key, sealed)?;

    if plaintext.len() != PLAINTEXT_LEN {
        return Err(anyhow!(
            "unexpected pairing payload length: {} (expected {PLAINTEXT_LEN})",
            plaintext.len()
        ));
    }

    let (id_bytes, key_bytes) = plaintext.split_at(8);
    let chatroom_id = u64::from_be_bytes(id_bytes.try_into().expect("split at 8"));
    let key: [u8; 32] = key_bytes.try_into().expect("split leaves 32 bytes");

    Ok((chatroom_id, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::generate_identity;
    use crate::test_support::TestCrypto;

    #[test]
    fn pair_and_accept_agree_on_chatroom_id_and_key() {
        let alice = generate_identity(&TestCrypto).expect("alice identity");
        let bob = generate_identity(&TestCrypto).expect("bob identity");

        let (chatroom_id, key, sealed) =
            pair(&TestCrypto, &alice, &bob.public_key).expect("alice mints the chatroom");

        let (accepted_id, accepted_key) = accept(&TestCrypto, &bob, &sealed).expect("bob accepts");

        assert_eq!(chatroom_id, accepted_id);
        assert_eq!(key, accepted_key);
    }

    #[test]
    fn wrong_identity_cannot_accept() {
        let alice = generate_identity(&TestCrypto).expect("alice identity");
        let bob = generate_identity(&TestCrypto).expect("bob identity");
        let mallory = generate_identity(&TestCrypto).expect("mallory identity");

        let (_, _, sealed) =
            pair(&TestCrypto, &alice, &bob.public_key).expect("alice mints the chatroom");

        assert!(accept(&TestCrypto, &mallory, &sealed).is_err());
    }
}
