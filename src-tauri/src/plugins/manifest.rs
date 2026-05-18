//! `plugin.toml` schema, parser, validation.
//!
//! See `mod.rs` for module-level docs. The data model is intentionally
//! pure — no I/O, no Tauri types — so it can be unit-tested in isolation
//! and re-used by the registry loader (Slice 2) and the example-plugins
//! repo's CI lint.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// SemVer requirement string for the Slab host version, e.g. ">=1.3.0".
    pub slab_compat: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub contributions: Contributions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Fs,
    Net,
    Spawn,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Contributions {
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
    #[serde(default)]
    pub locales: Vec<LocaleContribution>,
    #[serde(default)]
    pub pdf_actions: Vec<PdfActionContribution>,
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub ai_providers: Vec<AiProviderContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeContribution {
    pub id: String,
    pub label: String,
    /// CSS file relative to plugin dir.
    pub css: String,
    #[serde(default)]
    pub dark: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocaleContribution {
    pub locale: String,
    /// JSON file with the same shape as `src/lib/i18n/en.json`.
    pub bundle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PdfActionContribution {
    pub id: String,
    pub label: String,
    pub cli: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandContribution {
    pub id: String,
    pub label: String,
    /// Either `shell = "..."` or `url = "..."`. Validated post-parse
    /// (`Manifest::validate` enforces exactly one of the two).
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub default_keymap: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiProviderContribution {
    pub id: String,
    pub label: String,
    pub kind: String, // "openai_compat" for v1
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("validation failed: {0}")]
    Validation(String),
}

impl Manifest {
    /// Parse + validate in one call. Use this from the registry loader so
    /// validation cannot be forgotten.
    pub fn from_toml(src: &str) -> Result<Self, ManifestError> {
        let m: Self = toml::from_str(src)?;
        m.validate()?;
        Ok(m)
    }

    /// Post-parse semantic validation. Returns the first error encountered;
    /// the message names the offending field/id so the user can fix the file.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::Validation("id must not be empty".into()));
        }
        if !is_reverse_dns(&self.id) {
            return Err(ManifestError::Validation(format!(
                "id must be reverse-DNS like 'com.example.foo' (got {:?})",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Validation("name must not be empty".into()));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Validation(
                "version must not be empty".into(),
            ));
        }
        if self.slab_compat.trim().is_empty() {
            return Err(ManifestError::Validation(
                "slab_compat must not be empty (e.g. \">=1.3.0\")".into(),
            ));
        }
        for c in &self.contributions.commands {
            match (c.shell.is_some(), c.url.is_some()) {
                (false, false) => {
                    return Err(ManifestError::Validation(format!(
                        "command {:?} must set either shell or url",
                        c.id
                    )));
                }
                (true, true) => {
                    return Err(ManifestError::Validation(format!(
                        "command {:?} must set only one of shell or url",
                        c.id
                    )));
                }
                _ => {}
            }
        }
        for p in &self.contributions.pdf_actions {
            if p.cli.trim().is_empty() {
                return Err(ManifestError::Validation(format!(
                    "pdf_action {:?}: cli must not be empty",
                    p.id
                )));
            }
        }
        for ai in &self.contributions.ai_providers {
            if ai.kind != "openai_compat" {
                return Err(ManifestError::Validation(format!(
                    "ai_provider {:?}: only kind \"openai_compat\" is supported in v1.3.0",
                    ai.id
                )));
            }
            if !ai.base_url.starts_with("http://") && !ai.base_url.starts_with("https://") {
                return Err(ManifestError::Validation(format!(
                    "ai_provider {:?}: base_url must start with http:// or https://",
                    ai.id
                )));
            }
        }
        Ok(())
    }
}

fn is_reverse_dns(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_manifest_parses() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        assert_eq!(m.id, "com.example.hello");
        assert_eq!(m.name, "Hello");
        assert_eq!(m.version, "0.1.0");
        assert!(m.contributions.themes.is_empty());
        assert!(m.permissions.is_empty());
    }

    #[test]
    fn rejects_empty_id() {
        let src = r#"
            id = ""
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("id"));
    }

    #[test]
    fn rejects_non_reverse_dns_id() {
        let src = r#"
            id = "hello"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        assert!(m.validate().is_err());
    }

    #[test]
    fn command_must_have_shell_or_url() {
        let src = r#"
            id = "com.example.x"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
            [[contributions.commands]]
            id = "noop"
            label = "Noop"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("shell"));
    }

    #[test]
    fn command_rejects_both_shell_and_url() {
        let src = r#"
            id = "com.example.x"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
            [[contributions.commands]]
            id = "both"
            label = "Both"
            shell = "echo hi"
            url = "https://example.com"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("only one"));
    }

    #[test]
    fn ai_provider_rejects_unknown_kind() {
        let src = r#"
            id = "com.example.x"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
            [[contributions.ai_providers]]
            id = "weird"
            label = "Weird"
            kind = "magic"
            base_url = "http://localhost"
            default_model = "x"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("openai_compat"));
    }

    #[test]
    fn ai_provider_requires_http_scheme() {
        let src = r#"
            id = "com.example.x"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
            [[contributions.ai_providers]]
            id = "weird"
            label = "Weird"
            kind = "openai_compat"
            base_url = "ftp://nope"
            default_model = "x"
        "#;
        let m: Manifest = toml::from_str(src).unwrap();
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("http"));
    }

    #[test]
    fn accepts_full_manifest() {
        let src = include_str!("fixtures/full_manifest.toml");
        let m: Manifest = toml::from_str(src).unwrap();
        m.validate().expect("should validate");
        assert_eq!(m.contributions.themes.len(), 1);
        assert_eq!(m.contributions.locales.len(), 1);
        assert_eq!(m.contributions.pdf_actions.len(), 1);
        assert_eq!(m.contributions.commands.len(), 1);
        assert_eq!(m.contributions.ai_providers.len(), 1);
        assert_eq!(m.contributions.pdf_actions[0].timeout_ms, 60_000);
        assert!(m.permissions.contains(&Permission::Fs));
        assert!(m.permissions.contains(&Permission::Spawn));
    }

    #[test]
    fn pdf_action_default_timeout_is_30s() {
        let src = r#"
            id = "com.example.x"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
            [[contributions.pdf_actions]]
            id = "noop"
            label = "Noop"
            cli = "true"
        "#;
        let m = Manifest::from_toml(src).unwrap();
        assert_eq!(m.contributions.pdf_actions[0].timeout_ms, 30_000);
    }

    #[test]
    fn from_toml_combines_parse_and_validate() {
        let bad = r#"
            id = "no-dots"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
        "#;
        assert!(Manifest::from_toml(bad).is_err());

        let good = r#"
            id = "com.example.ok"
            name = "x"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
        "#;
        assert!(Manifest::from_toml(good).is_ok());
    }
}
