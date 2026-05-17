//! Customizable keyboard shortcuts. Backend half — owns the binding
//! parser, the canonical list of actions, the user's persisted keymap,
//! and the serde glue that drops `[keymap]` into `~/.slab/config.toml`.
//!
//! Frontend half lives at `src/lib/keymap.ts` (Svelte store + matcher).

pub mod action;
pub mod binding;
pub mod commands;

pub use action::{all_actions, default_keymap, ActionId, ActionInfo, UnknownAction};
pub use binding::{Binding, KeyEvent, Modifier, ModifierSet, ParseError, Platform};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// User-visible keymap config. Persisted as `[keymap] action.id = "Mod+K"`
/// in `~/.slab/config.toml`. Missing entries fall back to `default_keymap()`
/// at materialise time — that lets new releases ship new actions without
/// needing a config migration.
///
/// Unknown action keys in the on-disk TOML are dropped silently (forward-
/// compat with future builds that introduce new actions).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeymapConfig {
    overrides: HashMap<ActionId, Binding>,
}

impl KeymapConfig {
    pub fn get(&self, id: ActionId) -> Option<&Binding> {
        self.overrides.get(&id)
    }
    pub fn set(&mut self, id: ActionId, b: Binding) {
        self.overrides.insert(id, b);
    }
    pub fn reset(&mut self, id: ActionId) {
        self.overrides.remove(&id);
    }
    pub fn clear_all(&mut self) {
        self.overrides.clear();
    }
    pub fn is_default(&self) -> bool {
        self.overrides.is_empty()
    }
    /// Defaults merged with user overrides. The result is what the
    /// frontend should consult at runtime.
    pub fn materialise(&self) -> HashMap<ActionId, Binding> {
        let mut out = default_keymap();
        for (id, b) in &self.overrides {
            out.insert(*id, b.clone());
        }
        out
    }
}

impl Serialize for KeymapConfig {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        // Sort the keys so the on-disk file diff stays stable.
        let mut entries: Vec<(&'static str, String)> = self
            .overrides
            .iter()
            .map(|(id, b)| (id.as_str(), b.to_string()))
            .collect();
        entries.sort_by_key(|(k, _)| *k);
        let mut m = s.serialize_map(Some(entries.len()))?;
        for (k, v) in entries {
            m.serialize_entry(k, &v)?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for KeymapConfig {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw: HashMap<String, String> = HashMap::deserialize(d)?;
        let mut overrides = HashMap::with_capacity(raw.len());
        for (k, v) in raw {
            // Unknown actions are dropped silently (forward-compat).
            let Ok(id) = ActionId::parse(&k) else {
                continue;
            };
            // Invalid bindings DO error — a typo deserves a loud message.
            let b = Binding::parse(&v).map_err(serde::de::Error::custom)?;
            overrides.insert(id, b);
        }
        Ok(KeymapConfig { overrides })
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn empty_config_yields_defaults() {
        let cfg = KeymapConfig::default();
        let materialised = cfg.materialise();
        assert_eq!(materialised, default_keymap());
    }

    #[test]
    fn user_override_takes_precedence() {
        let mut cfg = KeymapConfig::default();
        cfg.set(ActionId::PaletteOpen, Binding::parse("Mod+P").unwrap());
        let m = cfg.materialise();
        assert_eq!(m[&ActionId::PaletteOpen].to_string(), "Mod+P");
        // Other actions still default.
        assert_eq!(m[&ActionId::ShortcutsShow].to_string(), "?");
    }

    #[test]
    fn toml_round_trip() {
        // Wrap in a small struct so we exercise the `[keymap]` parent.
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Wrapper {
            keymap: KeymapConfig,
        }
        let mut cfg = KeymapConfig::default();
        cfg.set(ActionId::TabsClose, Binding::parse("Mod+Shift+W").unwrap());
        let w = Wrapper { keymap: cfg };
        let s = toml::to_string_pretty(&w).unwrap();
        assert!(s.contains("[keymap]"));
        assert!(
            s.contains("\"tabs.close\" = \"Mod+Shift+W\"")
                || s.contains("tabs.close = \"Mod+Shift+W\"")
        );
        let parsed: Wrapper = toml::from_str(&s).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn invalid_binding_in_toml_errors() {
        let bad = "[keymap]\n\"palette.open\" = \"Hyper+K\"\n";
        let r: Result<KeymapConfig, _> = toml::from_str(bad)
            .map(|w: toml::Table| {
                // Pull the [keymap] subtree and parse it as KeymapConfig.
                let sub = w
                    .get("keymap")
                    .cloned()
                    .unwrap_or(toml::Value::Table(toml::Table::new()));
                sub.try_into::<KeymapConfig>().map_err(|e| e.to_string())
            })
            .unwrap_or_else(|e| Err(e.to_string()));
        assert!(r.is_err());
    }

    #[test]
    fn unknown_action_in_toml_is_ignored_not_fatal() {
        // Forward-compat: a future Slab release adds an action; an older
        // build reading the same config shouldn't crash. Unknown keys
        // are silently dropped.
        let s = "[keymap]\n\"palette.open\" = \"Mod+P\"\n\"future.action\" = \"Mod+Z\"\n";
        let parent: toml::Table = toml::from_str(s).unwrap();
        let sub = parent.get("keymap").unwrap().clone();
        let cfg: KeymapConfig = sub.try_into().unwrap();
        assert_eq!(cfg.get(ActionId::PaletteOpen).unwrap().to_string(), "Mod+P");
    }

    #[test]
    fn slab_config_carries_keymap() {
        let toml_in = "
            [beacon]
            provider = \"ollama\"

            [keymap]
            \"palette.open\" = \"Mod+J\"
        ";
        let cfg: crate::ai::config::SlabConfig = toml::from_str(toml_in).unwrap();
        let m = cfg.keymap.materialise();
        assert_eq!(m[&ActionId::PaletteOpen].to_string(), "Mod+J");
    }

    #[test]
    fn reset_action_removes_override() {
        let mut cfg = KeymapConfig::default();
        cfg.set(ActionId::PaletteOpen, Binding::parse("Mod+P").unwrap());
        cfg.reset(ActionId::PaletteOpen);
        assert_eq!(
            cfg.materialise()[&ActionId::PaletteOpen].to_string(),
            "Mod+K"
        );
    }

    #[test]
    fn clear_all_drops_overrides() {
        let mut cfg = KeymapConfig::default();
        cfg.set(ActionId::PaletteOpen, Binding::parse("Mod+P").unwrap());
        cfg.set(ActionId::TabsClose, Binding::parse("Mod+Shift+W").unwrap());
        assert!(!cfg.is_default());
        cfg.clear_all();
        assert!(cfg.is_default());
    }
}
