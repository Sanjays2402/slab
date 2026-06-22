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
//! - [`fetch`] — HTTP client + offline cache (Slice 3)
//! - [`install`] — download, sha256 verify, atomic extract (Slice 4)
//! - [`install_log`] — append-only sqlite history of install / update /
//!   uninstall / failed events (v3.39 Slice 53)

pub mod fetch;
pub mod index;
pub mod install;
pub mod install_log;
pub mod update_plan;
pub mod verify;

pub use fetch::{
    default_cache_path, default_client, fetch_index, fetch_index_with_cache, parse_index,
    FetchError, FetchOutcome, CACHE_FILE_NAME, DEFAULT_INDEX_URL,
};
pub use index::{Index, IndexEntry, IndexEntryUnsigned, CURRENT_SCHEMA_VERSION, MAX_TARBALL_BYTES};
pub use install::{
    install_from_bytes, install_from_entry, uninstall_plugin, InstallError, InstallReport,
    MAX_UNCOMPRESSED_BYTES,
};
pub use install_log::{
    default_log_path, ActivityBucket, AutoPruneOutcome, InstallAction, InstallEvent, InstallLog,
    InstallLogError, InstallStats, PluginHistogramRow, TimeBucketGranularity,
    AUTO_PRUNE_INTERVAL_SECS, DEFAULT_RETAIN_DAYS, MIN_RETAIN_DAYS,
};
pub use update_plan::{plan_updates, InstalledPlugin, UpdatePlan, UpdateTarget};
pub use verify::{
    verify_entry, verify_with_maintainer_key, VerifyError, MAINTAINER_KEY_ID, MAINTAINER_PUBLIC_KEY,
};
