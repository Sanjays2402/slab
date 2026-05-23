//! Slab Signet — PKCS#7-detached PDF digital signatures.
//!
//! v3.10.0 entry module. See [ADR 0011](../../../../docs/adr/0011-pkcs7-signatures-rustcrypto.md)
//! for the design decision (RustCrypto over OpenSSL).
//!
//! Public surface (incremental — fleshed out across the v3.10.0 ticks):
//!
//! - [`identity`] — load a [`SigningIdentity`](identity::SigningIdentity) from
//!   on-disk PEM cert + key pair. **Ready.**
//! - [`trust`] — user-managed X.509 trust store on the filesystem.
//!   **Ready (basic chain checks).**
//! - `cms_blob` — RFC 5652 SignedData (PKCS#7-detached) builder. _(planned, Task 3.)_
//! - `sign` — embed a Sig field + ByteRange + Contents window into a PDF.
//!   _(planned, Task 4.)_
//! - `verify` — re-hash + re-validate a signed PDF. _(planned, Task 5.)_

pub mod cms_blob;
pub mod identity;
pub mod sign;
pub mod trust;

pub use cms_blob::{build_pkcs7_detached, parse_signed_data, sha256};
pub use identity::{KeyAlgorithm, SignetError, SigningIdentity};
pub use sign::{sign_pdf, SignOptions, SignReport, SIGNATURE_HEX_PLACEHOLDER_BYTES};
pub use trust::{ChainStatus, TrustStore};
