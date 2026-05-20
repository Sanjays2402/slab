//! Foundry (v1.3.0) — declarative plugin system.
//!
//! Plugins live at `~/.slab/plugins/<name>/plugin.toml` and contribute
//! themes, locales, PDF actions, custom commands, and AI providers via
//! a strict TOML manifest. See `manifest.rs` for the schema,
//! `registry.rs` for discovery + enabled-state, and `contributions.rs`
//! for the active-list views that feature code consumes.

pub mod ai_materialize;
pub mod bundled;
pub mod command_runner;
pub mod contributions;
pub mod grants;
pub mod locale_loader;
pub mod manifest;
pub mod registry;
pub mod runner;
pub mod runtime;
pub mod runtime_registry;
pub mod storage;

pub use ai_materialize::{materialize_active, materialize_contribution};
pub use bundled::seed_bundled_plugins;
pub use command_runner::{
    run_command, CommandError, CommandOutcome, CommandStatus, ShellReport,
    DEFAULT_COMMAND_TIMEOUT_MS,
};
pub use contributions::{
    active_ai_providers, active_commands, active_locales, active_pdf_actions, active_themes,
    read_asset, ActiveAiProvider, ActiveCommand, ActiveLocale, ActivePdfAction, ActiveTheme,
};
pub use grants::{
    default_grants_path, enforce, read_grants, write_grants, CapabilityRequest, DenyReason,
    GrantStore, PluginGrants,
};
pub use locale_loader::load_locale_bundle;
pub use manifest::{
    AiProviderContribution, BeaconCap, Capabilities, CommandContribution, Contributions, FsCap,
    LocaleContribution, Manifest, ManifestError, NetCap, PdfActionContribution, Permission,
    RuntimeManifest, ThemeContribution, UiCap,
};
pub use registry::{
    default_plugins_root, default_state_path, read_enabled_state, write_enabled_state,
    EnabledState, Plugin, PluginRegistry,
};
pub use runner::{run_pdf_action, ActionError, ActionReport, ActionStatus};
pub use runtime::{
    BeaconAiProviderReg, BeaconToolReg, EnableOutput, NotifyCall, NotifyLevel, Registrations,
    Runtime, RuntimeError, ScriptOutput, UiPanelReg, UiToolReg,
};
pub use runtime_registry::{LiveEntry, PluginRuntimeRegistry};
pub use storage::{
    default_db_path as default_plugin_storage_path, shared_storage, PluginStorage,
    SharedPluginStorage, StorageError, MAX_KEY_BYTES, MAX_PLUGIN_BYTES, MAX_VALUE_BYTES,
};
