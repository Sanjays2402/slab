//! RFC 3161 timestamp-authority client.
//!
//! Network calls are opt-in via `SignOptions::tsa_url` (default `None`).
//! When unset, signatures are CAdES-BES grade (no timestamp); when set,
//! signatures are upgraded to CAdES-T by embedding the returned TST as the
//! `id-aa-timeStampToken` (OID `1.2.840.113549.1.9.16.2.14`) unsigned
//! attribute in the CMS SignerInfo.
//!
//! See ADR 0012 for the offline-default rationale.

use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use const_oid::db::rfc5911::ID_SIGNED_DATA;
use der::asn1::{Int, OctetString, SetOfVec};
use der::oid::ObjectIdentifier;
use der::{Any, Decode, Encode, Sequence};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::Attribute;

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
// CAdES-T embedding — splice a TST into an existing detached CMS blob as
// the `id-aa-timeStampToken` unsigned attribute on the SignerInfo.
// ---------------------------------------------------------------------------

/// Embed an RFC 3161 timestamp token into a previously-built detached CMS
/// `ContentInfo` (the DER bytes that go into the PDF `/Contents` window),
/// upgrading the signature from CAdES-BES to CAdES-T.
///
/// `cms_der` is the output of `signet::cms_blob::build_pkcs7_detached`.
/// `tst_der` is the raw `TimeStampToken` bytes from `TimestampResp::token`
/// (itself a CMS ContentInfo of type id-signedData — we wrap it as a single
/// AttributeValue `Any` and attach it).
///
/// Returns a fresh DER-encoded ContentInfo with the unsigned-attr added to
/// the single SignerInfo. If the input has more than one SignerInfo we
/// attach the timestamp to all of them (Slab only emits one, but defending
/// against future fan-out keeps the helper general).
pub fn embed_timestamp_token(cms_der: &[u8], tst_der: &[u8]) -> Result<Vec<u8>, SignetError> {
    if tst_der.is_empty() {
        return Err(SignetError::InvalidCert(
            "embed_timestamp_token: empty TST bytes".into(),
        ));
    }
    // Parse the outer ContentInfo and assert SignedData.
    let ci = ContentInfo::from_der(cms_der)
        .map_err(|e| SignetError::InvalidCert(format!("CMS ContentInfo parse: {e}")))?;
    if ci.content_type != ID_SIGNED_DATA {
        return Err(SignetError::InvalidCert(format!(
            "expected signedData OID, got {}",
            ci.content_type
        )));
    }
    let content_der = ci
        .content
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("re-DER inner: {e}")))?;
    let mut sd = SignedData::from_der(&content_der)
        .map_err(|e| SignetError::InvalidCert(format!("SignedData parse: {e}")))?;

    // Parse the TST as a `der::Any` so we attach it without re-encoding —
    // verifiers (Acrobat, the cms crate) expect the unsigned-attr value to
    // be the literal ContentInfo bytes.
    let tst_any = Any::from_der(tst_der)
        .map_err(|e| SignetError::InvalidCert(format!("TST parse as Any: {e}")))?;

    // Build the unsigned `Attribute { oid = id-aa-timeStampToken, values = { tst } }`.
    let mut values_set: SetOfVec<Any> = SetOfVec::new();
    values_set
        .insert(tst_any)
        .map_err(|e| SignetError::InvalidCert(format!("attr value set: {e}")))?;
    let ts_attr = Attribute {
        oid: ID_AA_TIMESTAMP_TOKEN,
        values: values_set,
    };

    // Attach to every SignerInfo. SignerInfos wraps a SetOfVec; we collect,
    // mutate each one, then rebuild the set.
    let signers: Vec<cms::signed_data::SignerInfo> = sd
        .signer_infos
        .0
        .iter()
        .map(|si| {
            let mut new_si = si.clone();
            let mut attrs_vec: Vec<Attribute> = new_si
                .unsigned_attrs
                .as_ref()
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();
            // Replace any pre-existing timestamp-token attr (idempotent).
            attrs_vec.retain(|a| a.oid != ID_AA_TIMESTAMP_TOKEN);
            attrs_vec.push(ts_attr.clone());
            let attrs_set: SetOfVec<Attribute> = SetOfVec::try_from(attrs_vec)
                .map_err(|e| SignetError::InvalidCert(format!("unsigned attrs SetOfVec: {e}")))?;
            new_si.unsigned_attrs = Some(attrs_set);
            Ok::<_, SignetError>(new_si)
        })
        .collect::<Result<_, _>>()?;

    sd.signer_infos = cms::signed_data::SignerInfos::try_from(signers)
        .map_err(|e| SignetError::InvalidCert(format!("SignerInfos rebuild: {e}")))?;

    // Re-wrap as ContentInfo.
    let inner_der = sd
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("SignedData re-DER: {e}")))?;
    let inner_any =
        Any::from_der(&inner_der).map_err(|e| SignetError::InvalidCert(format!("any: {e}")))?;
    let new_ci = ContentInfo {
        content_type: ID_SIGNED_DATA,
        content: inner_any,
    };
    new_ci
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("ContentInfo re-DER: {e}")))
}

/// Compute the SHA-256 of the SignerInfo's `signature` field — this is what
/// the TSA timestamps in a CAdES-T workflow (RFC 5126 §6.3.2). The caller
/// gives this digest as the imprint in [`build_timestamp_req`].
pub fn signer_signature_digest(cms_der: &[u8]) -> Result<[u8; 32], SignetError> {
    use sha2::Digest;
    let ci = ContentInfo::from_der(cms_der)
        .map_err(|e| SignetError::InvalidCert(format!("CMS parse: {e}")))?;
    let inner_der = ci
        .content
        .to_der()
        .map_err(|e| SignetError::InvalidCert(format!("inner re-DER: {e}")))?;
    let sd = SignedData::from_der(&inner_der)
        .map_err(|e| SignetError::InvalidCert(format!("SignedData parse: {e}")))?;
    let si = sd
        .signer_infos
        .0
        .iter()
        .next()
        .ok_or_else(|| SignetError::InvalidCert("no SignerInfo to timestamp".into()))?;
    let sig_bytes = si.signature.as_bytes();
    let mut h = sha2::Sha256::new();
    h.update(sig_bytes);
    let out = h.finalize();
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&out);
    Ok(digest)
}

// ---------------------------------------------------------------------------
// HTTP client (opt-in, off by default)
// ---------------------------------------------------------------------------

/// Content-type a conformant TSA accepts on POST (RFC 3161 §3.4).
pub const TSA_REQUEST_CONTENT_TYPE: &str = "application/timestamp-query";
/// Content-type a conformant TSA returns on success (RFC 3161 §3.4).
pub const TSA_RESPONSE_CONTENT_TYPE: &str = "application/timestamp-reply";

/// Caller-tunable HTTP knobs.
///
/// Defaults are conservative: 15s connect + 30s read timeout, no proxy, no
/// custom CA roots. The reqwest client is built fresh per call — TSA fetches
/// are rare (one per signature) so the connection-pool win isn't worth the
/// extra config plumbing.
#[derive(Debug, Clone)]
pub struct TsaFetchOptions {
    pub connect_timeout: std::time::Duration,
    pub request_timeout: std::time::Duration,
    /// Verify the response content-type matches `application/timestamp-reply`.
    /// Some self-hosted TSAs return `application/octet-stream`; users can
    /// disable the check for those.
    pub strict_content_type: bool,
}

impl Default for TsaFetchOptions {
    fn default() -> Self {
        Self {
            connect_timeout: std::time::Duration::from_secs(15),
            request_timeout: std::time::Duration::from_secs(30),
            strict_content_type: true,
        }
    }
}

/// POST a `TimeStampReq` DER blob to `tsa_url` and return the parsed
/// `TimeStampResp`.
///
/// **Network call.** Caller is responsible for honouring the user's
/// "offline" preference — `SignOptions::tsa_url == None` skips this entirely.
///
/// Errors bubble up as [`SignetError::InvalidCert`] with a "TSA fetch: …"
/// prefix so the UI can show a single clean failure row.
pub fn fetch_timestamp(
    tsa_url: &str,
    request_der: &[u8],
    opts: &TsaFetchOptions,
) -> Result<TimestampResp, SignetError> {
    if request_der.is_empty() {
        return Err(SignetError::InvalidCert(
            "TSA fetch: empty request body".into(),
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(opts.connect_timeout)
        .timeout(opts.request_timeout)
        // rustls-tls is enabled at the crate level; no extra root certs by
        // default. Users who need a private TSA CA add it to the system
        // trust store (or we expose a knob in v3.11.1).
        .user_agent(concat!("Slab/", env!("CARGO_PKG_VERSION"), " (signet_pro)"))
        .build()
        .map_err(|e| SignetError::InvalidCert(format!("TSA fetch: client build: {e}")))?;

    let resp = client
        .post(tsa_url)
        .header(reqwest::header::CONTENT_TYPE, TSA_REQUEST_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, TSA_RESPONSE_CONTENT_TYPE)
        .body(request_der.to_vec())
        .send()
        .map_err(|e| SignetError::InvalidCert(format!("TSA fetch: POST: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(SignetError::InvalidCert(format!(
            "TSA fetch: HTTP {} from {tsa_url}",
            status.as_u16()
        )));
    }

    if opts.strict_content_type {
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // Tolerate "application/timestamp-reply; charset=binary" and friends.
        if !ct.starts_with(TSA_RESPONSE_CONTENT_TYPE) {
            return Err(SignetError::InvalidCert(format!(
                "TSA fetch: unexpected Content-Type '{ct}' (expected {TSA_RESPONSE_CONTENT_TYPE})"
            )));
        }
    }

    let bytes = resp
        .bytes()
        .map_err(|e| SignetError::InvalidCert(format!("TSA fetch: read body: {e}")))?;
    parse_timestamp_resp(&bytes)
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
        assert!(parsed.cert_req);
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

    // -------------------------------------------------------------------
    // HTTP fetch tests — use mockito to stand up a fake TSA endpoint.
    // -------------------------------------------------------------------

    /// Minimal granted-response DER (status=0, no token) — same as
    /// `parse_minimal_granted_response`. Used as the fake TSA's reply.
    fn granted_resp_der() -> Vec<u8> {
        vec![0x30, 0x05, 0x30, 0x03, 0x02, 0x01, 0x00]
    }

    #[test]
    fn fetch_timestamp_round_trips_granted_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/tsa")
            .match_header("content-type", TSA_REQUEST_CONTENT_TYPE)
            .with_status(200)
            .with_header("content-type", TSA_RESPONSE_CONTENT_TYPE)
            .with_body(granted_resp_der())
            .create();

        let req = build_timestamp_req(&[0x55u8; 32], Some(123)).unwrap();
        let url = format!("{}/tsa", server.url());
        let resp = fetch_timestamp(&url, &req, &TsaFetchOptions::default()).expect("fetch ok");
        assert!(resp.status_granted());
        assert_eq!(resp.status, 0);
        mock.assert();
    }

    #[test]
    fn fetch_timestamp_errors_on_http_500() {
        let mut server = mockito::Server::new();
        let _mock = server.mock("POST", "/tsa").with_status(500).create();
        let req = build_timestamp_req(&[0u8; 32], None).unwrap();
        let url = format!("{}/tsa", server.url());
        let err = fetch_timestamp(&url, &req, &TsaFetchOptions::default())
            .expect_err("expected HTTP 500 to surface");
        let msg = format!("{err}");
        assert!(msg.contains("HTTP 500"), "msg: {msg}");
    }

    #[test]
    fn fetch_timestamp_errors_on_wrong_content_type() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/tsa")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("not a TSA response")
            .create();
        let req = build_timestamp_req(&[0u8; 32], None).unwrap();
        let url = format!("{}/tsa", server.url());
        let err = fetch_timestamp(&url, &req, &TsaFetchOptions::default())
            .expect_err("expected wrong content-type to fail");
        let msg = format!("{err}");
        assert!(msg.contains("Content-Type"), "msg: {msg}");
    }

    #[test]
    fn fetch_timestamp_tolerates_lax_content_type_when_opted_out() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/tsa")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(granted_resp_der())
            .create();
        let req = build_timestamp_req(&[0u8; 32], None).unwrap();
        let url = format!("{}/tsa", server.url());
        let opts = TsaFetchOptions {
            strict_content_type: false,
            ..Default::default()
        };
        let resp = fetch_timestamp(&url, &req, &opts).expect("fetch ok");
        assert!(resp.status_granted());
    }

    #[test]
    fn fetch_timestamp_rejects_empty_request() {
        let err = fetch_timestamp("http://127.0.0.1:1/", &[], &TsaFetchOptions::default())
            .expect_err("empty body should fail fast");
        let msg = format!("{err}");
        assert!(msg.contains("empty request body"), "msg: {msg}");
    }

    // -------------------------------------------------------------------
    // CAdES-T embedding tests — build a real CMS blob via the signet
    // crypto core, then splice a synthetic TST and verify the resulting
    // CMS still parses with an unsigned-attrs set containing our OID.
    // -------------------------------------------------------------------

    use crate::pdf::signet::cms_blob::{build_pkcs7_detached, parse_signed_data};
    use crate::pdf::signet::identity::SigningIdentity;
    use rsa::pkcs1v15::SigningKey as RsaSigningKey;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use rsa::sha2::Sha256 as RsaSha256;
    use std::str::FromStr;
    use x509_cert::builder::{Builder as _, CertificateBuilder, Profile};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;

    fn fixture_rsa_identity_local() -> SigningIdentity {
        let mut rng = rand::thread_rng();
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let key_pem = key
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .as_bytes()
            .to_vec();
        let serial = SerialNumber::from(13u32);
        let validity = Validity::from_now(std::time::Duration::from_secs(365 * 24 * 3600)).unwrap();
        let subject = Name::from_str("CN=Embed Test").unwrap();
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
        let cert_der = Encode::to_der(&cert).unwrap();
        let cert_pem = pem::encode(&pem::Pem::new("CERTIFICATE", cert_der));
        SigningIdentity::from_pem_bytes(cert_pem.as_bytes(), &key_pem, None).unwrap()
    }

    /// Hand-crafted minimal SignedData ContentInfo to serve as a fake TST —
    /// any well-formed `ContentInfo`(`id-signedData`) parses as Any.
    fn fake_tst_der() -> Vec<u8> {
        // ContentInfo SEQUENCE {
        //   contentType OID 1.2.840.113549.1.7.2 (signedData),
        //   content [0] EXPLICIT { SEQUENCE { version=3, digestAlgs=empty,
        //                                     encapContent SEQUENCE { OID 1.2.840.113549.1.7.1 },
        //                                     signerInfos=empty } }
        // }
        // We hand-roll it minimally — just enough that Any::from_der succeeds.
        // The actual CMS validity of the TST is not under test here; only the
        // splicing/re-encoding pipeline is.
        // Outer:   30 LL ...
        //   OID:   06 09 2A 86 48 86 F7 0D 01 07 02
        //   [0]:   A0 LL ...
        //     SD:  30 LL ...
        //       ver: 02 01 03
        //       digs: 31 00
        //       eci:  30 0B 06 09 2A 86 48 86 F7 0D 01 07 01
        //       sis:  31 00
        // Length math (innermost-out):
        // SD inner: 02 01 03 (3) + 31 00 (2) + 30 0B...(13) + 31 00 (2) = 20 bytes -> 30 14 ...
        // [0] inner = 30 14 ... = 22 bytes -> A0 16 ...
        // outer = OID (11 bytes total: 06 09 + 9) + [0] 24 = 35 -> 30 23 ...
        let mut out = Vec::new();
        out.extend_from_slice(&[0x30, 0x23]);
        out.extend_from_slice(&[
            0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02,
        ]);
        out.extend_from_slice(&[0xA0, 0x16]);
        out.extend_from_slice(&[0x30, 0x14]);
        out.extend_from_slice(&[0x02, 0x01, 0x03]);
        out.extend_from_slice(&[0x31, 0x00]);
        out.extend_from_slice(&[
            0x30, 0x0B, 0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01,
        ]);
        out.extend_from_slice(&[0x31, 0x00]);
        out
    }

    #[test]
    fn embed_timestamp_token_adds_unsigned_attr() {
        let id = fixture_rsa_identity_local();
        let cms = build_pkcs7_detached(&[0xAAu8; 32], &id, std::time::SystemTime::now()).unwrap();
        let tst = fake_tst_der();

        let cms_with_t = embed_timestamp_token(&cms, &tst).expect("embed");
        assert!(
            cms_with_t.len() > cms.len(),
            "CAdES-T blob should be larger than BES"
        );

        let sd = parse_signed_data(&cms_with_t).expect("parse with-T");
        let si = sd.signer_infos.0.iter().next().expect("one signer");
        let attrs = si
            .unsigned_attrs
            .as_ref()
            .expect("unsigned attrs present after embed");
        assert!(
            attrs.iter().any(|a| a.oid == ID_AA_TIMESTAMP_TOKEN),
            "id-aa-timeStampToken missing from unsigned attrs"
        );
    }

    #[test]
    fn embed_timestamp_token_is_idempotent() {
        let id = fixture_rsa_identity_local();
        let cms = build_pkcs7_detached(&[0xBBu8; 32], &id, std::time::SystemTime::now()).unwrap();
        let tst = fake_tst_der();
        let once = embed_timestamp_token(&cms, &tst).unwrap();
        let twice = embed_timestamp_token(&once, &tst).unwrap();
        // Idempotent: second embed replaces rather than duplicates the attr.
        let sd = parse_signed_data(&twice).unwrap();
        let si = sd.signer_infos.0.iter().next().unwrap();
        let attrs = si.unsigned_attrs.as_ref().unwrap();
        let count = attrs
            .iter()
            .filter(|a| a.oid == ID_AA_TIMESTAMP_TOKEN)
            .count();
        assert_eq!(count, 1, "duplicate timestamp-token attrs after re-embed");
    }

    #[test]
    fn embed_rejects_empty_tst() {
        let id = fixture_rsa_identity_local();
        let cms = build_pkcs7_detached(&[0u8; 32], &id, std::time::SystemTime::now()).unwrap();
        let err = embed_timestamp_token(&cms, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("empty TST"), "msg: {msg}");
    }

    #[test]
    fn signer_signature_digest_is_stable_per_blob() {
        let id = fixture_rsa_identity_local();
        let cms = build_pkcs7_detached(&[0xCCu8; 32], &id, std::time::SystemTime::now()).unwrap();
        let d1 = signer_signature_digest(&cms).unwrap();
        let d2 = signer_signature_digest(&cms).unwrap();
        assert_eq!(d1, d2);
        // And a digest of an unrelated blob differs.
        let cms2 = build_pkcs7_detached(&[0xDDu8; 32], &id, std::time::SystemTime::now()).unwrap();
        let d3 = signer_signature_digest(&cms2).unwrap();
        assert_ne!(d1, d3);
    }
}
