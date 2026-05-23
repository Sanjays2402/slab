//! RFC 3161 timestamp-authority client.
//!
//! Network calls are opt-in via `SignOptions::tsa_url` (default `None`).
//! When unset, signatures are CAdES-BES grade (no timestamp); when set,
//! signatures are upgraded to CAdES-T by embedding the returned TST as the
//! `id-aa-timeStampToken` (OID `1.2.840.113549.1.9.16.2.14`) unsigned
//! attribute in the CMS SignerInfo.
//!
//! See ADR 0012 for the offline-default rationale.

use der::asn1::{Int, OctetString};
use der::oid::ObjectIdentifier;
use der::{Decode, Encode, Sequence};
use spki::AlgorithmIdentifierOwned;

use crate::pdf::signet::SignetError;

/// SHA-256 OID — same value as `signet::cms_blob::OID_SHA_256`, redeclared here
/// to keep `signet_pro` independent of internal `signet` constants.
pub const SHA256_OID: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// `id-aa-timeStampToken` — the CMS unsigned-attribute OID we embed the
/// returned TST under (RFC 3161 §3.3).
pub const ID_AA_TIMESTAMP_TOKEN: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");

/// RFC 3161 §2.4.1 `MessageImprint`.
///
/// ```asn1
/// MessageImprint ::= SEQUENCE {
///   hashAlgorithm    AlgorithmIdentifier,
///   hashedMessage    OCTET STRING
/// }
/// ```
#[derive(Debug, Clone, Sequence)]
pub struct MessageImprint {
    pub hash_algorithm: AlgorithmIdentifierOwned,
    pub hashed_message: OctetString,
}

/// RFC 3161 §2.4.1 `TimeStampReq`.
///
/// ```asn1
/// TimeStampReq ::= SEQUENCE {
///   version          INTEGER  { v1(1) },
///   messageImprint   MessageImprint,
///   reqPolicy        TSAPolicyId             OPTIONAL,
///   nonce            INTEGER                 OPTIONAL,
///   certReq          BOOLEAN                 DEFAULT FALSE,
///   extensions       [0] IMPLICIT Extensions OPTIONAL
/// }
/// ```
///
/// We omit `reqPolicy` and `extensions` (neither is required by any public
/// TSA we target; FreeTSA, Sectigo, GlobalSign, DigiCert all accept the
/// minimal form).
#[derive(Debug, Clone, Sequence)]
pub struct TimeStampReq {
    pub version: Int,
    pub message_imprint: MessageImprint,
    #[asn1(optional = "true")]
    pub nonce: Option<Int>,
    /// We always request the TSA's cert chain be embedded in the response;
    /// without it we can't build a long-term-validation (LTV) signature.
    pub cert_req: bool,
}

/// Build an RFC 3161 `TimeStampReq` DER blob for the given SHA-256 digest.
///
/// The `nonce` is optional but strongly recommended (RFC 3161 §2.4.1) to
/// prevent replay attacks. Caller should pass a cryptographically random
/// `i64` (e.g. from `rand::random`).
pub fn build_timestamp_req(digest: &[u8; 32], nonce: Option<i64>) -> Result<Vec<u8>, SignetError> {
    let imprint = MessageImprint {
        hash_algorithm: AlgorithmIdentifierOwned {
            oid: SHA256_OID,
            parameters: None,
        },
        hashed_message: OctetString::new(digest.to_vec())
            .map_err(|e| SignetError::InvalidCert(format!("OctetString: {e}")))?,
    };
    let nonce_int = match nonce {
        Some(n) => Some(
            Int::new(&canonical_integer_be(n))
                .map_err(|e| SignetError::InvalidCert(format!("nonce Int: {e}")))?,
        ),
        None => None,
    };
    let req = TimeStampReq {
        version: Int::new(&[1])
            .map_err(|e| SignetError::InvalidCert(format!("version Int: {e}")))?,
        message_imprint: imprint,
        nonce: nonce_int,
        cert_req: true,
    };
    req.to_der()
        .map_err(|e| SignetError::InvalidCert(format!("TimeStampReq::to_der: {e}")))
}

/// Strip redundant leading zero / 0xFF bytes so the value matches DER's
/// canonical-INTEGER form (RFC 5280, X.690 §8.3). `der::asn1::Int::new`
/// requires this; `i64::to_be_bytes` does not.
fn canonical_integer_be(value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut out: &[u8] = &bytes;
    // Drop a leading byte iff it's redundant (top bit of next byte agrees).
    while out.len() > 1 {
        let first = out[0];
        let next = out[1];
        let redundant_pos = first == 0x00 && (next & 0x80) == 0;
        let redundant_neg = first == 0xFF && (next & 0x80) != 0;
        if redundant_pos || redundant_neg {
            out = &out[1..];
        } else {
            break;
        }
    }
    out.to_vec()
}

// ---------------------------------------------------------------------------
// Response side
// ---------------------------------------------------------------------------

/// RFC 3161 §2.4.2 `PKIStatusInfo` — minimal subset we care about.
#[derive(Debug, Clone, Sequence)]
struct PkiStatusInfo {
    status: Int,
    // status_string, fail_info are optional — we don't decode them yet.
}

/// RFC 3161 §2.4.2 `TimeStampResp`.
///
/// ```asn1
/// TimeStampResp ::= SEQUENCE {
///   status      PKIStatusInfo,
///   timeStampToken TimeStampToken OPTIONAL  -- a ContentInfo
/// }
/// ```
///
/// We carry the TST as an opaque `der::Any` so callers can splice it directly
/// into a CMS SignerInfo as the `id-aa-timeStampToken` unsigned attribute
/// value without re-encoding.
#[derive(Debug, Clone, Sequence)]
struct RawTimeStampResp {
    status: PkiStatusInfo,
    #[asn1(optional = "true")]
    time_stamp_token: Option<der::Any>,
}

/// Parsed RFC 3161 response — high-level view.
#[derive(Debug, Clone)]
pub struct TimestampResp {
    /// PKIStatus integer (0 = granted, 1 = grantedWithMods, 2 = rejection, …).
    pub status: u32,
    /// Raw TST bytes (a CMS ContentInfo of type id-signedData). Empty if the
    /// TSA rejected the request.
    pub token: Vec<u8>,
}

impl TimestampResp {
    /// True when the TSA accepted the request (PKIStatus 0 or 1).
    pub fn status_granted(&self) -> bool {
        self.status == 0 || self.status == 1
    }
}

/// Parse a DER-encoded `TimeStampResp` returned by a TSA over HTTP.
pub fn parse_timestamp_resp(bytes: &[u8]) -> Result<TimestampResp, SignetError> {
    let raw = RawTimeStampResp::from_der(bytes)
        .map_err(|e| SignetError::InvalidCert(format!("TimeStampResp parse: {e}")))?;

    // Decode the status INTEGER. PKIStatus values are 0..=5 in RFC 3161;
    // anything wider is a protocol violation.
    let status_bytes = raw.status.status.as_bytes();
    if status_bytes.len() > 4 {
        return Err(SignetError::InvalidCert(format!(
            "PKIStatus integer too wide: {} bytes",
            status_bytes.len()
        )));
    }
    let mut buf = [0u8; 4];
    buf[4 - status_bytes.len()..].copy_from_slice(status_bytes);
    let status = u32::from_be_bytes(buf);

    let token = match raw.time_stamp_token {
        Some(tok) => tok
            .to_der()
            .map_err(|e| SignetError::InvalidCert(format!("TST re-encode: {e}")))?,
        None => Vec::new(),
    };

    Ok(TimestampResp { status, token })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_req_encodes_sha256_imprint() {
        let digest = [0x42u8; 32];
        let req = build_timestamp_req(&digest, Some(0x0123_4567_89AB_CDEFi64)).expect("encode");
        // SEQUENCE
        assert_eq!(req[0], 0x30);
        // SHA-256 OID DER tail: 2.16.840.1.101.3.4.2.1.
        let sha256_oid = [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
        assert!(
            req.windows(sha256_oid.len()).any(|w| w == sha256_oid),
            "SHA-256 OID missing from TimeStampReq DER"
        );
        // Digest bytes copied verbatim.
        assert!(
            req.windows(32).any(|w| w == digest),
            "digest bytes missing from TimeStampReq DER"
        );
    }

    #[test]
    fn timestamp_req_omits_nonce_when_none() {
        let digest = [0x11u8; 32];
        let with_nonce = build_timestamp_req(&digest, Some(0xDEADBEEFi64)).unwrap();
        let without_nonce = build_timestamp_req(&digest, None).unwrap();
        // Omitting an OPTIONAL field strictly shortens the DER.
        assert!(without_nonce.len() < with_nonce.len());
    }

    #[test]
    fn timestamp_req_round_trips() {
        let digest = [0x77u8; 32];
        let der = build_timestamp_req(&digest, Some(42)).unwrap();
        let parsed = TimeStampReq::from_der(&der).expect("round-trip");
        assert_eq!(parsed.cert_req, true);
        assert_eq!(parsed.message_imprint.hash_algorithm.oid, SHA256_OID);
        assert_eq!(parsed.message_imprint.hashed_message.as_bytes(), &digest);
    }

    /// Hand-rolled minimal `TimeStampResp` DER: status=0 (granted), no token.
    /// Confirms `parse_timestamp_resp` accepts a well-formed response and
    /// reports `status_granted() == true`.
    #[test]
    fn parse_minimal_granted_response() {
        // SEQUENCE { SEQUENCE { INTEGER 0 } }
        // outer: 30 05
        //   inner status: 30 03
        //     INTEGER 0: 02 01 00
        let resp = [0x30u8, 0x05, 0x30, 0x03, 0x02, 0x01, 0x00];
        let parsed = parse_timestamp_resp(&resp).expect("parse");
        assert_eq!(parsed.status, 0);
        assert!(parsed.status_granted());
        assert!(parsed.token.is_empty(), "no token expected in minimal resp");
    }

    #[test]
    fn parse_rejection_response() {
        // status = 2 (rejection)
        let resp = [0x30u8, 0x05, 0x30, 0x03, 0x02, 0x01, 0x02];
        let parsed = parse_timestamp_resp(&resp).expect("parse");
        assert_eq!(parsed.status, 2);
        assert!(!parsed.status_granted());
    }

    #[test]
    fn parse_malformed_resp_errors() {
        let garbage = [0xFFu8; 16];
        assert!(parse_timestamp_resp(&garbage).is_err());
    }

    #[test]
    fn id_aa_timestamp_token_oid_value() {
        // Sanity-check the OID we splice into CMS SignerInfo.
        assert_eq!(
            ID_AA_TIMESTAMP_TOKEN.to_string(),
            "1.2.840.113549.1.9.16.2.14"
        );
    }
}
