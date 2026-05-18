//! Ed25519 signature verification for marketplace index entries.
//!
//! Each [`crate::marketplace::index::IndexEntry`] carries a
//! base64-encoded Ed25519 signature over the canonical JSON of the
//! entry minus its `signature` field. This module verifies that
//! signature against a baked-in public key (the Slab maintainer's),
//! or against an arbitrary key (for tests).
//!
//! ## Security model
//!
//! The trust anchor is a single Ed25519 public key compiled into the
//! Slab binary. Anyone with the matching private key can sign new
//! marketplace entries. Future key rotation will add additional
//! constants and select among them via [`crate::marketplace::index::Index::signing_key_id`].
//!
//! ## What gets signed
//!
//! `serde_json::to_vec(&entry.to_unsigned())` — the same byte stream
//! the maintainer signing tool emits. Field order is fixed by struct
//! declaration in [`crate::marketplace::index::IndexEntryUnsigned`]
//! and must never be changed without rotating the schema version.

use crate::marketplace::index::IndexEntry;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

/// Slab's maintainer Ed25519 public key (32 bytes).
///
/// **This is a placeholder of all-zero bytes for the v1.4 development
/// branch.** Slice 2 will generate the real maintainer key pair, at
/// which point this constant gets replaced with the real public key
/// and the index gets re-signed.
///
/// A real signature against this placeholder key is impossible (the
/// all-zero point is the identity element on Ed25519, which
/// `ed25519-dalek` correctly refuses to verify). That's deliberate —
/// it means a v1.4.0-bench-slice-1 build will reject every
/// marketplace entry until the real key lands. That's safer than
/// failing-open during development.
pub const MAINTAINER_PUBLIC_KEY: [u8; 32] = [0u8; 32];

/// Identifier embedded in `Index::signing_key_id`. Keeps room for
/// future key rotation without breaking older clients.
pub const MAINTAINER_KEY_ID: &str = "slab-maintainer-2026";

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("unknown signing_key_id: {0}")]
    UnknownKeyId(String),
    #[error("signature is not valid base64: {0}")]
    BadSignatureEncoding(String),
    #[error("signature has wrong length: expected 64 bytes, got {0}")]
    BadSignatureLength(usize),
    #[error("public key is not a valid Ed25519 point")]
    BadPublicKey,
    #[error("canonical JSON serialization failed: {0}")]
    Canonicalize(String),
    #[error("signature verification failed")]
    BadSignature,
}

/// Verify an entry's signature against the supplied public key.
///
/// Splitting the public key out as a parameter (rather than always
/// reading the const) makes the verifier unit-testable without having
/// to ship the real private key alongside the source.
pub fn verify_entry(entry: &IndexEntry, public_key: &[u8; 32]) -> Result<(), VerifyError> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};

    let sig_bytes = B64
        .decode(&entry.signature)
        .map_err(|e| VerifyError::BadSignatureEncoding(e.to_string()))?;
    if sig_bytes.len() != 64 {
        return Err(VerifyError::BadSignatureLength(sig_bytes.len()));
    }
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| VerifyError::BadSignatureLength(sig_bytes.len()))?;
    let signature = Signature::from_bytes(&sig_array);

    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| VerifyError::BadPublicKey)?;

    let unsigned = entry.to_unsigned();
    let canonical =
        serde_json::to_vec(&unsigned).map_err(|e| VerifyError::Canonicalize(e.to_string()))?;

    vk.verify(&canonical, &signature)
        .map_err(|_| VerifyError::BadSignature)
}

/// Convenience: verify against the baked-in maintainer key.
///
/// During v1.4 development this will always fail because the baked-in
/// key is a placeholder — see [`MAINTAINER_PUBLIC_KEY`] doc-comment.
pub fn verify_with_maintainer_key(entry: &IndexEntry) -> Result<(), VerifyError> {
    verify_entry(entry, &MAINTAINER_PUBLIC_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::index::IndexEntry;
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture_entry() -> IndexEntry {
        IndexEntry {
            id: "com.example.hello".into(),
            name: "Hello".into(),
            version: "0.1.0".into(),
            description: "Demo".into(),
            author: "Sanjay".into(),
            download_url: "https://example.com/hello.tar.gz".into(),
            sha256: "deadbeef".repeat(8),
            size_bytes: 1024,
            slab_compat: ">=1.4.0".into(),
            signature: String::new(),
        }
    }

    fn sign(entry: &IndexEntry, sk: &SigningKey) -> String {
        let canonical = serde_json::to_vec(&entry.to_unsigned()).unwrap();
        let sig = sk.sign(&canonical);
        B64.encode(sig.to_bytes())
    }

    #[test]
    fn verify_accepts_correct_signature() {
        // Deterministic test key.
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let mut e = fixture_entry();
        e.signature = sign(&e, &sk);

        assert!(verify_entry(&e, sk.verifying_key().as_bytes()).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_field() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let mut e = fixture_entry();
        e.signature = sign(&e, &sk);
        // Tamper post-signature: redirect the download_url to an attacker.
        e.download_url = "https://evil.example.com/hello.tar.gz".into();

        let err = verify_entry(&e, sk.verifying_key().as_bytes()).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignature));
    }

    #[test]
    fn verify_rejects_tampered_sha256() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let mut e = fixture_entry();
        e.signature = sign(&e, &sk);
        // Critical attack vector — tampered sha256 would let an attacker
        // substitute the tarball. Verify the verifier catches it.
        e.sha256 = "ff".repeat(32);

        let err = verify_entry(&e, sk.verifying_key().as_bytes()).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignature));
    }

    #[test]
    fn verify_rejects_wrong_public_key() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let mut e = fixture_entry();
        e.signature = sign(&e, &sk);

        let other = SigningKey::from_bytes(&[8u8; 32]);
        let err = verify_entry(&e, other.verifying_key().as_bytes()).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignature));
    }

    #[test]
    fn verify_rejects_bad_base64() {
        let mut e = fixture_entry();
        e.signature = "!!!not-base64!!!".into();
        let err = verify_entry(&e, &[0u8; 32]).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignatureEncoding(_)));
    }

    #[test]
    fn verify_rejects_short_signature() {
        let mut e = fixture_entry();
        e.signature = B64.encode(b"too-short");
        let err = verify_entry(&e, &[0u8; 32]).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignatureLength(_)));
    }

    #[test]
    fn verify_rejects_long_signature() {
        let mut e = fixture_entry();
        e.signature = B64.encode([0u8; 128]);
        let err = verify_entry(&e, &[0u8; 32]).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignatureLength(128)));
    }

    #[test]
    fn maintainer_placeholder_key_rejects_everything() {
        // The all-zero placeholder must never accept a signature.
        // This is a regression test for the "develop branch ships
        // failing-closed" property.
        let sk = SigningKey::from_bytes(&[1u8; 32]);
        let mut e = fixture_entry();
        e.signature = sign(&e, &sk);
        let err = verify_with_maintainer_key(&e).unwrap_err();
        // Either BadPublicKey or BadSignature both indicate fail-closed.
        assert!(matches!(
            err,
            VerifyError::BadPublicKey | VerifyError::BadSignature
        ));
    }
}
