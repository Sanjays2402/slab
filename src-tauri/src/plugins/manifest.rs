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
    /// Optional v2.0.0 runtime section. Absent ⇒ declarative-only
    /// plugin (v1.x behavior preserved). Present ⇒ Slab loads a
    /// `script.js` from the plugin dir, hash-pins it, and (slice 3+)
    /// spawns a QuickJS VM with the declared capability set.
    #[serde(default)]
    pub runtime: Option<RuntimeManifest>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Fs,
    Net,
    Spawn,
}

/// v2.0.0 plugin runtime descriptor. Pointer + integrity + capabilities.
///
/// `entry` is a relative path within the plugin directory (typically
/// `script.js`). `sha256` is the lowercase hex digest of the entry
/// file's bytes; the loader verifies this at discovery time and
/// refuses to attach script bytes to a plugin if the digest doesn't
/// match. This is **TOFU + pin**: the manifest carries the expected
/// hash, the loader enforces it. The Forge author-signing model in
/// v2.1 will layer on top — verifying both manifest *and* hash via
/// a publisher key — but TOFU is the v2.0 baseline.
///
/// `capabilities` defaults to "all none" — declared capabilities are
/// the *upper bound* the plugin can request from the user; the actual
/// grant happens at install time in the Cabinet UI (slice 3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeManifest {
    /// Path to the JS entry file, relative to the plugin directory.
    /// Must end with `.js`. No path traversal — must be a simple
    /// filename or a forward-slash relative path with no `..`.
    pub entry: String,
    /// Lowercase hex SHA-256 of the entry file. Exactly 64 chars,
    /// `[0-9a-f]`. Validation is strict because a bad hash is
    /// always a tooling bug, never a user bug.
    pub sha256: String,
    #[serde(default)]
    pub capabilities: Capabilities,
}

/// Plugin-declared capability *upper bounds*. The user grants the
/// actual set on first enable (slice 3). All fields default to the
/// most restrictive variant — Slab plugins are default-deny.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    #[serde(default)]
    pub fs: FsCap,
    #[serde(default)]
    pub net: NetCap,
    #[serde(default)]
    pub ui: UiCap,
    #[serde(default)]
    pub beacon: BeaconCap,
    /// When `net = "specific"`, the allow-list of hosts (e.g.
    /// `["api.openai.com", "huggingface.co"]`). Ignored unless
    /// `net = "specific"`.
    #[serde(default)]
    pub net_allow_hosts: Vec<String>,
    /// When `fs != "none"`, the allow-list of path globs (e.g.
    /// `["~/Documents/**"]`). Ignored when `fs = "none"`.
    #[serde(default)]
    pub fs_allow_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FsCap {
    #[default]
    None,
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NetCap {
    #[default]
    None,
    /// Allow requests only to hosts in `Capabilities::net_allow_hosts`.
    Specific,
    /// Allow any host. **Discouraged** — surfaces a louder grant prompt.
    Any,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UiCap {
    #[default]
    None,
    Panel,
    Tool,
    Both,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BeaconCap {
    #[default]
    None,
    ToolProvider,
    AiProvider,
    Both,
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
        if let Some(rt) = &self.runtime {
            validate_runtime(rt)?;
        }
        Ok(())
    }
}

/// Strict checks on a `[runtime]` section. Called from `Manifest::validate`.
fn validate_runtime(rt: &RuntimeManifest) -> Result<(), ManifestError> {
    let entry = rt.entry.trim();
    if entry.is_empty() {
        return Err(ManifestError::Validation(
            "runtime.entry must not be empty".into(),
        ));
    }
    if !entry.ends_with(".js") {
        return Err(ManifestError::Validation(format!(
            "runtime.entry must end with .js (got {:?})",
            rt.entry
        )));
    }
    if entry.starts_with('/')
        || entry.starts_with('\\')
        || entry.contains("..")
        || entry.contains(":\\")
    {
        return Err(ManifestError::Validation(format!(
            "runtime.entry must be a relative path inside the plugin dir (got {:?})",
            rt.entry
        )));
    }
    if rt.sha256.len() != 64 || !rt.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ManifestError::Validation(format!(
            "runtime.sha256 must be a 64-char lowercase hex string (got {} chars)",
            rt.sha256.len()
        )));
    }
    if rt.sha256.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(ManifestError::Validation(
            "runtime.sha256 must be lowercase hex".into(),
        ));
    }
    if matches!(rt.capabilities.net, NetCap::Specific) && rt.capabilities.net_allow_hosts.is_empty()
    {
        return Err(ManifestError::Validation(
            "runtime.capabilities.net = \"specific\" requires non-empty net_allow_hosts".into(),
        ));
    }
    Ok(())
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

    /// The shipped `examples/plugins/hello-slab/plugin.toml` is the user
    /// docs' source of truth for the manifest format. If we ever change
    /// the schema in a way that breaks it, this test fires and we have
    /// to update the example too — keeping docs honest.
    #[test]
    fn example_hello_slab_manifest_parses() {
        // CARGO_MANIFEST_DIR is `<repo>/src-tauri`; the example lives at
        // `<repo>/examples/plugins/hello-slab/plugin.toml`.
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // -> <repo>
        path.push("examples/plugins/hello-slab/plugin.toml");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
        let m = Manifest::from_toml(&src).expect("hello-slab manifest must parse");
        assert_eq!(m.id, "com.example.hello-slab");
        assert_eq!(m.contributions.themes.len(), 1);
        assert_eq!(m.contributions.locales.len(), 1);
        assert_eq!(m.contributions.commands.len(), 2);
        assert_eq!(m.contributions.ai_providers.len(), 1);
        assert_eq!(m.contributions.pdf_actions.len(), 1);
    }

    // -------- v2.0.0 Workshop runtime section tests --------

    /// Declarative-only manifests (the v1.x norm) must continue to
    /// parse without a `[runtime]` block. Backward compatibility is
    /// non-negotiable.
    #[test]
    fn declarative_only_manifest_has_no_runtime() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.1.0"
            slab_compat = ">=1.3.0"
        "#;
        let m = Manifest::from_toml(src).unwrap();
        assert!(m.runtime.is_none());
    }

    #[test]
    fn runtime_section_parses_with_capabilities() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

            [runtime.capabilities]
            fs = "read"
            net = "specific"
            net_allow_hosts = ["api.openai.com"]
            ui = "panel"
            beacon = "tool-provider"
        "#;
        let m = Manifest::from_toml(src).unwrap();
        let rt = m.runtime.expect("runtime section should parse");
        assert_eq!(rt.entry, "script.js");
        assert_eq!(rt.capabilities.fs, FsCap::Read);
        assert_eq!(rt.capabilities.net, NetCap::Specific);
        assert_eq!(rt.capabilities.net_allow_hosts, vec!["api.openai.com"]);
        assert_eq!(rt.capabilities.ui, UiCap::Panel);
        assert_eq!(rt.capabilities.beacon, BeaconCap::ToolProvider);
    }

    #[test]
    fn runtime_capabilities_default_to_none() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        let m = Manifest::from_toml(src).unwrap();
        let caps = m.runtime.unwrap().capabilities;
        assert_eq!(caps.fs, FsCap::None);
        assert_eq!(caps.net, NetCap::None);
        assert_eq!(caps.ui, UiCap::None);
        assert_eq!(caps.beacon, BeaconCap::None);
        assert!(caps.net_allow_hosts.is_empty());
        assert!(caps.fs_allow_paths.is_empty());
    }

    #[test]
    fn runtime_rejects_non_js_entry() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.py"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains(".js"));
    }

    #[test]
    fn runtime_rejects_empty_entry() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = ""
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("entry"));
    }

    #[test]
    fn runtime_rejects_path_traversal_in_entry() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "../../etc/passwd.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("relative"));
    }

    #[test]
    fn runtime_rejects_absolute_entry() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "/etc/passwd.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("relative"));
    }

    #[test]
    fn runtime_rejects_short_sha256() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "deadbeef"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("sha256"));
    }

    #[test]
    fn runtime_rejects_uppercase_sha256() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("lowercase"));
    }

    #[test]
    fn runtime_rejects_non_hex_sha256() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "zzzz0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("hex"));
    }

    #[test]
    fn runtime_specific_net_requires_allow_hosts() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

            [runtime.capabilities]
            net = "specific"
        "#;
        let err = Manifest::from_toml(src).unwrap_err();
        assert!(format!("{err}").contains("net_allow_hosts"));
    }

    #[test]
    fn runtime_any_net_does_not_require_allow_hosts() {
        let src = r#"
            id = "com.example.hello"
            name = "Hello"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

            [runtime.capabilities]
            net = "any"
        "#;
        let m = Manifest::from_toml(src).expect("net = any should not require hosts list");
        assert_eq!(m.runtime.unwrap().capabilities.net, NetCap::Any);
    }
}
