//! NEP-413 signed-message verification (wallet login, HANDOFF D38).
//!
//! The wallet signs this, as defined in the NEP-413 spec:
//!
//! ```text
//! sha256( u32_le(2^31 + 413)                   ← prefix tag, so the bytes can
//!       ‖ borsh(message: String)                  never be a valid transaction
//!       ‖ nonce: [u8; 32]
//!       ‖ borsh(recipient: String)
//!       ‖ borsh(callback_url: Option<String>) )
//! ```
//!
//! Borsh is written by hand: for these 4 fields it is only "length (u32
//! little-endian) + bytes" for strings and a 0/1 byte for the option, which
//! is clearer than pulling in a serialization crate.
//!
//! Everything here is pure (no I/O). Checking that the key really belongs to
//! the account is a separate RPC call (`near_rpc`).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// NEP-413 prefix tag: 2^31 + 413. It makes the signed bytes start with a
/// value no NEAR transaction can start with, so a login signature can never
/// be replayed as a transaction.
const PREFIX_TAG: u32 = 2_147_484_061;

/// Prefix of NEAR ed25519 public keys, e.g. `ed25519:DCkbTMS8s4kq…`.
const ED25519_PREFIX: &str = "ed25519:";

/// Why a key or signature string could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum Nep413Error {
    /// Not `ed25519:<base58 of 32 bytes>`, or not a valid curve point.
    BadPublicKey,
    /// Not base64 of 64 bytes.
    BadSignature,
}

/// The Borsh bytes of the NEP-413 `Payload` (without the prefix tag).
pub fn payload_bytes(
    message: &str,
    nonce: &[u8; 32],
    recipient: &str,
    callback_url: Option<&str>,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(message.len() + recipient.len() + 48);
    push_borsh_string(&mut bytes, message);
    bytes.extend_from_slice(nonce);
    push_borsh_string(&mut bytes, recipient);
    match callback_url {
        None => bytes.push(0),
        Some(url) => {
            bytes.push(1);
            push_borsh_string(&mut bytes, url);
        }
    }
    bytes
}

/// The 32-byte hash the wallet signs: `sha256(prefix tag ‖ payload)`.
pub fn payload_hash(
    message: &str,
    nonce: &[u8; 32],
    recipient: &str,
    callback_url: Option<&str>,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PREFIX_TAG.to_le_bytes());
    hasher.update(payload_bytes(message, nonce, recipient, callback_url));
    hasher.finalize().into()
}

/// Reads a NEAR public key string `ed25519:<base58>`.
pub fn parse_public_key(public_key: &str) -> Result<VerifyingKey, Nep413Error> {
    let encoded = public_key
        .strip_prefix(ED25519_PREFIX)
        .ok_or(Nep413Error::BadPublicKey)?;
    let bytes = bs58::decode(encoded)
        .into_vec()
        .map_err(|_| Nep413Error::BadPublicKey)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| Nep413Error::BadPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| Nep413Error::BadPublicKey)
}

/// Reads a base64 signature (what NEAR Connect's `signMessage` returns).
pub fn parse_signature(signature: &str) -> Result<Signature, Nep413Error> {
    let bytes = STANDARD
        .decode(signature)
        .map_err(|_| Nep413Error::BadSignature)?;
    Signature::from_slice(&bytes).map_err(|_| Nep413Error::BadSignature)
}

/// `true` when `signature` is `public_key`'s signature of `hash`.
///
/// Uses `verify_strict`, which also rejects "malleable" signatures (a second,
/// different-looking signature for the same message) and weak keys.
pub fn verify(public_key: &VerifyingKey, signature: &Signature, hash: &[u8; 32]) -> bool {
    public_key.verify_strict(hash, signature).is_ok()
}

/// Borsh `String`: u32 little-endian byte length, then the UTF-8 bytes.
fn push_borsh_string(bytes: &mut Vec<u8>, value: &str) {
    // Our inputs are short constants and URLs; a string over 4 GiB cannot
    // reach here (request bodies are limited far below that).
    let length = u32::try_from(value.len()).expect("string longer than u32::MAX bytes");
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// A fixed test key (never used outside tests).
    pub(crate) fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[42u8; 32])
    }

    /// The NEAR string form of the test key's public half.
    pub(crate) fn public_key_string(key: &SigningKey) -> String {
        format!(
            "ed25519:{}",
            bs58::encode(key.verifying_key().as_bytes()).into_string()
        )
    }

    /// Signs like a wallet would and returns the base64 signature.
    pub(crate) fn wallet_sign(
        key: &SigningKey,
        message: &str,
        nonce: &[u8; 32],
        recipient: &str,
    ) -> String {
        let hash = payload_hash(message, nonce, recipient, None);
        STANDARD.encode(key.sign(&hash).to_bytes())
    }

    fn spec_nonce() -> [u8; 32] {
        std::array::from_fn(|i| i as u8) // [0, 1, …, 31], as in the NEP-413 example
    }

    #[test]
    fn payload_bytes_match_the_spec_example_layout() {
        let bytes = payload_bytes("hi", &spec_nonce(), "myapp.com", Some("myapp.com/callback"));

        let mut expected = vec![2, 0, 0, 0, b'h', b'i'];
        expected.extend(0u8..32);
        expected.extend([9, 0, 0, 0]);
        expected.extend(b"myapp.com");
        expected.push(1);
        expected.extend([18, 0, 0, 0]);
        expected.extend(b"myapp.com/callback");
        assert_eq!(bytes, expected);
    }

    #[test]
    fn missing_callback_url_is_a_single_zero_byte() {
        let bytes = payload_bytes("hi", &spec_nonce(), "myapp.com", None);
        assert_eq!(bytes.last(), Some(&0));
        assert_eq!(bytes.len(), 6 + 32 + 13 + 1);
    }

    #[test]
    fn prefix_tag_is_2_pow_31_plus_413_little_endian() {
        assert_eq!(PREFIX_TAG, (1u32 << 31) + 413);
        assert_eq!(PREFIX_TAG.to_le_bytes(), [0x9d, 0x01, 0x00, 0x80]);
    }

    #[test]
    fn a_wallet_signature_verifies() {
        let key = test_key();
        let nonce = [7u8; 32];
        let signature = wallet_sign(&key, "Sign in", &nonce, "fondouk");

        let public_key = parse_public_key(&public_key_string(&key)).unwrap();
        let signature = parse_signature(&signature).unwrap();
        let hash = payload_hash("Sign in", &nonce, "fondouk", None);

        assert!(verify(&public_key, &signature, &hash));
    }

    #[test]
    fn any_changed_field_breaks_the_signature() {
        let key = test_key();
        let nonce = [7u8; 32];
        let public_key = parse_public_key(&public_key_string(&key)).unwrap();
        let signature = parse_signature(&wallet_sign(&key, "Sign in", &nonce, "fondouk")).unwrap();

        let mut other_nonce = nonce;
        other_nonce[0] ^= 1;
        for hash in [
            payload_hash("Sign in!", &nonce, "fondouk", None),
            payload_hash("Sign in", &other_nonce, "fondouk", None),
            payload_hash("Sign in", &nonce, "evil.app", None),
            payload_hash("Sign in", &nonce, "fondouk", Some("x")),
        ] {
            assert!(!verify(&public_key, &signature, &hash));
        }
    }

    #[test]
    fn another_key_does_not_verify() {
        let nonce = [7u8; 32];
        let signature =
            parse_signature(&wallet_sign(&test_key(), "Sign in", &nonce, "fondouk")).unwrap();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let other_public = parse_public_key(&public_key_string(&other)).unwrap();

        let hash = payload_hash("Sign in", &nonce, "fondouk", None);
        assert!(!verify(&other_public, &signature, &hash));
    }

    #[test]
    fn a_tampered_signature_does_not_verify() {
        let key = test_key();
        let nonce = [7u8; 32];
        let mut bytes = STANDARD
            .decode(wallet_sign(&key, "Sign in", &nonce, "fondouk"))
            .unwrap();
        bytes[10] ^= 1;
        let public_key = parse_public_key(&public_key_string(&key)).unwrap();

        let verified = parse_signature(&STANDARD.encode(&bytes)).is_ok_and(|signature| {
            verify(
                &public_key,
                &signature,
                &payload_hash("Sign in", &nonce, "fondouk", None),
            )
        });
        assert!(!verified);
    }

    #[test]
    fn malformed_keys_and_signatures_are_errors_not_panics() {
        for bad in [
            "",
            "ed25519:",
            "secp256k1:abc",
            "ed25519:0OIl",   // not base58
            "ed25519:3yZe7d", // valid base58, wrong length
        ] {
            assert_eq!(
                parse_public_key(bad).unwrap_err(),
                Nep413Error::BadPublicKey
            );
        }
        for bad in ["", "not base64!", "AAAA"] {
            assert_eq!(parse_signature(bad).unwrap_err(), Nep413Error::BadSignature);
        }
    }
}
