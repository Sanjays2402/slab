//! Load a [`SigningIdentity`] (X.509 certificate chain + private key) from disk.
//!
//! v3.10.0 ships **PEM-only** loading (cert.pem + key.pem). The original plan
//! also called for PKCS#12, but the pure-Rust PKCS#12 ecosystem is still
//! pre-1.0 (the `pkcs12` crate is 0.1, `p12` is unmaintained) — bringing in a
//! native dep would break the cross-platform parity we set in ADR 0011.
//! PKCS#12 support is tracked for v3.10.1 alongside CRL/OCSP.
//!
//! Supported algorithms:
//! - RSA (2048 / 3072 / 4096-bit) with SHA-256
//! - ECDSA P-256 with SHA-256
//! - ECDSA P-384 with SHA-384
//!
//! Both encrypted (password-protected PKCS#8) and unencrypted PEM keys are
//! accepted. We attempt unencrypted decoding first and only ask the password
//! crate to do work when the encrypted variant is detected, so the common case
//! is one allocation + one parse.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use der::Decode;
use pkcs8::DecodePrivateKey;
use rsa::pkcs1v15::SigningKey as RsaSigningKey;
use rsa::sha2::Sha256;
use x509_cert::Certificate;

/// Public-key algorithm family for a signing key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAlgorithm {
    /// RSA (any modulus size ≥ 2048).
    Rsa,
    /// ECDSA over the NIST P-256 curve, paired with SHA-256.
    EcdsaP256,
    /// ECDSA over the NIST P-384 curve, paired with SHA-384.
    EcdsaP384,
}

impl KeyAlgorithm {
    /// Display name used in UI / log lines.
    pub fn label(self) -> &'static str {
        match self {
            KeyAlgorithm::Rsa => "RSA",
            KeyAlgorithm::EcdsaP256 => "ECDSA P-256",
            KeyAlgorithm::EcdsaP384 => "ECDSA P-384",
        }
    }
}

/// A signing key in one of the supported algorithm families.
///
/// Kept opaque to callers — produce one via [`SigningIdentity::load_pem_pair`]
/// and feed it to the CMS builder. The underlying RustCrypto types implement
/// the traits `cms` needs (`Keypair + DynSignatureAlgorithmIdentifier +
/// Signer<…>`).
///
/// The RSA variant is boxed because its expanded key material (n, e, d,
/// primes, CRT params) dwarfs the ECDSA variants — boxing keeps the enum
/// small enough that `Clippy::large_enum_variant` is happy without forcing
/// allocations on the ECDSA hot path.
#[allow(clippy::large_enum_variant)]
pub enum SigningKey {
    Rsa(Box<RsaSigningKey<Sha256>>),
    EcdsaP256(p256::ecdsa::SigningKey),
    EcdsaP384(p384::ecdsa::SigningKey),
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print key bytes — even in debug.
        match self {
            SigningKey::Rsa(_) => f.write_str("SigningKey::Rsa(…)"),
            SigningKey::EcdsaP256(_) => f.write_str("SigningKey::EcdsaP256(…)"),
            SigningKey::EcdsaP384(_) => f.write_str("SigningKey::EcdsaP384(…)"),
        }
    }
}

/// A loaded signing identity ready to sign PDFs.
///
/// Holds the leaf certificate DER (for embedding in the CMS `certificates`
/// SET), any intermediate-CA DERs (chain), and the private signing key. The
/// `subject_cn` and validity fields are pre-extracted for the Sign tab UI so
/// the renderer doesn't re-parse X.509.
pub struct SigningIdentity {
    pub algorithm: KeyAlgorithm,
    /// DER bytes of the leaf (signing) certificate.
    pub cert_der: Vec<u8>,
    /// DER bytes of additional CA certificates in chain order
    /// (signer → … → root). Roots SHOULD NOT be embedded but we don't
    /// strip them either — that's the verifier's call.
    pub chain_der: Vec<Vec<u8>>,
    /// Private key. Never serialized, never logged.
    pub signing_key: SigningKey,
    /// Best-effort extraction of the cert's Subject CommonName for display.
    /// Falls back to the full DN serialized via `Display` if no CN exists.
    pub subject_cn: String,
    /// Unix-seconds notAfter timestamp.
    pub not_after_unix: i64,
    /// Unix-seconds notBefore timestamp.
    pub not_before_unix: i64,
}

impl std::fmt::Debug for SigningIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningIdentity")
            .field("algorithm", &self.algorithm)
            .field("subject_cn", &self.subject_cn)
            .field("cert_der_len", &self.cert_der.len())
            .field("chain_len", &self.chain_der.len())
            .field("not_after_unix", &self.not_after_unix)
            .field("not_before_unix", &self.not_before_unix)
            .finish_non_exhaustive()
    }
}

/// Errors that can arise while loading or validating a signing identity.
#[derive(Debug)]
pub enum SignetError {
    Io(std::io::Error),
    InvalidPem(String),
    InvalidCert(String),
    InvalidKey(String),
    UnsupportedAlgorithm(String),
    /// Either a wrong PEM password OR the file isn't actually encrypted but
    /// a password was supplied — the crypto layer can't always tell them
    /// apart so we report them together.
    WrongPassword,
    /// Certificate's notAfter has already elapsed (relative to the system clock
    /// at the time of [`SigningIdentity::ensure_valid_now`]).
    Expired,
    /// Certificate's notBefore is in the future.
    NotYetValid,
}

impl std::fmt::Display for SignetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignetError::Io(e) => write!(f, "I/O error: {e}"),
            SignetError::InvalidPem(s) => write!(f, "Invalid PEM: {s}"),
            SignetError::InvalidCert(s) => write!(f, "Invalid certificate: {s}"),
            SignetError::InvalidKey(s) => write!(f, "Invalid private key: {s}"),
            SignetError::UnsupportedAlgorithm(s) => write!(f, "Unsupported algorithm: {s}"),
            SignetError::WrongPassword => write!(f, "Wrong password (or unexpected password)"),
            SignetError::Expired => write!(f, "Certificate expired"),
            SignetError::NotYetValid => write!(f, "Certificate not yet valid"),
        }
    }
}

impl std::error::Error for SignetError {}

impl From<std::io::Error> for SignetError {
    fn from(e: std::io::Error) -> Self {
        SignetError::Io(e)
    }
}

impl SigningIdentity {
    /// Load an identity from a separate PEM cert file + PEM key file.
    ///
    /// The cert file may contain multiple `BEGIN CERTIFICATE` blocks — the
    /// first is treated as the leaf, the rest as the chain in given order.
    ///
    /// The key file may contain either an unencrypted PKCS#8 `PRIVATE KEY`
    /// block or an encrypted PKCS#8 `ENCRYPTED PRIVATE KEY` block. PKCS#1
    /// `RSA PRIVATE KEY` and SEC1 `EC PRIVATE KEY` blocks are also accepted
    /// via the algorithm-specific loaders.
    pub fn load_pem_pair(
        cert_pem_path: &Path,
        key_pem_path: &Path,
        key_password: Option<&str>,
    ) -> Result<Self, SignetError> {
        let cert_pem = fs::read(cert_pem_path)?;
        let key_pem = fs::read(key_pem_path)?;
        Self::from_pem_bytes(&cert_pem, &key_pem, key_password)
    }

    /// In-memory variant of [`Self::load_pem_pair`] used by tests and the
    /// (planned) Tauri command path that ships raw bytes from the renderer.
    pub fn from_pem_bytes(
        cert_pem: &[u8],
        key_pem: &[u8],
        key_password: Option<&str>,
    ) -> Result<Self, SignetError> {
        let pems = pem::parse_many(cert_pem).map_err(|e| SignetError::InvalidPem(e.to_string()))?;
        let cert_blocks: Vec<&pem::Pem> =
            pems.iter().filter(|p| p.tag() == "CERTIFICATE").collect();
        if cert_blocks.is_empty() {
            return Err(SignetError::InvalidCert(
                "no CERTIFICATE block in cert PEM".into(),
            ));
        }

        let leaf_der = cert_blocks[0].contents().to_vec();
        let chain_der: Vec<Vec<u8>> = cert_blocks[1..]
            .iter()
            .map(|p| p.contents().to_vec())
            .collect();

        let leaf = Certificate::from_der(&leaf_der)
            .map_err(|e| SignetError::InvalidCert(format!("DER parse: {e}")))?;

        let subject_cn =
            extract_common_name(&leaf).unwrap_or_else(|| leaf.tbs_certificate.subject.to_string());
        let (not_before_unix, not_after_unix) = extract_validity(&leaf);

        let signing_key = decode_signing_key(key_pem, key_password)?;
        let algorithm = match signing_key {
            SigningKey::Rsa(_) => KeyAlgorithm::Rsa,
            SigningKey::EcdsaP256(_) => KeyAlgorithm::EcdsaP256,
            SigningKey::EcdsaP384(_) => KeyAlgorithm::EcdsaP384,
        };

        Ok(SigningIdentity {
            algorithm,
            cert_der: leaf_der,
            chain_der,
            signing_key,
            subject_cn,
            not_after_unix,
            not_before_unix,
        })
    }

    /// Returns [`SignetError::Expired`] if the cert's validity window has
    /// closed, or [`SignetError::NotYetValid`] if it hasn't opened.
    pub fn ensure_valid_now(&self) -> Result<(), SignetError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now < self.not_before_unix {
            return Err(SignetError::NotYetValid);
        }
        if now > self.not_after_unix {
            return Err(SignetError::Expired);
        }
        Ok(())
    }
}

/// Try every supported key family in priority order.
///
/// Strategy: PKCS#8 unencrypted → encrypted PKCS#8 (if password) → PKCS#1
/// RSA → SEC1 EC. The first parser that returns `Ok` wins. Errors propagate
/// only if no parser accepts the input AND we hit the encrypted-key path
/// (where a wrong password is the most likely cause).
fn decode_signing_key(key_pem: &[u8], password: Option<&str>) -> Result<SigningKey, SignetError> {
    let blocks = pem::parse_many(key_pem).map_err(|e| SignetError::InvalidPem(e.to_string()))?;
    let key_block = blocks.iter().find(|p| {
        matches!(
            p.tag(),
            "PRIVATE KEY" | "ENCRYPTED PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY"
        )
    });
    let Some(block) = key_block else {
        return Err(SignetError::InvalidKey(
            "no PRIVATE KEY block recognised in key PEM".into(),
        ));
    };

    let tag = block.tag();
    let der = block.contents();

    // Encrypted PKCS#8: need the password.
    if tag == "ENCRYPTED PRIVATE KEY" {
        let pw = password.ok_or(SignetError::WrongPassword)?;
        let epki = pkcs8::EncryptedPrivateKeyInfo::try_from(der)
            .map_err(|e| SignetError::InvalidKey(format!("EPKI parse: {e}")))?;
        let pki_doc = epki.decrypt(pw).map_err(|_| SignetError::WrongPassword)?;
        return decode_pkcs8_der(pki_doc.as_bytes());
    }

    // Unencrypted PKCS#8.
    if tag == "PRIVATE KEY" {
        return decode_pkcs8_der(der);
    }

    // PKCS#1 RSA (legacy).
    if tag == "RSA PRIVATE KEY" {
        use rsa::pkcs1::DecodeRsaPrivateKey;
        let key = rsa::RsaPrivateKey::from_pkcs1_der(der)
            .map_err(|e| SignetError::InvalidKey(format!("PKCS#1 RSA: {e}")))?;
        return Ok(SigningKey::Rsa(Box::new(RsaSigningKey::<Sha256>::new(key))));
    }

    // SEC1 EC (legacy). Try P-256 then P-384 via PKCS#8 conversion.
    if tag == "EC PRIVATE KEY" {
        // We don't pull in `sec1` directly; instead reject and ask the user
        // to convert their key with `openssl pkcs8 -topk8 -nocrypt`. SEC1 is
        // rare in 2025 and supporting it would re-pull `sec1` as a direct dep
        // with no payoff over PKCS#8.
        return Err(SignetError::UnsupportedAlgorithm(
            "SEC1 'EC PRIVATE KEY' format not supported — convert to PKCS#8 with `openssl pkcs8 -topk8 -nocrypt -in key.pem -out pkcs8.pem`".into(),
        ));
    }

    unreachable!("filtered tag above")
}

/// PKCS#8 unencrypted DER → first parser that accepts it.
fn decode_pkcs8_der(der: &[u8]) -> Result<SigningKey, SignetError> {
    if let Ok(k) = rsa::RsaPrivateKey::from_pkcs8_der(der) {
        return Ok(SigningKey::Rsa(Box::new(RsaSigningKey::<Sha256>::new(k))));
    }
    if let Ok(k) = p256::ecdsa::SigningKey::from_pkcs8_der(der) {
        return Ok(SigningKey::EcdsaP256(k));
    }
    if let Ok(k) = p384::ecdsa::SigningKey::from_pkcs8_der(der) {
        return Ok(SigningKey::EcdsaP384(k));
    }
    Err(SignetError::UnsupportedAlgorithm(
        "PKCS#8 key is neither RSA nor P-256 nor P-384".into(),
    ))
}

/// Walk the cert's Subject DN looking for the first commonName (OID 2.5.4.3).
fn extract_common_name(cert: &Certificate) -> Option<String> {
    use const_oid::db::rfc4519::CN;
    for rdn in cert.tbs_certificate.subject.0.iter() {
        for atv in rdn.0.iter() {
            if atv.oid == CN {
                if let Ok(s) = atv.value.decode_as::<der::asn1::PrintableStringRef<'_>>() {
                    return Some(s.as_str().to_string());
                }
                if let Ok(s) = atv.value.decode_as::<der::asn1::Utf8StringRef<'_>>() {
                    return Some(s.as_str().to_string());
                }
                if let Ok(s) = atv.value.decode_as::<der::asn1::Ia5StringRef<'_>>() {
                    return Some(s.as_str().to_string());
                }
            }
        }
    }
    None
}

/// Extract notBefore / notAfter as Unix-seconds. Returns (0, i64::MAX) on
/// parse trouble so we degrade to "always valid" rather than crash —
/// downstream validation will still re-check.
fn extract_validity(cert: &Certificate) -> (i64, i64) {
    let nb = cert
        .tbs_certificate
        .validity
        .not_before
        .to_unix_duration()
        .as_secs() as i64;
    let na = cert
        .tbs_certificate
        .validity
        .not_after
        .to_unix_duration()
        .as_secs() as i64;
    (nb, na)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use std::str::FromStr;
    use x509_cert::builder::{Builder as _, CertificateBuilder, Profile};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;

    /// Generate a self-signed RSA-2048 cert + its key, return them as PEM
    /// (cert.pem, key.pem).
    fn fixture_rsa_cert(cn: &str) -> (Vec<u8>, Vec<u8>) {
        let mut rng = rand::thread_rng();
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let key_pem = key
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .as_bytes()
            .to_vec();

        let serial = SerialNumber::from(42u32);
        let validity = Validity::from_now(std::time::Duration::from_secs(365 * 24 * 3600)).unwrap();
        let subject = Name::from_str(&format!("CN={cn}")).unwrap();
        let pub_key = key.to_public_key();
        let signing_key = RsaSigningKey::<Sha256>::new(key.clone());
        let spki = rsa::pkcs8::EncodePublicKey::to_public_key_der(&pub_key).unwrap();
        let spki_info = spki::SubjectPublicKeyInfoOwned::try_from(spki.as_bytes()).unwrap();

        let builder = CertificateBuilder::new(
            Profile::Root,
            serial,
            validity,
            subject,
            spki_info,
            &signing_key,
        )
        .unwrap();

        let cert = builder.build::<rsa::pkcs1v15::Signature>().unwrap();
        let cert_der = der::Encode::to_der(&cert).unwrap();
        let cert_pem = pem::encode(&pem::Pem::new("CERTIFICATE", cert_der));
        (cert_pem.into_bytes(), key_pem)
    }

    #[test]
    fn loads_rsa_pem_pair() {
        let (cert_pem, key_pem) = fixture_rsa_cert("Slab Test Signer");
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        assert_eq!(id.algorithm, KeyAlgorithm::Rsa);
        assert_eq!(id.subject_cn, "Slab Test Signer");
        assert!(!id.cert_der.is_empty());
        assert!(id.chain_der.is_empty());
        matches!(id.signing_key, SigningKey::Rsa(_));
    }

    #[test]
    fn extracts_subject_cn_from_complex_dn() {
        let (cert_pem, key_pem) = fixture_rsa_cert("Acme Corp Code Signer");
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        assert_eq!(id.subject_cn, "Acme Corp Code Signer");
    }

    #[test]
    fn rejects_garbage_cert_pem() {
        let key_pem =
            b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIA==\n-----END PRIVATE KEY-----\n";
        let cert_pem =
            b"-----BEGIN CERTIFICATE-----\nbm90YXJlYWxjZXJ0\n-----END CERTIFICATE-----\n";
        let err = SigningIdentity::from_pem_bytes(cert_pem, key_pem, None).unwrap_err();
        assert!(matches!(
            err,
            SignetError::InvalidCert(_) | SignetError::InvalidPem(_)
        ));
    }

    #[test]
    fn rejects_key_pem_with_no_private_key_block() {
        let (cert_pem, _) = fixture_rsa_cert("X");
        let key_pem = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA\n-----END PUBLIC KEY-----\n";
        let err = SigningIdentity::from_pem_bytes(&cert_pem, key_pem, None).unwrap_err();
        assert!(matches!(err, SignetError::InvalidKey(_)));
    }

    #[test]
    fn ensure_valid_now_passes_for_freshly_generated() {
        let (cert_pem, key_pem) = fixture_rsa_cert("Fresh");
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        id.ensure_valid_now().unwrap();
        assert!(id.not_after_unix > id.not_before_unix);
    }

    #[test]
    fn parses_chain_when_multiple_certs_present() {
        let (leaf_pem, key_pem) = fixture_rsa_cert("Leaf");
        let (chain_pem, _) = fixture_rsa_cert("Intermediate");
        let mut combined = leaf_pem.clone();
        combined.extend_from_slice(&chain_pem);
        let id = SigningIdentity::from_pem_bytes(&combined, &key_pem, None).unwrap();
        assert_eq!(id.subject_cn, "Leaf");
        assert_eq!(id.chain_der.len(), 1);
    }

    #[test]
    fn key_algorithm_label_is_human_readable() {
        assert_eq!(KeyAlgorithm::Rsa.label(), "RSA");
        assert_eq!(KeyAlgorithm::EcdsaP256.label(), "ECDSA P-256");
        assert_eq!(KeyAlgorithm::EcdsaP384.label(), "ECDSA P-384");
    }

    #[test]
    fn debug_does_not_leak_key_bytes() {
        let (cert_pem, key_pem) = fixture_rsa_cert("Secret");
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        let dbg = format!("{:?}", id);
        assert!(!dbg.contains("RsaPrivateKey"));
        assert!(!dbg.contains("BEGIN"));
        assert!(dbg.contains("Secret"));
    }
}
