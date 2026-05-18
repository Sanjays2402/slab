//! Foundry (v1.3.0) — declarative plugin system.
//!
//! Plugins live at `~/.slab/plugins/<name>/plugin.toml` and contribute
//! themes, locales, PDF actions, custom commands, and AI providers via
//! a strict TOML manifest. See `manifest.rs` for the schema.

pub mod manifest;

pub use manifest::{
    AiProviderContribution, CommandContribution, Contributions, LocaleContribution, Manifest,
    ManifestError, PdfActionContribution, Permission, ThemeContribution,
};
