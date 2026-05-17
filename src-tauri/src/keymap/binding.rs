//! Parse + match keyboard bindings. Storage format is a stable
//! `Mod+Shift+K`-style string so users can hand-edit `~/.slab/config.toml`
//! without surprises.
//!
//! `Mod` is the platform-abstract modifier — macOS = Cmd, others = Ctrl.
//! It's preserved as `Mod` on disk; the platform-specific check happens
//! at match time.
//!
//! Canonical print order is `Mod, Ctrl, Alt, Shift, <key>`. Any input
//! order parses, but `Display` always emits the canonical form so the
//! diff against `defaults()` is stable.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Mod,
    Ctrl,
    Alt,
    Shift,
}

/// Tiny bitset over the 4 modifiers. We don't pull `bitflags!` for this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ModifierSet(u8);

const M_MOD: u8 = 1 << 0;
const M_CTRL: u8 = 1 << 1;
const M_ALT: u8 = 1 << 2;
const M_SHIFT: u8 = 1 << 3;

impl ModifierSet {
    pub fn empty() -> Self {
        Self(0)
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn contains(self, m: Modifier) -> bool {
        self.0 & Self::bit(m) != 0
    }
    pub fn insert(&mut self, m: Modifier) {
        self.0 |= Self::bit(m);
    }
    fn bit(m: Modifier) -> u8 {
        match m {
            Modifier::Mod => M_MOD,
            Modifier::Ctrl => M_CTRL,
            Modifier::Alt => M_ALT,
            Modifier::Shift => M_SHIFT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub modifiers: ModifierSet,
    /// Normalised key string. Single-char keys are stored as the upper-
    /// case letter (`"K"`) or the literal character (`"?"`, `"+"`).
    /// Named keys keep their JS `KeyboardEvent.key` capitalisation
    /// (`"Tab"`, `"Enter"`, `"Escape"`, `"ArrowUp"`, `"PageDown"`, …).
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("binding is empty")]
    Empty,
    #[error("binding has no non-modifier key")]
    OnlyModifiers,
    #[error("unknown modifier: {0}")]
    UnknownModifier(String),
    #[error("invalid key token: {0}")]
    InvalidKey(String),
}

const NAMED_KEYS: &[&str] = &[
    "Tab",
    "Enter",
    "Escape",
    "Space",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "PageUp",
    "PageDown",
    "Home",
    "End",
    "Insert",
    "Delete",
    "Backspace",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
];

fn canonical_key(token: &str) -> Result<String, ParseError> {
    // 1. Named key (case-sensitive match against our allow-list).
    if NAMED_KEYS.contains(&token) {
        return Ok(token.to_string());
    }
    // 2. Single character (letter, digit, punctuation).
    let mut chars = token.chars();
    let first = chars
        .next()
        .ok_or_else(|| ParseError::InvalidKey(token.into()))?;
    if chars.next().is_some() {
        return Err(ParseError::InvalidKey(token.into()));
    }
    // Letters are stored upper-case.
    Ok(if first.is_ascii_alphabetic() {
        first.to_ascii_uppercase().to_string()
    } else {
        first.to_string()
    })
}

fn parse_modifier(token: &str) -> Option<Modifier> {
    match token {
        "Mod" => Some(Modifier::Mod),
        "Ctrl" => Some(Modifier::Ctrl),
        "Alt" | "Option" | "Opt" => Some(Modifier::Alt),
        "Shift" => Some(Modifier::Shift),
        _ => None,
    }
}

impl Binding {
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseError::Empty);
        }
        // Split on '+' but tolerate a final empty token that means the
        // bound key is literally '+', e.g. "Mod++" → Mod+'+'.
        let raw_tokens: Vec<&str> = trimmed.split('+').collect();
        // Detect the "trailing-plus-as-key" pattern: last token empty AND
        // we have ≥ 2 tokens (e.g. "Mod++" → ["Mod", "", ""] after split,
        // but split on '+' for "Mod++" gives ["Mod", "", ""] actually
        // produces 3 — let's normalise).
        let tokens: Vec<String> = {
            // Special-case "Mod++" → ["Mod", "+"] (key is literally '+').
            if let Some(head) = trimmed.strip_suffix("++") {
                let mut v: Vec<String> = head.split('+').map(|s| s.to_string()).collect();
                v.push("+".to_string());
                v
            } else if trimmed == "+" {
                vec!["+".to_string()]
            } else {
                raw_tokens.iter().map(|s| s.to_string()).collect()
            }
        };

        let mut modifiers = ModifierSet::empty();
        let mut key: Option<String> = None;
        let last = tokens.len() - 1;
        for (i, raw) in tokens.iter().enumerate() {
            let tok = raw.trim();
            if i < last {
                let m =
                    parse_modifier(tok).ok_or_else(|| ParseError::UnknownModifier(tok.into()))?;
                modifiers.insert(m);
            } else {
                // Last token: either a modifier-only string (error) or the key.
                if tok.is_empty() {
                    return Err(ParseError::Empty);
                }
                if parse_modifier(tok).is_some() {
                    return Err(ParseError::OnlyModifiers);
                }
                key = Some(canonical_key(tok)?);
            }
        }
        let key = key.ok_or(ParseError::Empty)?;
        Ok(Binding { modifiers, key })
    }

    /// Does the given keyboard event match this binding on the given platform?
    pub fn matches(&self, ev: &KeyEvent, platform: Platform) -> bool {
        let want_mod = self.modifiers.contains(Modifier::Mod);
        let want_ctrl = self.modifiers.contains(Modifier::Ctrl);
        let want_alt = self.modifiers.contains(Modifier::Alt);
        let want_shift = self.modifiers.contains(Modifier::Shift);
        // Mod resolves to meta on mac, ctrl elsewhere. On Mac, the explicit
        // Ctrl modifier is the ev.ctrl field. On non-mac, Mod *is* Ctrl,
        // so an explicit Ctrl-in-the-binding can't co-exist with Mod (we
        // treat it as already-covered by Mod).
        let (got_mod, got_explicit_ctrl) = match platform {
            Platform::Mac => (ev.meta, ev.ctrl),
            // On non-mac, there's only one "Ctrl"-shaped key. If both
            // `Mod` AND `Ctrl` are required by the binding, that's an
            // impossible combo on non-mac (we'd refuse to match).
            // Otherwise `Mod` consumes ev.ctrl and `got_explicit_ctrl`
            // is reported as false.
            Platform::Other => {
                if want_mod && want_ctrl {
                    return false;
                }
                if want_mod {
                    (ev.ctrl, false)
                } else {
                    (false, ev.ctrl)
                }
            }
        };
        if want_mod != got_mod {
            return false;
        }
        if want_ctrl != got_explicit_ctrl {
            return false;
        }
        if want_alt != ev.alt {
            return false;
        }
        if want_shift != ev.shift {
            return false;
        }
        // Compare keys case-insensitively for ASCII letters, exact otherwise.
        if self.key.len() == 1
            && self
                .key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            ev.key.eq_ignore_ascii_case(&self.key)
        } else {
            ev.key == self.key
        }
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in [
            Modifier::Mod,
            Modifier::Ctrl,
            Modifier::Alt,
            Modifier::Shift,
        ] {
            if self.modifiers.contains(m) {
                let s = match m {
                    Modifier::Mod => "Mod",
                    Modifier::Ctrl => "Ctrl",
                    Modifier::Alt => "Alt",
                    Modifier::Shift => "Shift",
                };
                write!(f, "{s}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl FromStr for Binding {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Binding {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Binding::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Platform tag for `matches()`. We don't auto-detect at the binding
/// level so tests can drive both branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Other
        }
    }
}

/// Minimal cross-platform keyboard-event mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: String,
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_letter() {
        let b = Binding::parse("K").unwrap();
        assert_eq!(b.key, "K");
        assert!(b.modifiers.is_empty());
        assert_eq!(b.to_string(), "K");
    }

    #[test]
    fn parse_mod_plus_letter() {
        let b = Binding::parse("Mod+K").unwrap();
        assert!(b.modifiers.contains(Modifier::Mod));
        assert_eq!(b.key, "K");
        assert_eq!(b.to_string(), "Mod+K");
    }

    #[test]
    fn parse_multi_mod_canonical_order() {
        // Canonical order is Mod, Ctrl, Alt, Shift regardless of input order.
        let b = Binding::parse("Shift+Ctrl+Tab").unwrap();
        assert_eq!(b.to_string(), "Ctrl+Shift+Tab");
    }

    #[test]
    fn parse_question_mark() {
        let b = Binding::parse("?").unwrap();
        assert_eq!(b.key, "?");
        assert!(b.modifiers.is_empty());
    }

    #[test]
    fn parse_named_special_keys() {
        for name in &["Tab", "Enter", "Escape", "ArrowUp", "PageDown", "F5"] {
            let b = Binding::parse(name).unwrap();
            assert_eq!(b.key, *name);
        }
    }

    #[test]
    fn parse_digit() {
        let b = Binding::parse("Mod+1").unwrap();
        assert_eq!(b.key, "1");
        assert_eq!(b.to_string(), "Mod+1");
    }

    #[test]
    fn parse_trailing_plus_as_key() {
        // Mod++ means Mod + the '+' key (used for "zoom in" on many keymaps).
        let b = Binding::parse("Mod++").unwrap();
        assert_eq!(b.key, "+");
        assert!(b.modifiers.contains(Modifier::Mod));
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(Binding::parse("").is_err());
        assert!(Binding::parse("   ").is_err());
    }

    #[test]
    fn parse_rejects_only_modifiers() {
        assert!(Binding::parse("Mod+Shift").is_err());
    }

    #[test]
    fn parse_rejects_unknown_modifier() {
        assert!(Binding::parse("Hyper+K").is_err());
    }

    #[test]
    fn parse_rejects_multichar_letter_key() {
        assert!(Binding::parse("Mod+KK").is_err());
    }

    #[test]
    fn matches_event_mac() {
        let b = Binding::parse("Mod+K").unwrap();
        let ev = KeyEvent {
            key: "k".into(),
            meta: true,
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert!(b.matches(&ev, Platform::Mac));
        // On Mac, plain Ctrl+K is NOT Mod+K.
        let ev2 = KeyEvent {
            key: "k".into(),
            meta: false,
            ctrl: true,
            alt: false,
            shift: false,
        };
        assert!(!b.matches(&ev2, Platform::Mac));
    }

    #[test]
    fn matches_event_non_mac() {
        let b = Binding::parse("Mod+K").unwrap();
        // On Linux/Windows, Mod == Ctrl.
        let ev = KeyEvent {
            key: "k".into(),
            meta: false,
            ctrl: true,
            alt: false,
            shift: false,
        };
        assert!(b.matches(&ev, Platform::Other));
        // Plain Meta+K is NOT Mod+K on non-mac.
        let ev2 = KeyEvent {
            key: "k".into(),
            meta: true,
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert!(!b.matches(&ev2, Platform::Other));
    }

    #[test]
    fn matches_case_insensitive_letters() {
        let b = Binding::parse("K").unwrap();
        let lower = KeyEvent {
            key: "k".into(),
            meta: false,
            ctrl: false,
            alt: false,
            shift: false,
        };
        let upper = KeyEvent {
            key: "K".into(),
            meta: false,
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert!(b.matches(&lower, Platform::Other));
        assert!(b.matches(&upper, Platform::Other));
    }

    #[test]
    fn matches_named_key_exact() {
        let b = Binding::parse("Ctrl+Tab").unwrap();
        let ev = KeyEvent {
            key: "Tab".into(),
            meta: false,
            ctrl: true,
            alt: false,
            shift: false,
        };
        assert!(b.matches(&ev, Platform::Mac));
        assert!(b.matches(&ev, Platform::Other));
    }

    #[test]
    fn binding_serde_round_trip_via_toml() {
        // Direct serialisation: not through KeymapConfig.
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            #[serde(default)]
            b: Option<Binding>,
        }
        let h = Holder {
            b: Some(Binding::parse("Mod+Shift+Tab").unwrap()),
        };
        let s = toml::to_string(&h).unwrap();
        let h2: Holder = toml::from_str(&s).unwrap();
        assert_eq!(h, h2);
    }
}
