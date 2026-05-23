//! RFC 3161 timestamp-authority client.
//!
//! Network calls are opt-in via `SignOptions::tsa_url` (default `None`).
//! When unset, signatures are CAdES-BES grade (no timestamp); when set,
//! signatures are upgraded to CAdES-T by embedding the returned TST as the
//! `id-aa-timeStampToken` (OID `1.2.840.113549.1.9.16.2.14`) unsigned
//! attribute in the CMS SignerInfo.

#![allow(dead_code)] // Scaffolding — fleshed out in Task 2/3 of the v3.11.0 plan.

use crate::pdf::signet::SignetError;

/// Build an RFC 3161 `TimeStampReq` for the given SHA-256 digest.
///
/// _Stub — implemented in Task 2 of the v3.11.0 plan._
pub fn build_timestamp_req(
    _digest: &[u8; 32],
    _nonce: Option<i64>,
) -> Result<Vec<u8>, SignetError> {
    Err(SignetError::InvalidCert(
        "tsa::build_timestamp_req not yet implemented".into(),
    ))
}

/// Parse an RFC 3161 `TimeStampResp` returned by a TSA.
///
/// _Stub — implemented in Task 3 of the v3.11.0 plan._
pub fn parse_timestamp_resp(_bytes: &[u8]) -> Result<TimestampResp, SignetError> {
    Err(SignetError::InvalidCert(
        "tsa::parse_timestamp_resp not yet implemented".into(),
    ))
}

/// Parsed RFC 3161 response.
#[derive(Debug, Clone)]
pub struct TimestampResp {
    pub status: u32,
    pub token: Vec<u8>,
}

impl TimestampResp {
    pub fn status_granted(&self) -> bool {
        self.status == 0 || self.status == 1
    }
}
