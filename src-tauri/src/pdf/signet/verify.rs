//! Verify a signed PDF.
//!
//! v3.10.0 — single-pass verifier for adbe.pkcs7.detached signatures.
//!
//! Per ISO 32000-1 §12.8 + Adobe profile:
//!
//! 1. Walk AcroForm.Fields to collect all `/FT /Sig` fields.
//! 2. For each, read `/V → Sig dict → /ByteRange + /Contents`.
//! 3. Hex-decode `/Contents`, strip trailing zero padding.
//! 4. Hash the file across the four-tuple ByteRange.
//! 5. Parse the CMS blob via [`super::cms_blob::parse_signed_data`].
//! 6. Compare the SHA-256 we just computed against the `messageDigest`
//!    signed attribute the signer claimed.
//! 7. Verify the SignerInfo signature using the embedded leaf cert's
//!    public key. (v3.10.0: RSA-PKCS#1 v1.5 with SHA-256 only; ECDSA
//!    arrives in v3.10.1 alongside ECDSA *signing*.)
//! 8. Compute Coverage: FullDocument iff `byte_range[0] == 0`,
//!    `byte_range[1] == hex_open`, and `byte_range[2] + byte_range[3]
//!    == file_len`. Otherwise PartialDocument (an incremental update
//!    was appended after the signature — Acrobat does the same check).
//! 9. Run [`TrustStore::verify_chain`] for ChainStatus.
//!
//! Revocation (CRL / OCSP) is intentionally **not** checked in v3.10.0.
//! The UI surfaces this with a "revocation: not checked" note.

use std::path::Path;
use std::time::SystemTime;

use cms::signed_data::SignerIdentifier;
use const_oid::db::rfc5911::{ID_MESSAGE_DIGEST, ID_SIGNING_TIME};
use const_oid::ObjectIdentifier;
use der::{Decode, Encode};
use lopdf::{Document, Object, ObjectId};
use rsa::pkcs1v15::{Signature as RsaSignature, VerifyingKey as RsaVerifyingKey};
use rsa::sha2::Sha256 as RsaSha256;
use rsa::signature::Verifier;
use sha2::Sha256;
use x509_cert::Certificate;

use super::cms_blob::parse_signed_data;
use super::identity::SignetError;
use super::trust::{ChainStatus, TrustStore};

/// Did the signed bytes cover the entire on-disk file?
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Coverage {
    /// ByteRange covered every byte of the file outside the hex window.
    /// This is what we want.
    FullDocument,
    /// Bytes exist after `byte_range[2] + byte_range[3]` — i.e. somebody
    /// appended an incremental update after the signature was applied.
    /// The original signed content is still trustworthy, but the *current*
    /// document goes beyond what was signed.
    PartialDocument,
}

/// SHA-256 hash claimed in CMS vs computed from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DigestStatus {
    /// `messageDigest` signed-attr equals our SHA-256 over the ByteRange.
    Match,
    /// Mismatch — content was tampered after signing.
    Mismatch,
}

/// Signature math result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CryptoStatus {
    /// SignerInfo.signature verifies against the leaf cert's public key.
    Valid,
    /// Signature math failed.
    Invalid,
    /// We don't yet verify this algorithm (e.g. ECDSA on v3.10.0).
    UnsupportedAlgorithm,
}

/// One signature found in the PDF.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifiedSignature {
    pub field_name: String,
    pub signer_cn: String,
    pub signed_at_unix: i64,
    pub byte_range: [u64; 4],
    pub coverage: Coverage,
    pub digest_status: DigestStatus,
    pub crypto_status: CryptoStatus,
    pub chain_status: ChainStatus,
    pub cert_subject: String,
    pub cert_issuer: String,
    pub cert_not_before: i64,
    pub cert_not_after: i64,
}

/// Verify every signature in `input`. Returns one entry per Sig field, in
/// the order they appear in AcroForm.Fields. Empty Vec if the PDF has no
/// signatures.
pub fn verify(input: &Path, trust: &TrustStore) -> Result<Vec<VerifiedSignature>, SignetError> {
    let file_bytes =
        std::fs::read(input).map_err(|e| SignetError::Io(std::io::Error::other(e.to_string())))?;
    verify_bytes(&file_bytes, trust)
}

/// Same as [`verify`] but operates on an in-memory PDF — used by tests.
pub fn verify_bytes(
    file_bytes: &[u8],
    trust: &TrustStore,
) -> Result<Vec<VerifiedSignature>, SignetError> {
    let doc = Document::load_mem(file_bytes)
        .map_err(|e| SignetError::InvalidCert(format!("lopdf load: {e}")))?;

    let sig_fields = collect_sig_fields(&doc)?;
    let mut out = Vec::with_capacity(sig_fields.len());
    for f in sig_fields {
        out.push(verify_one(&doc, &f, file_bytes, trust)?);
    }
    Ok(out)
}

struct SigField {
    name: String,
    /// Object ID of the Sig dictionary (the `/V` target).
    sig_id: ObjectId,
}

fn collect_sig_fields(doc: &Document) -> Result<Vec<SigField>, SignetError> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| SignetError::InvalidCert(format!("trailer Root: {e}")))?;
    let catalog = match doc.get_object(catalog_id) {
        Ok(Object::Dictionary(d)) => d,
        _ => return Err(SignetError::InvalidCert("catalog not a dict".into())),
    };
    let Some(acroform_obj) = catalog.get(b"AcroForm").ok() else {
        return Ok(Vec::new());
    };
    let acroform = match acroform_obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(rid) => match doc.get_object(*rid) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return Err(SignetError::InvalidCert("AcroForm not a dict".into())),
        },
        _ => return Err(SignetError::InvalidCert("AcroForm wrong type".into())),
    };
    let fields = match acroform.get(b"Fields") {
        Ok(Object::Array(a)) => a.clone(),
        _ => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    for field_ref in fields {
        let field_id = match field_ref {
            Object::Reference(rid) => rid,
            _ => continue,
        };
        let field_dict = match doc.get_object(field_id) {
            Ok(Object::Dictionary(d)) => d,
            _ => continue,
        };
        let ft = field_dict.get(b"FT").ok().and_then(|o| o.as_name().ok());
        if ft != Some(b"Sig") {
            continue;
        }
        let name = field_dict
            .get(b"T")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| "Signature".to_string());
        let sig_id = match field_dict.get(b"V") {
            Ok(Object::Reference(rid)) => *rid,
            _ => continue,
        };
        out.push(SigField { name, sig_id });
    }
    Ok(out)
}

fn verify_one(
    doc: &Document,
    field: &SigField,
    file_bytes: &[u8],
    trust: &TrustStore,
) -> Result<VerifiedSignature, SignetError> {
    let sig_dict = match doc.get_object(field.sig_id) {
        Ok(Object::Dictionary(d)) => d,
        _ => return Err(SignetError::InvalidCert("Sig field /V not a dict".into())),
    };

    let byte_range = parse_byte_range(sig_dict)?;
    let contents_hex = match sig_dict.get(b"Contents") {
        Ok(Object::String(b, _)) => b.clone(),
        _ => {
            return Err(SignetError::InvalidCert(
                "Sig dict missing /Contents".into(),
            ))
        }
    };
    // lopdf gives us the *decoded* hex bytes including any trailing zero
    // padding we baked into the placeholder. Trim to the DER outer-SEQUENCE
    // length before parsing.
    let der_len = der_envelope_len(&contents_hex).ok_or_else(|| {
        SignetError::InvalidCert("Could not determine CMS DER length from /Contents".into())
    })?;
    let signed_data = parse_signed_data(&contents_hex[..der_len])?;

    // Compute SHA-256 over the four ByteRange spans.
    let computed_digest = digest_byte_range(file_bytes, &byte_range)?;

    // Extract the leaf certificate from CMS.
    let leaf_cert = extract_leaf_cert(&signed_data)?;
    let leaf_der = leaf_cert
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("leaf re-DER: {e}")))?;

    // Extract SignerInfo (we built exactly one).
    let signer_info = signed_data
        .signer_infos
        .0
        .as_slice()
        .first()
        .ok_or_else(|| SignetError::InvalidCert("no SignerInfo".into()))?;

    // 1. messageDigest signed-attribute → DigestStatus.
    let claimed_digest = find_signed_attr_octets(signer_info, ID_MESSAGE_DIGEST)?;
    let digest_status = if claimed_digest == computed_digest {
        DigestStatus::Match
    } else {
        DigestStatus::Mismatch
    };

    // 2. signingTime signed-attribute → signed_at_unix.
    let signed_at_unix = parse_signing_time(signer_info).unwrap_or(0);

    // 3. Crypto verify: hash the DER-encoded signedAttrs SET and verify
    //    the signature against the leaf's public key.
    let crypto_status = verify_signer_info(signer_info, &leaf_cert);

    // 4. Coverage.
    let hex_open_pos = find_contents_open(file_bytes)?;
    let coverage = compute_coverage(file_bytes.len() as u64, &byte_range, hex_open_pos as u64);

    // 5. Chain.
    let chain_intermediates: Vec<Vec<u8>> = collect_chain_der(&signed_data, &leaf_der);
    let chain_status = trust.verify_chain(&leaf_der, &chain_intermediates, SystemTime::now());

    // Display fields.
    let signer_cn = cn_from_name(&leaf_cert.tbs_certificate.subject);
    let cert_subject = leaf_cert.tbs_certificate.subject.to_string();
    let cert_issuer = leaf_cert.tbs_certificate.issuer.to_string();
    let (cert_not_before, cert_not_after) = validity_unix(&leaf_cert);

    Ok(VerifiedSignature {
        field_name: field.name.clone(),
        signer_cn,
        signed_at_unix,
        byte_range,
        coverage,
        digest_status,
        crypto_status,
        chain_status,
        cert_subject,
        cert_issuer,
        cert_not_before,
        cert_not_after,
    })
}

fn parse_byte_range(sig_dict: &lopdf::Dictionary) -> Result<[u64; 4], SignetError> {
    let arr = match sig_dict.get(b"ByteRange") {
        Ok(Object::Array(a)) => a,
        _ => {
            return Err(SignetError::InvalidCert(
                "Sig dict missing /ByteRange".into(),
            ))
        }
    };
    if arr.len() != 4 {
        return Err(SignetError::InvalidCert(format!(
            "/ByteRange has {} entries, want 4",
            arr.len()
        )));
    }
    let mut out = [0u64; 4];
    for (i, entry) in arr.iter().enumerate() {
        out[i] = match entry {
            Object::Integer(n) => *n as u64,
            // After in-place rewrite we usually have Integer, but accept
            // a literal-string placeholder turned into numeric digits too
            // (string-form unlikely in a real signed file but harmless).
            Object::String(s, _) => std::str::from_utf8(s)
                .ok()
                .and_then(|t| t.trim().parse::<u64>().ok())
                .ok_or_else(|| {
                    SignetError::InvalidCert("ByteRange string entry not numeric".into())
                })?,
            _ => {
                return Err(SignetError::InvalidCert(
                    "ByteRange entry not numeric".into(),
                ))
            }
        };
    }
    Ok(out)
}

fn digest_byte_range(file_bytes: &[u8], br: &[u64; 4]) -> Result<[u8; 32], SignetError> {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    let len = file_bytes.len() as u64;
    let (a, b, c, d) = (br[0], br[1], br[2], br[3]);
    if a.checked_add(b).is_none()
        || c.checked_add(d).is_none()
        || a + b > len
        || c + d > len
        || c < a + b
    {
        return Err(SignetError::InvalidCert(format!(
            "ByteRange out of bounds: [{a} {b} {c} {d}] vs file_len={len}"
        )));
    }
    hasher.update(&file_bytes[a as usize..(a + b) as usize]);
    hasher.update(&file_bytes[c as usize..(c + d) as usize]);
    let out = hasher.finalize();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    Ok(buf)
}

fn extract_leaf_cert(sd: &cms::signed_data::SignedData) -> Result<Certificate, SignetError> {
    use cms::cert::CertificateChoices;
    let certs = sd
        .certificates
        .as_ref()
        .ok_or_else(|| SignetError::InvalidCert("CMS has no certificates".into()))?;
    let signer = sd
        .signer_infos
        .0
        .as_slice()
        .first()
        .ok_or_else(|| SignetError::InvalidCert("no SignerInfo".into()))?;
    let SignerIdentifier::IssuerAndSerialNumber(want) = &signer.sid else {
        return Err(SignetError::InvalidCert(
            "SignerIdentifier::SubjectKeyIdentifier not supported (v3.10.0 expects IAS)".into(),
        ));
    };
    for choice in certs.0.iter() {
        if let CertificateChoices::Certificate(c) = choice {
            if c.tbs_certificate.serial_number == want.serial_number
                && c.tbs_certificate.issuer == want.issuer
            {
                return Ok(c.clone());
            }
        }
    }
    Err(SignetError::InvalidCert(
        "leaf cert matching SignerInfo.sid not found in CMS".into(),
    ))
}

fn find_signed_attr_octets(
    si: &cms::signed_data::SignerInfo,
    oid: ObjectIdentifier,
) -> Result<Vec<u8>, SignetError> {
    let Some(attrs) = si.signed_attrs.as_ref() else {
        return Err(SignetError::InvalidCert(
            "SignerInfo has no signedAttrs".into(),
        ));
    };
    for attr in attrs.iter() {
        if attr.oid == oid {
            let val =
                attr.values.as_slice().first().ok_or_else(|| {
                    SignetError::InvalidCert("attribute has empty value set".into())
                })?;
            let der = val
                .to_der()
                .map_err(|e| SignetError::InvalidCert(format!("attr to_der: {e}")))?;
            // OCTET STRING: tag 0x04, then length, then bytes.
            let os = der::asn1::OctetString::from_der(&der)
                .map_err(|e| SignetError::InvalidCert(format!("OCTET STRING parse: {e}")))?;
            return Ok(os.as_bytes().to_vec());
        }
    }
    Err(SignetError::InvalidCert(format!(
        "signed attribute {oid} not found"
    )))
}

fn parse_signing_time(si: &cms::signed_data::SignerInfo) -> Option<i64> {
    let attrs = si.signed_attrs.as_ref()?;
    for attr in attrs.iter() {
        if attr.oid == ID_SIGNING_TIME {
            let val = attr.values.as_slice().first()?;
            let der = val.to_der().ok()?;
            // Try UTCTime first, then GeneralizedTime.
            if let Ok(t) = der::asn1::UtcTime::from_der(&der) {
                return Some(
                    t.to_unix_duration()
                        .as_secs()
                        .try_into()
                        .unwrap_or(i64::MAX),
                );
            }
            if let Ok(t) = der::asn1::GeneralizedTime::from_der(&der) {
                return Some(
                    t.to_unix_duration()
                        .as_secs()
                        .try_into()
                        .unwrap_or(i64::MAX),
                );
            }
        }
    }
    None
}

fn verify_signer_info(si: &cms::signed_data::SignerInfo, cert: &Certificate) -> CryptoStatus {
    // CMS signedAttrs are signed as the DER of `IMPLICIT [0]` re-tagged
    // as an explicit SET (tag 0x31). cms-0.2's `SignedAttributes::to_der`
    // already emits the SET form.
    let Some(attrs) = si.signed_attrs.as_ref() else {
        return CryptoStatus::Invalid;
    };
    // cms attrs encode as IMPLICIT [0] SET; we need the SET form (0x31) — call
    // .to_der() then patch the tag.
    let Ok(mut signed_data_to_hash) = attrs.to_der() else {
        return CryptoStatus::Invalid;
    };
    // Replace context-specific [0] (0xA0) with SET (0x31) per RFC 5652 §5.4.
    if signed_data_to_hash.first() == Some(&0xA0) {
        signed_data_to_hash[0] = 0x31;
    }

    // Algorithm gate: RSA-PKCS#1 v1.5 only for v3.10.0.
    // sha256WithRSAEncryption = 1.2.840.113549.1.1.11
    // rsaEncryption          = 1.2.840.113549.1.1.1
    let sig_alg = si.signature_algorithm.oid.to_string();
    let is_rsa = matches!(
        sig_alg.as_str(),
        "1.2.840.113549.1.1.11" | "1.2.840.113549.1.1.1"
    );
    if !is_rsa {
        return CryptoStatus::UnsupportedAlgorithm;
    }

    // Extract the leaf's RSA public key.
    let spki_der = match cert.tbs_certificate.subject_public_key_info.to_der() {
        Ok(d) => d,
        Err(_) => return CryptoStatus::Invalid,
    };
    let rsa_pub: rsa::RsaPublicKey =
        match rsa::pkcs8::DecodePublicKey::from_public_key_der(&spki_der) {
            Ok(k) => k,
            Err(_) => return CryptoStatus::Invalid,
        };
    let verifying_key = RsaVerifyingKey::<RsaSha256>::new(rsa_pub);

    let sig_bytes = si.signature.as_bytes();
    let Ok(sig) = RsaSignature::try_from(sig_bytes) else {
        return CryptoStatus::Invalid;
    };

    if verifying_key.verify(&signed_data_to_hash, &sig).is_ok() {
        CryptoStatus::Valid
    } else {
        CryptoStatus::Invalid
    }
}

fn find_contents_open(bytes: &[u8]) -> Result<usize, SignetError> {
    // Locate the /Contents whose value is a hex string (`<...>`). The page
    // dict's /Contents is an array, so we skip occurrences that aren't
    // followed by `<`.
    let needle = b"/Contents";
    let mut start = 0;
    while let Some(rel) = memmem(&bytes[start..], needle) {
        let pos = start + rel;
        let mut i = pos + needle.len();
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'<' {
            return Ok(i);
        }
        start = pos + needle.len();
    }
    Err(SignetError::InvalidCert(
        "no /Contents <hex> string in signed PDF".into(),
    ))
}

fn compute_coverage(file_len: u64, br: &[u64; 4], hex_open: u64) -> Coverage {
    // ByteRange should be [0, hex_open, hex_close+1, file_len-(hex_close+1)]
    // where hex_close+1 = br[2]. So br[0]+br[1] == hex_open AND
    // br[2]+br[3] == file_len.
    let head_ok = br[0] == 0 && br[0] + br[1] == hex_open;
    let tail_ok = br[2] + br[3] == file_len;
    if head_ok && tail_ok {
        Coverage::FullDocument
    } else {
        Coverage::PartialDocument
    }
}

fn collect_chain_der(sd: &cms::signed_data::SignedData, leaf_der: &[u8]) -> Vec<Vec<u8>> {
    use cms::cert::CertificateChoices;
    let mut out = Vec::new();
    if let Some(set) = sd.certificates.as_ref() {
        for choice in set.0.iter() {
            if let CertificateChoices::Certificate(c) = choice {
                if let Ok(der) = c.to_der() {
                    if der != leaf_der {
                        out.push(der);
                    }
                }
            }
        }
    }
    out
}

fn cn_from_name(name: &x509_cert::name::Name) -> String {
    // RFC 4514 string is like "CN=Alice,O=Acme". Pluck the CN.
    let s = name.to_string();
    for part in s.split(',') {
        let p = part.trim();
        if let Some(rest) = p.strip_prefix("CN=") {
            return rest.to_string();
        }
    }
    s
}

fn validity_unix(cert: &Certificate) -> (i64, i64) {
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

fn der_envelope_len(bytes: &[u8]) -> Option<usize> {
    // Generic ASN.1 DER outer TLV length. Tag byte + length encoding.
    if bytes.len() < 2 {
        return None;
    }
    let len_byte = bytes[1];
    if len_byte < 0x80 {
        Some(2 + len_byte as usize)
    } else {
        let n = (len_byte & 0x7f) as usize;
        if n == 0 || n > 4 || bytes.len() < 2 + n {
            return None;
        }
        let mut len = 0usize;
        for &b in &bytes[2..2 + n] {
            len = (len << 8) | b as usize;
        }
        Some(2 + n + len)
    }
}

fn memmem(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let last = hay.len() - needle.len();
    let first = needle[0];
    let mut i = 0;
    while i <= last {
        if hay[i] == first && hay[i..i + needle.len()] == *needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::signet::sign::{sign_pdf, SignOptions};
    use crate::pdf::signet::SigningIdentity;
    use lopdf::dictionary;
    use rsa::pkcs1v15::SigningKey as RsaSigningKey;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use rsa::sha2::Sha256 as RsaSha256;
    use std::str::FromStr;
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

    fn write_minimal_pdf(path: &Path) {
        // Use lopdf to produce a syntactically valid minimal PDF so its
        // own loader accepts it on the verify side.
        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
                "Contents" => Object::Array(vec![]),
                "Resources" => Object::Dictionary(dictionary!{}),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.save(path).unwrap();
    }

    #[test]
    fn visible_appearance_renders_widget_with_ap_n_xobject() {
        use crate::pdf::signet_pro::appearance::AppearanceSpec;
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("in.pdf");
        let output = tmp.path().join("out.pdf");
        write_minimal_pdf(&input);
        let id = fixture_rsa_identity("Visible Signer");
        let opts = SignOptions {
            reason: Some("Approval".into()),
            location: Some("Seattle, WA".into()),
            contact_info: None,
            field_name: None,
            appearance: Some(AppearanceSpec {
                page: 1,
                rect: [50.0, 50.0, 280.0, 130.0],
                font_size: 9.0,
                show_name: true,
                show_date: true,
                show_reason: true,
                show_location: true,
                image: None,
                reason: Some("Approval".into()),
                location: Some("Seattle, WA".into()),
                signing_time: Some("2026-05-23 22:00 UTC".into()),
            }),
            tsa_url: None,
        };
        sign_pdf(&input, &output, &id, &opts).unwrap();

        // 1. Verification still passes — visible appearance must NOT break the
        //    PKCS#7 byte-range protection.
        let store = TrustStore::new();
        let results = verify(&output, &store).expect("verify");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].digest_status, DigestStatus::Match);
        assert_eq!(results[0].crypto_status, CryptoStatus::Valid);

        // 2. Reload the signed PDF and assert structural properties via the
        //    PDF object graph (avoids coupling to serializer whitespace and
        //    flate-encoding of streams).
        let signed = lopdf::Document::load(&output).unwrap();
        let mut found_form_xobject = false;
        let mut found_widget_with_ap = false;
        for (_, obj) in signed.objects.iter() {
            match obj {
                lopdf::Object::Stream(s) => {
                    if matches!(s.dict.get(b"Subtype"), Ok(lopdf::Object::Name(n)) if n == b"Form")
                    {
                        found_form_xobject = true;
                    }
                }
                lopdf::Object::Dictionary(d) => {
                    let is_widget =
                        matches!(d.get(b"Subtype"), Ok(lopdf::Object::Name(n)) if n == b"Widget");
                    if is_widget && d.get(b"AP").is_ok() {
                        found_widget_with_ap = true;
                    }
                }
                _ => {}
            }
        }
        assert!(found_form_xobject, "no /Subtype /Form XObject in output");
        assert!(found_widget_with_ap, "no Widget with /AP entry in output");
    }

    #[test]
    fn sign_then_verify_same_file_passes_all_checks() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("in.pdf");
        let output = tmp.path().join("out.pdf");
        write_minimal_pdf(&input);
        let id = fixture_rsa_identity("Alice Test");
        let _ = sign_pdf(&input, &output, &id, &SignOptions::default()).unwrap();

        let store = TrustStore::new();
        let results = verify(&output, &store).expect("verify");
        assert_eq!(results.len(), 1, "expected exactly one signature");
        let s = &results[0];
        assert_eq!(s.digest_status, DigestStatus::Match, "digest must match");
        assert_eq!(s.crypto_status, CryptoStatus::Valid, "crypto must verify");
        assert_eq!(s.coverage, Coverage::FullDocument);
        assert_eq!(s.chain_status, ChainStatus::SelfSigned);
        assert_eq!(s.signer_cn, "Alice Test");
    }

    #[test]
    fn tampering_invalidates_digest() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("in.pdf");
        let output = tmp.path().join("out.pdf");
        write_minimal_pdf(&input);
        let id = fixture_rsa_identity("Bob");
        let report = sign_pdf(&input, &output, &id, &SignOptions::default()).unwrap();

        // Flip an ASCII letter in the signed region but past the file header.
        // Skip the %PDF magic (first 9 bytes) and find the next letter.
        let mut bytes = std::fs::read(&output).unwrap();
        let tamper_at = bytes
            .iter()
            .enumerate()
            .skip(20)
            .find(|(_, b)| b.is_ascii_alphabetic())
            .map(|(i, _)| i)
            .unwrap();
        bytes[tamper_at] ^= 0x20; // flip case
        let _ = report;
        std::fs::write(&output, &bytes).unwrap();

        let store = TrustStore::new();
        let results = verify(&output, &store).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].digest_status,
            DigestStatus::Mismatch,
            "tampered file must report digest mismatch"
        );
    }

    #[test]
    fn incremental_update_reports_partial_coverage() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("in.pdf");
        let output = tmp.path().join("out.pdf");
        write_minimal_pdf(&input);
        let id = fixture_rsa_identity("Carol");
        let _ = sign_pdf(&input, &output, &id, &SignOptions::default()).unwrap();

        // Append a trailing comment — simulates a downstream tool tacking on
        // an incremental update after the signature was applied.
        let mut bytes = std::fs::read(&output).unwrap();
        bytes.extend_from_slice(b"\n% appended after signing\n");
        std::fs::write(&output, &bytes).unwrap();

        let store = TrustStore::new();
        let results = verify(&output, &store).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].coverage, Coverage::PartialDocument);
        // Digest still matches — the appended bytes are outside ByteRange.
        assert_eq!(results[0].digest_status, DigestStatus::Match);
    }

    #[test]
    fn unsigned_pdf_returns_empty_vec() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plain.pdf");
        write_minimal_pdf(&path);
        let store = TrustStore::new();
        let results = verify(&path, &store).unwrap();
        assert!(results.is_empty());
    }
}
