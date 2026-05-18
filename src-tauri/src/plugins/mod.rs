//! Foundry (v1.3.0) — declarative plugin system.
//!
//! Plugins live at `~/.slab/plugins/<name>/plugin.toml` and contribute
//! themes, locales, PDF actions, custom commands, and AI providers via
//! a strict TOML manifest. See `manifest.rs` for the schema,
//! `registry.rs` for discovery + enabled-state, and `contributions.rs`
//! for the active-list views that feature code consumes.

pub mod command_runner;
pub mod contributions;
pub mod locale_loader;
pub mod manifest;
pub mod registry;
pub mod runner;

pub use command_runner::{
    run_command, CommandError, CommandOutcome, CommandStatus, ShellReport,
    DEFAULT_COMMAND_TIMEOUT_MS,
};
pub use contributions::{
    active_ai_providers, active_commands, active_locales, active_pdf_actions, active_themes,
    read_asset, ActiveAiProvider, ActiveCommand, ActiveLocale, ActivePdfAction, ActiveTheme,
};
pub use locale_loader::load_locale_bundle;
pub use manifest::{
    AiProviderContribution, CommandContribution, Contributions, LocaleContribution, Manifest,
    ManifestError, PdfActionContribution, Permission, ThemeContribution,
};
pub use registry::{
    default_plugins_root, default_state_path, read_enabled_state, write_enabled_state,
    EnabledState, Plugin, PluginRegistry,
};
pub use runner::{run_pdf_action, ActionError, ActionReport, ActionStatus};
