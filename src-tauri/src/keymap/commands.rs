//! DTOs + business logic for the Tauri command surface. `lib.rs` exposes
//! thin `#[tauri::command]` wrappers that load → mutate → save
//! `SlabConfig` and translate `KeymapApplyError` into `CmdResult`.

use super::{action::all_actions, ActionId, Binding, KeymapConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct KeymapView {
    pub actions: Vec<KeymapAction>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeymapAction {
    pub id: String,
    pub label: String,
    pub group: String,
    pub binding: String,
    pub default_binding: String,
    pub is_override: bool,
}

pub fn build_view(cfg: &KeymapConfig) -> KeymapView {
    let materialised = cfg.materialise();
    // `all_actions()` is in canonical registration order; we just project
    // it. That preserves grouping (Global → Tabs → Reading → Beacon)
    // because we declare them in that order.
    let actions: Vec<KeymapAction> = all_actions()
        .iter()
        .map(|info| {
            let binding = materialised[&info.id].to_string();
            KeymapAction {
                id: info.key.to_string(),
                label: info.label.to_string(),
                group: info.group.to_string(),
                binding,
                default_binding: info.default_binding.to_string(),
                is_override: cfg.get(info.id).is_some(),
            }
        })
        .collect();
    KeymapView {
        actions,
        is_default: cfg.is_default(),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeymapWriteArgs {
    pub overrides: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeymapApplyError {
    #[error("unknown action id: {id}")]
    UnknownAction { id: String },
    #[error("invalid binding `{binding}` for {id}: {reason}")]
    InvalidBinding {
        id: String,
        binding: String,
        reason: String,
    },
    #[error("binding `{binding}` is bound to multiple actions: {ids:?}")]
    Conflict { binding: String, ids: Vec<String> },
}

/// Apply the user's proposed remap to `cfg` *in place*. On error, `cfg`
/// is left untouched (we mutate a clone first and only commit on success).
pub fn apply_overrides(
    cfg: &mut KeymapConfig,
    pairs: Vec<(String, String)>,
) -> Result<(), KeymapApplyError> {
    // 1. Parse all entries.
    let mut proposed: HashMap<ActionId, Binding> = HashMap::new();
    for (id_s, binding_s) in &pairs {
        let id = ActionId::parse(id_s)
            .map_err(|_| KeymapApplyError::UnknownAction { id: id_s.clone() })?;
        let b = Binding::parse(binding_s).map_err(|e| KeymapApplyError::InvalidBinding {
            id: id_s.clone(),
            binding: binding_s.clone(),
            reason: e.to_string(),
        })?;
        proposed.insert(id, b);
    }

    // 2. Compute the materialised map *as if* we applied the overrides,
    //    so we can collision-check against actions the user didn't touch.
    let mut trial = cfg.clone();
    for (id, b) in &proposed {
        trial.set(*id, b.clone());
    }
    let materialised = trial.materialise();

    // 3. Group action ids by resulting binding string; any binding bound
    //    to >1 action that the user *touched* is a conflict.
    let mut by_binding: HashMap<String, Vec<String>> = HashMap::new();
    for (id, b) in &materialised {
        by_binding
            .entry(b.to_string())
            .or_default()
            .push(id.as_str().to_string());
    }
    for (binding, ids) in &by_binding {
        if ids.len() > 1 {
            let touched_by_user = proposed.values().any(|b| b.to_string() == *binding);
            if touched_by_user {
                let mut ids_sorted = ids.clone();
                ids_sorted.sort();
                return Err(KeymapApplyError::Conflict {
                    binding: binding.clone(),
                    ids: ids_sorted,
                });
            }
        }
    }

    // 4. Prune entries equal to default — keeps the on-disk file tidy.
    let defaults = super::action::default_keymap();
    for (id, b) in proposed {
        if defaults.get(&id) == Some(&b) {
            cfg.reset(id);
        } else {
            cfg.set(id, b);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_includes_every_action() {
        let cfg = KeymapConfig::default();
        let v = build_view(&cfg);
        assert_eq!(v.actions.len(), all_actions().len());
        assert!(v.is_default);
    }

    #[test]
    fn view_marks_overridden_actions() {
        let mut cfg = KeymapConfig::default();
        cfg.set(ActionId::PaletteOpen, Binding::parse("Mod+P").unwrap());
        let v = build_view(&cfg);
        let row = v.actions.iter().find(|a| a.id == "palette.open").unwrap();
        assert_eq!(row.binding, "Mod+P");
        assert_eq!(row.default_binding, "Mod+K");
        assert!(row.is_override);
        let other = v.actions.iter().find(|a| a.id == "shortcuts.show").unwrap();
        assert!(!other.is_override);
        assert!(!v.is_default);
    }

    #[test]
    fn apply_overrides_validates_each_entry() {
        let mut cfg = KeymapConfig::default();
        let bad = vec![("palette.open".to_string(), "Hyper+K".to_string())];
        let r = apply_overrides(&mut cfg, bad);
        assert!(matches!(r, Err(KeymapApplyError::InvalidBinding { .. })));
        // cfg untouched on error.
        assert!(cfg.is_default());
    }

    #[test]
    fn apply_overrides_validates_action_id() {
        let mut cfg = KeymapConfig::default();
        let bad = vec![("not.real".to_string(), "Mod+K".to_string())];
        let r = apply_overrides(&mut cfg, bad);
        assert!(matches!(r, Err(KeymapApplyError::UnknownAction { .. })));
    }

    #[test]
    fn apply_overrides_detects_collisions() {
        // Two distinct actions can't both bind to Mod+J.
        let mut cfg = KeymapConfig::default();
        let pairs = vec![
            ("palette.open".to_string(), "Mod+J".to_string()),
            ("tabs.new".to_string(), "Mod+J".to_string()),
        ];
        let r = apply_overrides(&mut cfg, pairs);
        match r {
            Err(KeymapApplyError::Conflict { binding, ids }) => {
                assert_eq!(binding, "Mod+J");
                assert_eq!(ids.len(), 2);
                assert!(ids.contains(&"palette.open".to_string()));
                assert!(ids.contains(&"tabs.new".to_string()));
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn apply_overrides_against_default_keeps_implicit_defaults_unique() {
        // Setting palette.open to "?" should collide with the default
        // binding of shortcuts.show, even though the user didn't touch
        // shortcuts.show.
        let mut cfg = KeymapConfig::default();
        let pairs = vec![("palette.open".to_string(), "?".to_string())];
        let r = apply_overrides(&mut cfg, pairs);
        match r {
            Err(KeymapApplyError::Conflict { binding, ids }) => {
                assert_eq!(binding, "?");
                assert!(ids.iter().any(|s| s == "palette.open"));
                assert!(ids.iter().any(|s| s == "shortcuts.show"));
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn apply_overrides_drops_overrides_equal_to_default() {
        let mut cfg = KeymapConfig::default();
        cfg.set(ActionId::PaletteOpen, Binding::parse("Mod+P").unwrap());
        assert!(!cfg.is_default());
        let pairs = vec![("palette.open".to_string(), "Mod+K".to_string())];
        apply_overrides(&mut cfg, pairs).unwrap();
        // Override pruned because it matches the default.
        assert!(cfg.get(ActionId::PaletteOpen).is_none());
        assert!(cfg.is_default());
    }

    #[test]
    fn apply_overrides_happy_path_writes_through() {
        let mut cfg = KeymapConfig::default();
        let pairs = vec![("tabs.close".to_string(), "Mod+Shift+W".to_string())];
        apply_overrides(&mut cfg, pairs).unwrap();
        assert_eq!(
            cfg.get(ActionId::TabsClose).unwrap().to_string(),
            "Mod+Shift+W"
        );
    }
}
