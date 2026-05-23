//! Slab Signet — PKCS#7-detached PDF digital signatures.
//!
//! v3.10.0 entry module. See [ADR 0011](../../../../docs/adr/0011-pkcs7-signatures-rustcrypto.md)
//! for the design decision (RustCrypto over OpenSSL).
//!
//! Public surface (incremental — fleshed out across the v3.10.0 ticks):
//!
//! - [`identity`] — load a [`SigningIdentity`](identity::SigningIdentity) from
//!   on-disk PKCS#12 or a PEM cert+key pair. **Ready.**
//! - `cms_blob` — RFC 5652 SignedData (PKCS#7-detached) builder. _(planned, Task 3.)_
//! - `sign` — embed a Sig field + ByteRange + Contents window into a PDF.
//!   _(planned, Task 4.)_
//! - `verify` — re-hash + re-validate a signed PDF. _(planned, Task 5.)_
//! - `trust` — user-managed X.509 trust store. _(planned, Task 5.)_

pub mod identity;

pub use identity::{KeyAlgorithm, SignetError, SigningIdentity};
