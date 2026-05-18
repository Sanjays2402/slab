//! Foundry (v1.3.0) — declarative plugin system.
//!
//! Plugins live at `~/.slab/plugins/<name>/plugin.toml` and contribute
//! themes, locales, PDF actions, custom commands, and AI providers via
//! a strict TOML manifest. See `manifest.rs` for the schema and
//! `registry.rs` for discovery + enabled-state persistence.

pub mod manifest;
pub mod registry;

pub use manifest::{
    AiProviderContribution, CommandContribution, Contributions, LocaleContribution, Manifest,
    ManifestError, PdfActionContribution, Permission, ThemeContribution,
};
pub use registry::{
    default_plugins_root, default_state_path, read_enabled_state, write_enabled_state,
    EnabledState, Plugin, PluginRegistry,
};
