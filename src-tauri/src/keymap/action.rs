//! Canonical list of bindable actions. Adding a new shortcut means:
//!   1. Adding a variant to `ActionId`.
//!   2. Adding `(variant, "namespace.id", "Human label", "Group", "Default")`
//!      to the ACTIONS table below.
//!   3. Wiring the frontend to call `matches(event, "namespace.id")`.

use super::Binding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionId {
    PaletteOpen,
    ShortcutsShow,
    TabsNew,
    TabsClose,
    TabsNext,
    TabsPrev,
    TabsGoto1,
    TabsGoto2,
    TabsGoto3,
    TabsGoto4,
    TabsGoto5,
    TabsGoto6,
    TabsGoto7,
    TabsGoto8,
    TabsGoto9,
    FindOpen,
    ZoomIn,
    ZoomOut,
    BeaconSend,
    LibrarySearch,
    TheaterStart,
    TheaterNext,
    TheaterPrev,
    TheaterToggleBlackout,
    TheaterToggleInk,
    TheaterExit,
    BedrockOpen,
    PressOpen,
    FormsOpen,
    QuillBatchOpen,
    QuillDesignerOpen,
    QuillAutodetectOpen,
    QuillTour,
    AtelierOpen,
    HopperOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionInfo {
    pub id: ActionId,
    /// Stable wire-format id used on the JS side and in the TOML map.
    /// `"namespace.id"`. Lowercase, dotted, ASCII, never changes.
    pub key: &'static str,
    /// Human-readable label for the Settings UI.
    pub label: &'static str,
    /// Section heading shown in the Settings UI.
    pub group: &'static str,
    /// Default binding (must be `Binding::parse`-able).
    pub default_binding: &'static str,
}

const ACTIONS: &[ActionInfo] = &[
    ActionInfo {
        id: ActionId::PaletteOpen,
        key: "palette.open",
        label: "Open command palette",
        group: "Global",
        default_binding: "Mod+K",
    },
    ActionInfo {
        id: ActionId::ShortcutsShow,
        key: "shortcuts.show",
        label: "Show keyboard shortcuts",
        group: "Global",
        default_binding: "?",
    },
    ActionInfo {
        id: ActionId::TabsNew,
        key: "tabs.new",
        label: "Open PDF in new tab",
        group: "Tabs",
        default_binding: "Mod+T",
    },
    ActionInfo {
        id: ActionId::TabsClose,
        key: "tabs.close",
        label: "Close current tab",
        group: "Tabs",
        default_binding: "Mod+W",
    },
    ActionInfo {
        id: ActionId::TabsNext,
        key: "tabs.next",
        label: "Next tab",
        group: "Tabs",
        default_binding: "Ctrl+Tab",
    },
    ActionInfo {
        id: ActionId::TabsPrev,
        key: "tabs.prev",
        label: "Previous tab",
        group: "Tabs",
        default_binding: "Ctrl+Shift+Tab",
    },
    ActionInfo {
        id: ActionId::TabsGoto1,
        key: "tabs.goto1",
        label: "Jump to tab 1",
        group: "Tabs",
        default_binding: "Mod+1",
    },
    ActionInfo {
        id: ActionId::TabsGoto2,
        key: "tabs.goto2",
        label: "Jump to tab 2",
        group: "Tabs",
        default_binding: "Mod+2",
    },
    ActionInfo {
        id: ActionId::TabsGoto3,
        key: "tabs.goto3",
        label: "Jump to tab 3",
        group: "Tabs",
        default_binding: "Mod+3",
    },
    ActionInfo {
        id: ActionId::TabsGoto4,
        key: "tabs.goto4",
        label: "Jump to tab 4",
        group: "Tabs",
        default_binding: "Mod+4",
    },
    ActionInfo {
        id: ActionId::TabsGoto5,
        key: "tabs.goto5",
        label: "Jump to tab 5",
        group: "Tabs",
        default_binding: "Mod+5",
    },
    ActionInfo {
        id: ActionId::TabsGoto6,
        key: "tabs.goto6",
        label: "Jump to tab 6",
        group: "Tabs",
        default_binding: "Mod+6",
    },
    ActionInfo {
        id: ActionId::TabsGoto7,
        key: "tabs.goto7",
        label: "Jump to tab 7",
        group: "Tabs",
        default_binding: "Mod+7",
    },
    ActionInfo {
        id: ActionId::TabsGoto8,
        key: "tabs.goto8",
        label: "Jump to tab 8",
        group: "Tabs",
        default_binding: "Mod+8",
    },
    ActionInfo {
        id: ActionId::TabsGoto9,
        key: "tabs.goto9",
        label: "Jump to tab 9",
        group: "Tabs",
        default_binding: "Mod+9",
    },
    ActionInfo {
        id: ActionId::FindOpen,
        key: "find.open",
        label: "Find in document",
        group: "Reading",
        default_binding: "Mod+F",
    },
    ActionInfo {
        id: ActionId::ZoomIn,
        key: "zoom.in",
        label: "Zoom in",
        group: "Reading",
        default_binding: "Mod++",
    },
    ActionInfo {
        id: ActionId::ZoomOut,
        key: "zoom.out",
        label: "Zoom out",
        group: "Reading",
        default_binding: "Mod+-",
    },
    ActionInfo {
        id: ActionId::BeaconSend,
        key: "beacon.send",
        label: "Send Beacon message",
        group: "Beacon",
        default_binding: "Mod+Enter",
    },
    ActionInfo {
        id: ActionId::LibrarySearch,
        key: "library.search",
        label: "Search across library",
        group: "Library",
        default_binding: "Mod+Shift+F",
    },
    ActionInfo {
        id: ActionId::TheaterStart,
        key: "theater.start",
        label: "Start Theater (presenter mode)",
        group: "Theater",
        default_binding: "Mod+Shift+P",
    },
    ActionInfo {
        id: ActionId::TheaterNext,
        key: "theater.next",
        label: "Next slide",
        group: "Theater",
        default_binding: "PageDown",
    },
    ActionInfo {
        id: ActionId::TheaterPrev,
        key: "theater.prev",
        label: "Previous slide",
        group: "Theater",
        default_binding: "PageUp",
    },
    ActionInfo {
        id: ActionId::TheaterToggleBlackout,
        key: "theater.blackout",
        label: "Toggle blackout",
        group: "Theater",
        default_binding: "B",
    },
    ActionInfo {
        id: ActionId::TheaterToggleInk,
        key: "theater.ink",
        label: "Toggle ink/laser pen",
        group: "Theater",
        default_binding: "I",
    },
    ActionInfo {
        id: ActionId::TheaterExit,
        key: "theater.exit",
        label: "Exit Theater",
        group: "Theater",
        default_binding: "Escape",
    },
    ActionInfo {
        id: ActionId::BedrockOpen,
        key: "bedrock.open",
        label: "Archive as PDF/A",
        group: "Archive",
        default_binding: "Mod+Shift+A",
    },
    ActionInfo {
        id: ActionId::PressOpen,
        key: "press.open",
        label: "Convert to PDF/X-4 (Press)",
        group: "Press",
        default_binding: "Mod+Shift+X",
    },
    ActionInfo {
        id: ActionId::FormsOpen,
        key: "forms.open",
        label: "Forms inspector & fill",
        group: "Forms",
        default_binding: "Mod+Shift+F",
    },
    ActionInfo {
        id: ActionId::QuillBatchOpen,
        key: "quill.batch",
        label: "Batch fill from CSV (mail-merge)",
        group: "Forms",
        default_binding: "Mod+Shift+B",
    },
    ActionInfo {
        id: ActionId::QuillDesignerOpen,
        key: "quill.designer",
        label: "Designer — author form fields",
        group: "Forms",
        default_binding: "Mod+Shift+D",
    },
    ActionInfo {
        id: ActionId::QuillAutodetectOpen,
        key: "quill.autodetect",
        label: "Auto-detect form fields on a flat PDF",
        group: "Forms",
        default_binding: "Mod+Shift+Y",
    },
    ActionInfo {
        id: ActionId::QuillTour,
        key: "quill.tour",
        label: "Show Forms welcome tour",
        group: "Forms",
        default_binding: "Mod+Shift+/",
    },
    ActionInfo {
        id: ActionId::AtelierOpen,
        key: "atelier.open",
        label: "Atelier — recipe runner",
        group: "Atelier",
        default_binding: "Mod+Shift+R",
    },
    ActionInfo {
        id: ActionId::HopperOpen,
        key: "hopper.open",
        label: "Hopper — watched folders",
        group: "Hopper",
        default_binding: "Mod+Shift+H",
    },
];

impl ActionId {
    pub fn as_str(&self) -> &'static str {
        ACTIONS
            .iter()
            .find(|a| a.id == *self)
            .expect("action registered in ACTIONS table")
            .key
    }

    pub fn info(&self) -> &'static ActionInfo {
        ACTIONS
            .iter()
            .find(|a| a.id == *self)
            .expect("action registered in ACTIONS table")
    }

    pub fn parse(s: &str) -> Result<ActionId, UnknownAction> {
        ACTIONS
            .iter()
            .find(|a| a.key == s)
            .map(|a| a.id)
            .ok_or_else(|| UnknownAction(s.to_string()))
    }

    pub fn all() -> impl Iterator<Item = ActionId> {
        ACTIONS.iter().map(|a| a.id)
    }
}

pub fn all_actions() -> &'static [ActionInfo] {
    ACTIONS
}

pub fn default_keymap() -> HashMap<ActionId, Binding> {
    let mut out = HashMap::with_capacity(ACTIONS.len());
    for a in ACTIONS {
        let b = Binding::from_str(a.default_binding).unwrap_or_else(|e| {
            panic!(
                "default binding for {} ({}) is invalid: {e}",
                a.key, a.default_binding
            )
        });
        out.insert(a.id, b);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown action id: {0}")]
pub struct UnknownAction(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_an_id() {
        for a in all_actions() {
            assert!(!a.key.is_empty(), "action id empty: {a:?}");
            assert!(
                a.key.contains('.'),
                "action id missing namespace: {}",
                a.key
            );
        }
    }

    #[test]
    fn action_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for a in all_actions() {
            assert!(seen.insert(a.key), "duplicate id: {}", a.key);
        }
    }

    #[test]
    fn parse_round_trip_action_ids() {
        for a in all_actions() {
            let parsed = ActionId::parse(a.key).unwrap();
            assert_eq!(parsed, a.id);
        }
    }

    #[test]
    fn unknown_action_id_errors() {
        assert!(ActionId::parse("does.not.exist").is_err());
    }

    #[test]
    fn defaults_cover_every_action() {
        let d = default_keymap();
        for a in all_actions() {
            assert!(d.contains_key(&a.id), "default missing for {}", a.key);
        }
    }

    #[test]
    fn defaults_are_parseable_bindings() {
        let d = default_keymap();
        for (id, b) in &d {
            let s = b.to_string();
            let reparsed = crate::keymap::Binding::parse(&s)
                .unwrap_or_else(|e| panic!("default for {} ({s}) not parseable: {e}", id.as_str()));
            assert_eq!(*b, reparsed);
        }
    }

    #[test]
    fn defaults_match_pre_v1_hardcoded_keys() {
        let d = default_keymap();
        assert_eq!(d[&ActionId::PaletteOpen].to_string(), "Mod+K");
        assert_eq!(d[&ActionId::ShortcutsShow].to_string(), "?");
        assert_eq!(d[&ActionId::TabsNew].to_string(), "Mod+T");
        assert_eq!(d[&ActionId::TabsClose].to_string(), "Mod+W");
        assert_eq!(d[&ActionId::TabsNext].to_string(), "Ctrl+Tab");
        assert_eq!(d[&ActionId::TabsPrev].to_string(), "Ctrl+Shift+Tab");
        assert_eq!(d[&ActionId::TabsGoto1].to_string(), "Mod+1");
        assert_eq!(d[&ActionId::TabsGoto9].to_string(), "Mod+9");
        assert_eq!(d[&ActionId::BeaconSend].to_string(), "Mod+Enter");
    }

    #[test]
    fn theater_defaults_are_presenter_native() {
        // Locks Slice 7 of the v2.3.0 "Theater" plan: presenter-mode
        // shortcuts must stay stable so muscle memory survives upgrades.
        let d = default_keymap();
        assert_eq!(d[&ActionId::TheaterStart].to_string(), "Mod+Shift+P");
        assert_eq!(d[&ActionId::TheaterNext].to_string(), "PageDown");
        assert_eq!(d[&ActionId::TheaterPrev].to_string(), "PageUp");
        assert_eq!(d[&ActionId::TheaterToggleBlackout].to_string(), "B");
        assert_eq!(d[&ActionId::TheaterToggleInk].to_string(), "I");
        assert_eq!(d[&ActionId::TheaterExit].to_string(), "Escape");
    }

    #[test]
    fn theater_actions_share_one_group() {
        // The Settings → Keymap panel sections rows by `group`. The six
        // Theater actions must all live under "Theater" so they appear
        // as one block, not scattered across other groups.
        for id in [
            ActionId::TheaterStart,
            ActionId::TheaterNext,
            ActionId::TheaterPrev,
            ActionId::TheaterToggleBlackout,
            ActionId::TheaterToggleInk,
            ActionId::TheaterExit,
        ] {
            assert_eq!(
                id.info().group,
                "Theater",
                "{} not in Theater group",
                id.as_str()
            );
        }
    }
}
