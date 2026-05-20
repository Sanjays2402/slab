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
/// Generated 2026-05-17 (v1.4.0 Slice 2) — the matching private key
/// lives at `~/.slab-maintainer-key` on the maintainer's machine and
/// is **never** committed to the repo. See [`SIGNING.md`](../../../SIGNING.md)
/// for the rotation procedure if the key is ever compromised.
///
/// The maintainer signing tool (`cargo run --bin slab-sign-plugin`)
/// reads the private key from `~/.slab-maintainer-key` and emits
/// signed [`IndexEntry`] JSON ready to paste into the marketplace
/// `index.json`. Any signature produced by that tool will verify
/// against this constant.
///
/// Encoded as a hex literal for review-friendliness — easy to compare
/// against the `slab-sign-plugin --print-public-key` output.
pub const MAINTAINER_PUBLIC_KEY: [u8; 32] = [
    0x17, 0xf3, 0x8d, 0x92, 0xdb, 0x3a, 0xf9, 0x64, 0x2f, 0x0c, 0xf3, 0x5d, 0xd0, 0x3e, 0xdb, 0x8c,
    0x7e, 0x26, 0xe5, 0xe1, 0x18, 0xf2, 0x26, 0x45, 0xd1, 0x9b, 0xb5, 0x2f, 0x8c, 0xad, 0x7b, 0x27,
];

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

/// Sentinel signature value used by entries that ship inside the Slab
/// binary (the `assets/marketplace/seed-index.json` resource). These
/// entries are trusted by construction: their bytes are compiled into
/// the binary the user already chose to install, so a separate signature
/// check would only protect against a corrupted asset bundle — a class
/// of attack defeated by the OS code-signing chain higher up.
///
/// The verifier accepts `signature == "BUNDLED"` IFF `download_url`
/// also starts with `bundled://`. Both conditions must hold; the second
/// blocks a malicious remote `index.json` from claiming `"BUNDLED"` to
/// shortcut verification.
pub const BUNDLED_SIGNATURE_SENTINEL: &str = "BUNDLED";
pub const BUNDLED_DOWNLOAD_SCHEME: &str = "bundled://";

/// Verify an entry's signature against the supplied public key.
///
/// Splitting the public key out as a parameter (rather than always
/// reading the const) makes the verifier unit-testable without having
/// to ship the real private key alongside the source.
pub fn verify_entry(entry: &IndexEntry, public_key: &[u8; 32]) -> Result<(), VerifyError> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};

    // Trusted-by-construction bypass for entries that ship in-binary.
    // Both conditions must hold so a malicious remote index can't fake
    // the bypass by setting signature="BUNDLED" on its own entries.
    if entry.signature == BUNDLED_SIGNATURE_SENTINEL
        && entry.download_url.starts_with(BUNDLED_DOWNLOAD_SCHEME)
    {
        return Ok(());
    }

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
/// Returns `Ok(())` iff the entry was signed by the holder of the
/// private key matching [`MAINTAINER_PUBLIC_KEY`]. This is the
/// production entry point that the fetch + install pipeline calls.
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
            // v2 fields default to empty/zero so the canonical signing
            // payload remains byte-identical to a v1 entry. This is
            // intentional — the v2.0.2 backward-compat invariant.
            categories: Vec::new(),
            tags: Vec::new(),
            screenshots: Vec::new(),
            installs: 0,
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
    fn maintainer_key_verifies_known_signature() {
        // Regression fixture: the IndexEntry fields in `fixture_entry()`
        // signed with the Slab maintainer private key
        // (~/.slab-maintainer-key, generated 2026-05-17) produce this
        // exact base64 signature. If `MAINTAINER_PUBLIC_KEY` is ever
        // changed without rotating the matching private key, this
        // test breaks loudly — that's the point.
        //
        // To regenerate: run
        //   cargo run -q --bin slab-sign-plugin -- \
        //     --print-fixture-signature
        // (added in Slice 2) and paste the new b64 here.
        let mut e = fixture_entry();
        e.signature = "moFLWn76JK9odPF1iXgW6BnaVOyacRyDnaudSruwOPumfOEGNijnPAUNRx33stgEYLYLQ5MxSYCnzPfCMTFLBw==".into();
        verify_with_maintainer_key(&e)
            .expect("known-good signature must verify against baked-in maintainer key");
    }

    #[test]
    fn maintainer_key_rejects_tampered_known_signature() {
        // Same as above but with the description mutated — must fail.
        let mut e = fixture_entry();
        e.description = "Tampered description".into();
        e.signature = "moFLWn76JK9odPF1iXgW6BnaVOyacRyDnaudSruwOPumfOEGNijnPAUNRx33stgEYLYLQ5MxSYCnzPfCMTFLBw==".into();
        let err = verify_with_maintainer_key(&e).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignature));
    }

    // ---- v2.0.2 Workshop Marketplace: BUNDLED-sentinel verification ----

    #[test]
    fn verify_accepts_bundled_sentinel_when_download_url_is_bundled_scheme() {
        // Trusted-by-construction path: bundled:// + signature=="BUNDLED"
        // means the entry ships in the binary itself, so the verifier
        // skips the Ed25519 check.
        let mut e = fixture_entry();
        e.signature = BUNDLED_SIGNATURE_SENTINEL.into();
        e.download_url = "bundled://com.slab.examples.hello".into();
        // Any public key, even a zero one — the bundled path never reads it.
        assert!(verify_entry(&e, &[0u8; 32]).is_ok());
    }

    #[test]
    fn verify_rejects_bundled_sentinel_with_remote_download_url() {
        // Attack scenario: malicious remote index.json sets
        // signature="BUNDLED" to skip verification. Must fail because
        // download_url is not the bundled:// scheme.
        let mut e = fixture_entry();
        e.signature = BUNDLED_SIGNATURE_SENTINEL.into();
        e.download_url = "https://evil.example.com/payload.tar.gz".into();
        let err = verify_entry(&e, &[0u8; 32]).unwrap_err();
        // Falls through to normal verification. The exact error
        // variant depends on whether "BUNDLED" decodes as valid b64
        // (it does, but with bad padding) — what matters is that the
        // bypass did NOT fire and we got an error.
        assert!(
            matches!(
                err,
                VerifyError::BadSignatureEncoding(_)
                    | VerifyError::BadSignatureLength(_)
                    | VerifyError::BadSignature
            ),
            "remote URL with BUNDLED sentinel must NOT bypass verification, got {err:?}"
        );
    }

    #[test]
    fn verify_rejects_bundled_url_without_sentinel_signature() {
        // The reverse attack: bundled:// URL but a real-looking forged
        // signature. The bypass requires BOTH conditions to hold, so
        // this falls through to normal Ed25519 verification — which
        // fails because the signature was made over different bytes
        // (or with a different key).
        let mut e = fixture_entry();
        e.download_url = "bundled://com.slab.examples.hello".into();
        // A garbage-but-valid-length base64 signature (88 b64 chars = 64 bytes).
        e.signature = B64.encode([0u8; 64]);
        let err = verify_entry(&e, &[0u8; 32]).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::BadPublicKey | VerifyError::BadSignature
        ));
    }
}
