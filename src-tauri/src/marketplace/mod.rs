//! Bench (v1.4.0) — Slab plugin marketplace.
//!
//! Fetches a curated `index.json` of plugins from
//! `https://raw.githubusercontent.com/Sanjays2402/slab-plugins/main/index.json`,
//! verifies each entry against the Slab maintainer's Ed25519 public
//! key (hard-coded), and lets the user one-click install a plugin to
//! `~/.slab/plugins/<id>/`. Reuses Foundry's plugin registry — once
//! a tarball is extracted, the existing discovery loop picks it up.
//!
//! Module layout:
//! - [`index`] — JSON schema (`Index`, `IndexEntry`)
//! - [`verify`] — Ed25519 signature verification
//! - `fetch` (Slice 3) — HTTP client + offline cache
//! - `install` (Slice 4) — download, sha256 verify, atomic extract

pub mod index;
pub mod verify;

pub use index::{Index, IndexEntry, IndexEntryUnsigned, CURRENT_SCHEMA_VERSION, MAX_TARBALL_BYTES};
pub use verify::{
    verify_entry, verify_with_maintainer_key, VerifyError, MAINTAINER_KEY_ID, MAINTAINER_PUBLIC_KEY,
};
