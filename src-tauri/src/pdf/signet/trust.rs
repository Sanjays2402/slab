//! User-managed X.509 trust store for Signet.
//!
//! v3.10.0 lands a deliberately-simple trust store: a directory of PEM files,
//! each holding one or more `CERTIFICATE` blocks. Anything in the directory is
//! considered a trusted root. The store starts **empty** — no embedded CAs,
//! no Mozilla bundle — because Signet's audience is enterprises with their
//! own internal PKI, not general web TLS.
//!
//! What we check in v3.10.0:
//! - Issuer/Subject DN matching for chain assembly.
//! - notBefore / notAfter against the supplied `now`.
//! - Chain terminates at a root in the store.
//!
//! What we explicitly DO NOT check (tracked for v3.10.1):
//! - Cryptographic signature validity at each chain link (relies on the
//!   underlying `cms` parser to enforce signer-cert correctness during the
//!   verify pipeline; chain-internal signature checks land alongside CAdES).
//! - CRL / OCSP revocation.
//! - Name constraints, EKU enforcement, policy mapping.
//!
//! These limitations are surfaced in the Verify-tab UI by the
//! `ChainStatus::SelfSigned` / `ChainStatus::Untrusted` variants and the
//! "revocation: not checked" hint.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use der::Decode;
use x509_cert::Certificate;

use super::identity::SignetError;

/// Outcome of validating a signer certificate against a trust store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChainStatus {
    /// Chain successfully terminates at a cert in the store, all dates valid.
    Trusted,
    /// Chain assembled but no root in the store matched.
    Untrusted,
    /// Some cert in the assembled chain is past its notAfter.
    Expired,
    /// Some cert in the assembled chain is before its notBefore.
    NotYetValid,
    /// Could not extend the chain to any root (issuer not found among
    /// supplied intermediates or the trust store).
    ChainBroken,
    /// Leaf cert is self-signed (subject == issuer) and not present in the
    /// trust store. The verify UI presents this as a neutral state — common
    /// during testing, but a paying customer should add the root explicitly.
    SelfSigned,
}

impl ChainStatus {
    /// User-facing one-word label, suitable for a status pill in the UI.
    pub fn label(self) -> &'static str {
        match self {
            ChainStatus::Trusted => "Trusted",
            ChainStatus::Untrusted => "Untrusted",
            ChainStatus::Expired => "Expired",
            ChainStatus::NotYetValid => "Not yet valid",
            ChainStatus::ChainBroken => "Chain broken",
            ChainStatus::SelfSigned => "Self-signed",
        }
    }
}

/// A bag of trusted root certificates, populated from a directory on disk.
#[derive(Default, Debug)]
pub struct TrustStore {
    roots: Vec<Certificate>,
    /// Where roots are read from / written to. `None` if the store was
    /// constructed in-memory only (tests).
    source: Option<PathBuf>,
}

impl TrustStore {
    /// Empty in-memory store.
    pub fn new() -> Self {
        TrustStore::default()
    }

    /// Default on-disk trust directory:
    /// - macOS: `~/Library/Application Support/slab/signet/trusted`
    /// - Linux: `$XDG_DATA_HOME/slab/signet/trusted` (fallback `~/.local/share/...`)
    /// - Windows: `%APPDATA%\slab\signet\trusted`
    ///
    /// Creates the directory if it doesn't exist (so the user can drop a PEM
    /// in once and have it pick up next time).
    pub fn default_path() -> Option<PathBuf> {
        let proj = directories::ProjectDirs::from("dev", "slab", "slab")?;
        let dir = proj.data_dir().join("signet").join("trusted");
        let _ = fs::create_dir_all(&dir);
        Some(dir)
    }

    /// Load a trust store from the default user-data directory.
    /// Missing directory → empty store (not an error).
    pub fn load_default() -> Result<Self, SignetError> {
        match Self::default_path() {
            Some(p) => Self::load_dir(&p),
            None => Ok(Self::new()),
        }
    }

    /// Load every `*.pem` / `*.crt` in `dir` (non-recursive).
    pub fn load_dir(dir: &Path) -> Result<Self, SignetError> {
        let mut store = TrustStore::new();
        store.source = Some(dir.to_path_buf());
        if !dir.exists() {
            return Ok(store);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "pem" | "crt" | "cer") {
                continue;
            }
            let bytes = fs::read(&path)?;
            store.add_pem(&bytes)?;
        }
        Ok(store)
    }

    /// Parse one or more `CERTIFICATE` PEM blocks and add them as roots.
    pub fn add_pem(&mut self, pem_bytes: &[u8]) -> Result<usize, SignetError> {
        let blocks =
            pem::parse_many(pem_bytes).map_err(|e| SignetError::InvalidPem(e.to_string()))?;
        let mut added = 0;
        for b in blocks {
            if b.tag() != "CERTIFICATE" {
                continue;
            }
            let cert = Certificate::from_der(b.contents())
                .map_err(|e| SignetError::InvalidCert(format!("DER: {e}")))?;
            self.roots.push(cert);
            added += 1;
        }
        Ok(added)
    }

    /// Number of trusted roots in the store.
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// `true` when the store has no roots.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Validate `leaf_der` (with optional `chain_der` intermediates) against
    /// the store. Date checks against `now`. See type-level docs for what
    /// this does and doesn't cover.
    pub fn verify_chain(
        &self,
        leaf_der: &[u8],
        chain_der: &[Vec<u8>],
        now: SystemTime,
    ) -> ChainStatus {
        let Ok(leaf) = Certificate::from_der(leaf_der) else {
            return ChainStatus::ChainBroken;
        };

        let intermediates: Vec<Certificate> = chain_der
            .iter()
            .filter_map(|b| Certificate::from_der(b).ok())
            .collect();

        // Self-signed leaf with no trust-store match → SelfSigned.
        let leaf_self_signed = subject_eq_issuer(&leaf);

        // Walk leaf → … → root, capping depth at 8 to avoid loops.
        let mut current = &leaf;
        let mut visited_in_chain = vec![&leaf];
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        for _ in 0..8 {
            // Date check on current cert.
            match cert_validity_status(current, now_secs) {
                ChainStatus::Trusted => {}
                other => return other,
            }

            // Is current trusted directly?
            if self.roots.iter().any(|r| same_subject(r, current)) {
                return ChainStatus::Trusted;
            }

            // Is current self-issued? Walk no further.
            if subject_eq_issuer(current) {
                if leaf_self_signed && std::ptr::eq(current, &leaf) {
                    return ChainStatus::SelfSigned;
                }
                return ChainStatus::Untrusted;
            }

            // Find issuer in intermediates first, then trust store.
            let next = intermediates
                .iter()
                .find(|c| dn_eq(&c.tbs_certificate.subject, &current.tbs_certificate.issuer))
                .or_else(|| {
                    self.roots.iter().find(|c| {
                        dn_eq(&c.tbs_certificate.subject, &current.tbs_certificate.issuer)
                    })
                });
            let Some(parent) = next else {
                return ChainStatus::ChainBroken;
            };
            if visited_in_chain.iter().any(|c| std::ptr::eq(*c, parent)) {
                return ChainStatus::ChainBroken;
            }
            visited_in_chain.push(parent);
            // If parent is in trust store, we're done.
            if self.roots.iter().any(|r| same_subject(r, parent)) {
                return match cert_validity_status(parent, now_secs) {
                    ChainStatus::Trusted => ChainStatus::Trusted,
                    other => other,
                };
            }
            current = parent;
        }

        ChainStatus::ChainBroken
    }
}

fn subject_eq_issuer(cert: &Certificate) -> bool {
    dn_eq(&cert.tbs_certificate.subject, &cert.tbs_certificate.issuer)
}

fn same_subject(a: &Certificate, b: &Certificate) -> bool {
    dn_eq(&a.tbs_certificate.subject, &b.tbs_certificate.subject)
}

/// DER-encode both DNs and byte-compare. Stricter than text comparison but
/// reliable for chain assembly where DNs come from sibling certs that were
/// minted by the same CA tooling.
fn dn_eq(a: &x509_cert::name::Name, b: &x509_cert::name::Name) -> bool {
    let Ok(ae) = der::Encode::to_der(a) else {
        return false;
    };
    let Ok(be) = der::Encode::to_der(b) else {
        return false;
    };
    ae == be
}

fn cert_validity_status(cert: &Certificate, now_secs: i64) -> ChainStatus {
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
    if now_secs < nb {
        ChainStatus::NotYetValid
    } else if now_secs > na {
        ChainStatus::Expired
    } else {
        ChainStatus::Trusted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::signet::identity::SigningIdentity;
    use rsa::pkcs1v15::SigningKey as RsaSigningKey;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use rsa::sha2::Sha256;
    use std::str::FromStr;
    use std::time::Duration;
    use x509_cert::builder::{Builder as _, CertificateBuilder, Profile};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;

    /// Produce a self-signed root cert + the key, returning (cert PEM bytes, key PEM bytes).
    fn self_signed_root(cn: &str, valid_for_secs: u64) -> (Vec<u8>, Vec<u8>) {
        let mut rng = rand::thread_rng();
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let key_pem = key
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .as_bytes()
            .to_vec();
        let serial = SerialNumber::from(1u32);
        let validity = Validity::from_now(Duration::from_secs(valid_for_secs)).unwrap();
        let subject = Name::from_str(&format!("CN={cn}")).unwrap();
        let pub_key = key.to_public_key();
        let signing_key = RsaSigningKey::<Sha256>::new(key.clone());
        let spki_der = rsa::pkcs8::EncodePublicKey::to_public_key_der(&pub_key).unwrap();
        let spki = spki::SubjectPublicKeyInfoOwned::try_from(spki_der.as_bytes()).unwrap();
        let builder =
            CertificateBuilder::new(Profile::Root, serial, validity, subject, spki, &signing_key)
                .unwrap();
        let cert = builder.build::<rsa::pkcs1v15::Signature>().unwrap();
        let cert_der = der::Encode::to_der(&cert).unwrap();
        let cert_pem = pem::encode(&pem::Pem::new("CERTIFICATE", cert_der));
        (cert_pem.into_bytes(), key_pem)
    }

    #[test]
    fn empty_store_returns_self_signed_for_self_signed_leaf() {
        let (cert_pem, key_pem) = self_signed_root("Self", 365 * 24 * 3600);
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        let store = TrustStore::new();
        let status = store.verify_chain(&id.cert_der, &[], SystemTime::now());
        assert_eq!(status, ChainStatus::SelfSigned);
    }

    #[test]
    fn store_with_matching_root_returns_trusted() {
        let (cert_pem, key_pem) = self_signed_root("Root A", 365 * 24 * 3600);
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        let mut store = TrustStore::new();
        store.add_pem(&cert_pem).unwrap();
        let status = store.verify_chain(&id.cert_der, &[], SystemTime::now());
        assert_eq!(status, ChainStatus::Trusted);
    }

    #[test]
    fn store_with_unrelated_root_returns_self_signed_for_self_signed_leaf() {
        let (cert_pem, key_pem) = self_signed_root("Leaf", 365 * 24 * 3600);
        let (other_root_pem, _) = self_signed_root("OtherRoot", 365 * 24 * 3600);
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        let mut store = TrustStore::new();
        store.add_pem(&other_root_pem).unwrap();
        let status = store.verify_chain(&id.cert_der, &[], SystemTime::now());
        assert_eq!(status, ChainStatus::SelfSigned);
    }

    #[test]
    fn expired_cert_reports_expired() {
        // valid for 1 second, then sleep past it.
        let (cert_pem, key_pem) = self_signed_root("Expiry", 1);
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        let future = SystemTime::now() + Duration::from_secs(10);
        let mut store = TrustStore::new();
        store.add_pem(&cert_pem).unwrap();
        let status = store.verify_chain(&id.cert_der, &[], future);
        assert_eq!(status, ChainStatus::Expired);
    }

    #[test]
    fn not_yet_valid_cert_reports_not_yet_valid() {
        let (cert_pem, key_pem) = self_signed_root("Future", 365 * 24 * 3600);
        let id = SigningIdentity::from_pem_bytes(&cert_pem, &key_pem, None).unwrap();
        let mut store = TrustStore::new();
        store.add_pem(&cert_pem).unwrap();
        // Far past — before any cert we just minted.
        let way_back = SystemTime::UNIX_EPOCH + Duration::from_secs(60);
        let status = store.verify_chain(&id.cert_der, &[], way_back);
        assert_eq!(status, ChainStatus::NotYetValid);
    }

    #[test]
    fn add_pem_ignores_non_certificate_blocks() {
        let mut store = TrustStore::new();
        let mixed = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA\n-----END PUBLIC KEY-----\n";
        let n = store.add_pem(mixed).unwrap();
        assert_eq!(n, 0);
        assert!(store.is_empty());
    }

    #[test]
    fn chain_status_label_is_human_readable() {
        assert_eq!(ChainStatus::Trusted.label(), "Trusted");
        assert_eq!(ChainStatus::SelfSigned.label(), "Self-signed");
        assert_eq!(ChainStatus::ChainBroken.label(), "Chain broken");
    }

    #[test]
    fn load_dir_skips_unknown_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        let (cert_pem, _) = self_signed_root("PickMe", 365 * 24 * 3600);
        std::fs::write(tmp.path().join("root.pem"), &cert_pem).unwrap();
        std::fs::write(tmp.path().join("README.txt"), b"hi").unwrap();
        let store = TrustStore::load_dir(tmp.path()).unwrap();
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn load_default_creates_directory_when_missing() {
        // Just smoke-test that this doesn't panic. Real path varies by OS;
        // we don't write anything.
        let _ = TrustStore::load_default();
    }
}
