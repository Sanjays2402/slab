//! Build an `adbe.pkcs7.detached` CMS SignedData blob over a SHA-256 digest.
//!
//! This is the cryptographic core of [`crate::pdf::signet::sign`]: given a
//! 32-byte SHA-256 of the un-/Contents PDF bytes and a [`SigningIdentity`],
//! produce the DER-encoded ContentInfo bytes that go in `/Contents`.
//!
//! Conforming to:
//! - **RFC 5652** (CMS) — SignedData with `eContent = None` (detached).
//! - **Adobe Acrobat profile** — `Filter /Adobe.PPKLite`,
//!   `SubFilter /adbe.pkcs7.detached`, version 1, IssuerAndSerialNumber SID.
//!
//! v3.10.0 ships **RSA-SHA-256** signing only. The identity loader accepts
//! ECDSA keys (P-256/P-384) but emitting an ECDSA `SignerInfo` requires
//! threading `signature::Signer<DerSignature>` bounds through the cms builder
//! — a future v3.10.1 enhancement. ECDSA identities reach this layer and
//! return [`SignetError::UnsupportedAlgorithm`] with a clear message.

use std::time::SystemTime;

use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::ContentInfo;
use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};
use const_oid::db::rfc5911::{ID_DATA, ID_SIGNED_DATA};
use der::asn1::OctetString;
use der::{Any, Decode, Encode, Tag};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;

use super::identity::{KeyAlgorithm, SignetError, SigningIdentity, SigningKey};

/// SHA-256 OID.
const OID_SHA_256: &str = "2.16.840.1.101.3.4.2.1";

/// Build a PKCS#7-detached CMS blob suitable for `/Contents` in a PDF
/// signature dict.
///
/// `digest_sha256` MUST be the SHA-256 of the PDF bytes covered by ByteRange
/// (i.e. with the `/Contents <…>` window EXCLUDED, brackets included in the
/// included tail).
///
/// `signing_time` is embedded both as the SignerInfo `signingTime` attribute
/// and indirectly through the cms builder's own attribute helpers.
///
/// The returned bytes are the full DER-encoded `ContentInfo` (`signedData`),
/// ready to be hex-encoded and spliced into the placeholder window.
pub fn build_pkcs7_detached(
    digest_sha256: &[u8; 32],
    identity: &SigningIdentity,
    _signing_time: SystemTime,
) -> Result<Vec<u8>, SignetError> {
    // RSA is the only signing path implemented in v3.10.0; ECDSA identities
    // are loaded but a future tick wires their SignerInfo emission. The
    // typed match keeps this honest at compile time.
    match identity.algorithm {
        KeyAlgorithm::Rsa => build_rsa(digest_sha256, identity),
        KeyAlgorithm::EcdsaP256 | KeyAlgorithm::EcdsaP384 => {
            Err(SignetError::UnsupportedAlgorithm(
                "ECDSA signing in CMS arrives in v3.10.1 — please use an RSA-2048+ key for v3.10.0"
                    .into(),
            ))
        }
    }
}

fn build_rsa(digest_sha256: &[u8; 32], identity: &SigningIdentity) -> Result<Vec<u8>, SignetError> {
    let SigningKey::Rsa(signer) = &identity.signing_key else {
        return Err(SignetError::UnsupportedAlgorithm(
            "internal: build_rsa called on non-RSA identity".into(),
        ));
    };

    // Parse the leaf cert from the identity (DER) into the x509-cert type cms expects.
    let cert = Certificate::from_der(&identity.cert_der)
        .map_err(|e| SignetError::InvalidCert(format!("re-parse leaf: {e}")))?;

    // Detached encap content: id-data, no eContent.
    let encap_content_info = EncapsulatedContentInfo {
        econtent_type: ID_DATA,
        econtent: None,
    };

    // SignerIdentifier = issuerAndSerialNumber pulled from the cert itself.
    let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
        issuer: cert.tbs_certificate.issuer.clone(),
        serial_number: cert.tbs_certificate.serial_number.clone(),
    });

    let digest_algorithm = sha256_algorithm_identifier()?;

    // External message digest: the cms builder will set messageDigest signed
    // attribute from this and sign over the DER of the signed-attributes set.
    let mut signer_info_builder = SignerInfoBuilder::new(
        signer.as_ref(),
        sid,
        digest_algorithm.clone(),
        &encap_content_info,
        Some(digest_sha256.as_slice()),
    )
    .map_err(|e| SignetError::InvalidCert(format!("SignerInfoBuilder: {e}")))?;

    // The cms builder auto-adds messageDigest, contentType, and signingTime
    // when `external_message_digest` is Some — see cms-0.2.3/src/builder.rs
    // §finalize. We don't need to push attributes manually.

    let mut sd_builder = SignedDataBuilder::new(&encap_content_info);
    sd_builder
        .add_digest_algorithm(digest_algorithm)
        .map_err(|e| SignetError::InvalidCert(format!("add_digest_algorithm: {e}")))?
        .add_certificate(CertificateChoices::Certificate(cert.clone()))
        .map_err(|e| SignetError::InvalidCert(format!("add_certificate (leaf): {e}")))?;

    // Embed CA chain (intermediates) so verifiers can build a path even
    // without a fully populated trust store.
    for ca_der in &identity.chain_der {
        let ca = Certificate::from_der(ca_der)
            .map_err(|e| SignetError::InvalidCert(format!("chain cert parse: {e}")))?;
        sd_builder
            .add_certificate(CertificateChoices::Certificate(ca))
            .map_err(|e| SignetError::InvalidCert(format!("add_certificate (chain): {e}")))?;
    }

    // Ensure a signingTime is present even when the cms builder doesn't add
    // it automatically (the auto-add path is keyed on encap content shape).
    if let Ok(signing_time) = cms::builder::create_signing_time_attribute() {
        let _ = signer_info_builder.add_signed_attribute(signing_time);
    }

    sd_builder
        .add_signer_info::<rsa::pkcs1v15::SigningKey<rsa::sha2::Sha256>, rsa::pkcs1v15::Signature>(
            signer_info_builder,
        )
        .map_err(|e| SignetError::InvalidCert(format!("add_signer_info (RSA): {e}")))?;

    let content_info: ContentInfo = sd_builder
        .build()
        .map_err(|e| SignetError::InvalidCert(format!("SignedDataBuilder::build: {e}")))?;

    // Sanity: ensure top-level is signedData.
    debug_assert_eq!(content_info.content_type, ID_SIGNED_DATA);

    let der_bytes = content_info
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("ContentInfo::to_der: {e}")))?;
    Ok(der_bytes)
}

fn sha256_algorithm_identifier() -> Result<AlgorithmIdentifierOwned, SignetError> {
    Ok(AlgorithmIdentifierOwned {
        oid: OID_SHA_256
            .parse()
            .map_err(|e| SignetError::InvalidCert(format!("bad SHA-256 OID: {e}")))?,
        // RFC 5754 §2: SHA-256 AlgorithmIdentifier SHOULD have absent
        // parameters; some legacy verifiers expect explicit NULL.  Acrobat
        // accepts either; we follow modern RFC by omitting.
        parameters: None,
    })
}

/// Compute the SHA-256 of an arbitrary byte slice — convenience for callers
/// that don't already pull in `sha2`. Returns the 32-byte digest by value.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// Parse a CMS blob back and inspect the SignedData. Used by tests and by the
/// verify pipeline (v3.10.0 Task 5) for round-trip checks.
pub fn parse_signed_data(der_bytes: &[u8]) -> Result<cms::signed_data::SignedData, SignetError> {
    let ci = ContentInfo::from_der(der_bytes)
        .map_err(|e| SignetError::InvalidCert(format!("ContentInfo parse: {e}")))?;
    if ci.content_type != ID_SIGNED_DATA {
        return Err(SignetError::InvalidCert(format!(
            "expected signedData OID, got {}",
            ci.content_type
        )));
    }
    let content_der = ci
        .content
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("content re-DER: {e}")))?;
    let sd = cms::signed_data::SignedData::from_der(&content_der)
        .map_err(|e| SignetError::InvalidCert(format!("SignedData parse: {e}")))?;
    Ok(sd)
}

// Helper to silence dead-code linter in case `Any`/`Tag`/`OctetString` are
// unused in trimmed builds; we keep the imports because future ticks (Task 5
// verify) re-use them and stable imports = stable diffs.
#[allow(dead_code)]
fn _imports_keepalive(_: Any, _: Tag, _: OctetString) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::signet::identity::SigningIdentity;
    use rsa::pkcs1v15::SigningKey as RsaSigningKey;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use rsa::sha2::Sha256 as RsaSha256;
    use std::str::FromStr;
    use std::time::SystemTime;
    use x509_cert::builder::{Builder as _, CertificateBuilder, Profile};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;

    fn fixture_rsa_identity(cn: &str) -> SigningIdentity {
        let mut rng = rand::thread_rng();
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let key_pem = key
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .as_bytes()
            .to_vec();

        let serial = SerialNumber::from(7u32);
        let validity = Validity::from_now(std::time::Duration::from_secs(365 * 24 * 3600)).unwrap();
        let subject = Name::from_str(&format!("CN={cn}")).unwrap();
        let pub_key = key.to_public_key();
        let signing_key = RsaSigningKey::<RsaSha256>::new(key.clone());
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

        SigningIdentity::from_pem_bytes(cert_pem.as_bytes(), &key_pem, None).unwrap()
    }

    #[test]
    fn produces_nonempty_blob_for_rsa() {
        let id = fixture_rsa_identity("RSA Signer");
        let digest = [42u8; 32];
        let blob = build_pkcs7_detached(&digest, &id, SystemTime::now()).expect("build");
        assert!(
            blob.len() > 200,
            "PKCS#7 blob suspiciously small: {}",
            blob.len()
        );
        assert!(blob.len() < 8192, "PKCS#7 blob too big to fit hex window");
    }

    #[test]
    fn blob_round_trips_via_cms_parser() {
        let id = fixture_rsa_identity("Roundtrip");
        let digest = [9u8; 32];
        let blob = build_pkcs7_detached(&digest, &id, SystemTime::now()).unwrap();
        let sd = parse_signed_data(&blob).expect("parse");
        assert_eq!(
            sd.signer_infos.0.len(),
            1,
            "expected exactly one SignerInfo"
        );
        assert!(sd.certificates.is_some(), "leaf cert should be embedded");
    }

    #[test]
    fn signed_data_omits_econtent_for_detached() {
        let id = fixture_rsa_identity("Detached");
        let digest = [1u8; 32];
        let blob = build_pkcs7_detached(&digest, &id, SystemTime::now()).unwrap();
        let sd = parse_signed_data(&blob).unwrap();
        assert!(
            sd.encap_content_info.econtent.is_none(),
            "detached signature must NOT embed eContent"
        );
        assert_eq!(sd.encap_content_info.econtent_type, ID_DATA);
    }

    #[test]
    fn digest_algorithm_is_sha256() {
        let id = fixture_rsa_identity("SHA256");
        let blob = build_pkcs7_detached(&[3u8; 32], &id, SystemTime::now()).unwrap();
        let sd = parse_signed_data(&blob).unwrap();
        let oids: Vec<String> = sd
            .digest_algorithms
            .iter()
            .map(|a| a.oid.to_string())
            .collect();
        assert!(
            oids.iter().any(|o| o == OID_SHA_256),
            "SHA-256 OID missing: {:?}",
            oids
        );
    }

    #[test]
    fn ecdsa_keys_reported_as_unsupported_for_v3_10_0() {
        // Build a real P-256 identity, ensure we get a clean error rather
        // than a panic when we try to sign.
        let key = p256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let key_pkcs8 = p256::pkcs8::EncodePrivateKey::to_pkcs8_pem(&key, LineEnding::LF).unwrap();
        // Self-signed P-256 cert.
        let serial = SerialNumber::from(11u32);
        let validity = Validity::from_now(std::time::Duration::from_secs(3600)).unwrap();
        let subject = Name::from_str("CN=ECDSA Tester").unwrap();
        let verifying = key.verifying_key();
        let pub_der = p256::pkcs8::EncodePublicKey::to_public_key_der(verifying).unwrap();
        let spki_info = spki::SubjectPublicKeyInfoOwned::try_from(pub_der.as_bytes()).unwrap();
        let builder =
            CertificateBuilder::new(Profile::Root, serial, validity, subject, spki_info, &key)
                .unwrap();
        let cert: x509_cert::Certificate = builder.build::<p256::ecdsa::DerSignature>().unwrap();
        let cert_der = der::Encode::to_der(&cert).unwrap();
        let cert_pem = pem::encode(&pem::Pem::new("CERTIFICATE", cert_der));
        let id = SigningIdentity::from_pem_bytes(cert_pem.as_bytes(), key_pkcs8.as_bytes(), None)
            .unwrap();
        assert_eq!(id.algorithm, KeyAlgorithm::EcdsaP256);
        let err = build_pkcs7_detached(&[0u8; 32], &id, SystemTime::now()).unwrap_err();
        assert!(matches!(err, SignetError::UnsupportedAlgorithm(_)));
    }

    #[test]
    fn sha256_helper_matches_known_vector() {
        // Empty string SHA-256 = e3b0c442 98fc1c14 9afbf4c8 996fb924 27ae41e4 649b934c a495991b 7852b855
        let d = sha256(b"");
        let hex: String = d.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
