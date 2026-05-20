//! Workshop (v2.0.0 Slice 4) — host-side state captured during plugin
//! script evaluation.
//!
//! When a plugin's `script.js` runs in the sandboxed QuickJS context,
//! it can call into a small set of `slab.*` host functions to register
//! tools, panels, AI providers, etc. Those calls don't *do* the work
//! themselves; they accumulate state into a [`Registrations`] struct
//! that the host reads back after `Runtime::enable_plugin` returns.
//!
//! This separation is deliberate. Plugins evaluate top-level once at
//! enable time, and the side effects of that evaluation are pure
//! description — "I would like to register a tool named foo". The
//! host then decides whether to actually wire that registration into
//! Beacon, the Cabinet UI, etc.
//!
//! ## Why JSON values?
//!
//! Each registration carries a `serde_json::Value` blob from the
//! plugin side. We could nail the schema down in Rust, but plugin
//! authors will iterate fast and the host can validate cheaply when
//! it consumes the registration. Stringly-typed at the FFI boundary,
//! strongly-typed at the consumer.
//!
//! ## Slice 4 scope
//!
//! - Tool registrations (Beacon)
//! - AI provider registrations (Beacon)
//! - Panel registrations (Cabinet)
//! - Tool registrations (Cabinet UI surface)
//! - `console.*` logs (already shipped in Slice 1)
//! - Notifications via `slab.ui.notify(message, level?)`
//!
//! Slice 5+ will wire these into Beacon / the panel renderer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Severity tier for `slab.ui.notify(message, level?)` calls. Mirrors
/// the frontend toast-system tiers — `info` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyLevel {
    /// Informational, no user action required. Default level when a
    /// plugin omits the second argument to `slab.ui.notify`.
    #[default]
    Info,
    /// Soft warning; doesn't block the user.
    Warn,
    /// Hard failure / actionable error from the plugin.
    Error,
}

impl NotifyLevel {
    /// Parse from the optional second arg of `slab.ui.notify`. Unknown
    /// strings degrade to `Info` rather than throwing — plugins should
    /// be able to evolve the level vocabulary without breaking older
    /// hosts.
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "warn" | "warning" => Self::Warn,
            "error" | "err" => Self::Error,
            _ => Self::Info,
        }
    }
}

/// One `slab.beacon.registerTool({...})` call from a plugin script.
/// The plugin-supplied JSON descriptor (id, description, schema, etc.)
/// is stashed verbatim — the Beacon consumer validates it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeaconToolReg {
    /// Plugin ID that owns this registration. Stamped by the host so
    /// downstream consumers can route + audit without parsing the
    /// blob.
    pub plugin_id: String,
    /// Raw descriptor as JSON. Expected shape (validated later):
    /// `{ id: string, name?: string, description?: string,
    ///    parameters?: object, ... }`.
    pub descriptor: Value,
}

/// One `slab.beacon.registerAiProvider({...})` call from a plugin
/// script. Closes the long-parked v1.3.x AI-provider TODO once the
/// Beacon consumer wires this through.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeaconAiProviderReg {
    pub plugin_id: String,
    pub descriptor: Value,
}

/// One `slab.ui.registerPanel({...})` call. The descriptor carries the
/// panel ID + display name + (eventually) a declarative DOM-fragment
/// spec the renderer walks; Slice 6 implements the renderer side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiPanelReg {
    pub plugin_id: String,
    pub descriptor: Value,
}

/// One `slab.ui.registerTool({...})` call — a custom toolbar/quick-
/// action contributed by the plugin (distinct from a Beacon tool).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiToolReg {
    pub plugin_id: String,
    pub descriptor: Value,
}

/// One `slab.ui.notify(...)` call during plugin enable. Plugins often
/// notify on first enable ("plugin loaded — try the X command"), so
/// we capture these so the host can replay them through the toast
/// system after enable completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotifyCall {
    pub plugin_id: String,
    pub message: String,
    pub level: NotifyLevel,
}

/// Everything a plugin registered during its enable-time evaluation.
/// Default is "empty" — a plugin that does nothing at top-level
/// produces a default `Registrations`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Registrations {
    #[serde(default)]
    pub beacon_tools: Vec<BeaconToolReg>,
    #[serde(default)]
    pub beacon_ai_providers: Vec<BeaconAiProviderReg>,
    #[serde(default)]
    pub ui_panels: Vec<UiPanelReg>,
    #[serde(default)]
    pub ui_tools: Vec<UiToolReg>,
    #[serde(default)]
    pub notifications: Vec<NotifyCall>,
}

impl Registrations {
    /// Convenience: did the plugin register *anything*? Useful for
    /// the Cabinet to decide whether to surface an "active plugin"
    /// badge vs. a "loaded but inert" state.
    pub fn is_empty(&self) -> bool {
        self.beacon_tools.is_empty()
            && self.beacon_ai_providers.is_empty()
            && self.ui_panels.is_empty()
            && self.ui_tools.is_empty()
            && self.notifications.is_empty()
    }

    /// Total registration count across all categories. Used for the
    /// per-plugin diagnostics line ("foo registered 3 things").
    pub fn total(&self) -> usize {
        self.beacon_tools.len()
            + self.beacon_ai_providers.len()
            + self.ui_panels.len()
            + self.ui_tools.len()
            + self.notifications.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registrations_is_empty() {
        let r = Registrations::default();
        assert!(r.is_empty());
        assert_eq!(r.total(), 0);
    }

    #[test]
    fn notify_level_defaults_to_info() {
        assert_eq!(NotifyLevel::default(), NotifyLevel::Info);
    }

    #[test]
    fn notify_level_parses_known_variants() {
        assert_eq!(NotifyLevel::from_str_loose("info"), NotifyLevel::Info);
        assert_eq!(NotifyLevel::from_str_loose("warn"), NotifyLevel::Warn);
        assert_eq!(NotifyLevel::from_str_loose("warning"), NotifyLevel::Warn);
        assert_eq!(NotifyLevel::from_str_loose("error"), NotifyLevel::Error);
        assert_eq!(NotifyLevel::from_str_loose("ERR"), NotifyLevel::Error);
    }

    #[test]
    fn notify_level_unknown_degrades_to_info() {
        assert_eq!(NotifyLevel::from_str_loose("debug"), NotifyLevel::Info);
        assert_eq!(NotifyLevel::from_str_loose(""), NotifyLevel::Info);
        assert_eq!(NotifyLevel::from_str_loose("FATAL"), NotifyLevel::Info);
    }

    #[test]
    fn total_counts_across_all_categories() {
        let r = Registrations {
            beacon_tools: vec![BeaconToolReg {
                plugin_id: "p".into(),
                descriptor: Value::Null,
            }],
            ui_panels: vec![
                UiPanelReg {
                    plugin_id: "p".into(),
                    descriptor: Value::Null,
                },
                UiPanelReg {
                    plugin_id: "p".into(),
                    descriptor: Value::Null,
                },
            ],
            notifications: vec![NotifyCall {
                plugin_id: "p".into(),
                message: "hi".into(),
                level: NotifyLevel::Info,
            }],
            ..Registrations::default()
        };
        assert!(!r.is_empty());
        assert_eq!(r.total(), 4);
    }
}
