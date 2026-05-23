//! Slab Signet Pro — enterprise extensions to v3.10.0 Signet.
//!
//! v3.11.0 adds three Acrobat-Pro-parity capabilities on top of the v3.10.0
//! PKCS#7-detached signing core:
//!
//! - [`tsa`] — RFC 3161 timestamp-authority client. Produces a TimeStampReq,
//!   POSTs it (opt-in, default-offline), parses the TimeStampResp, and embeds
//!   the resulting timestamp token (TST) as the `id-aa-timeStampToken`
//!   unsigned attribute inside the CMS SignerInfo. This upgrades signatures
//!   from CAdES-BES to CAdES-T grade, which is what courts and long-term
//!   archives require.
//!
//! - [`appearance`] — visible signature appearances. Builds a Form XObject
//!   (AP/N stream) rendered into a Widget annotation so the signature shows
//!   up as a visible stamp in Acrobat Reader / Preview / Foxit, including
//!   signer common name, signing time, reason, and location.
//!
//! - [`batch`] — sign every `*.pdf` in a folder in parallel via rayon, with
//!   per-file progress events surfaced to the Tauri frontend. The headline
//!   buyer-facing demo: 50 contracts signed in ~8 seconds, all offline.
//!
//! See [ADR 0012](../../../../docs/adr/0012-signet-pro-tsa-batch.md) for the
//! design rationale.

pub mod appearance;
pub mod batch;
pub mod tsa;
