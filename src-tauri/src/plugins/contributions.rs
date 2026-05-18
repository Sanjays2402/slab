//! Contribution resolution: walk active plugins and surface their
//! themes, locales, PDF actions, commands, and AI providers as flat
//! lists with the plugin's directory carried alongside (so the
//! frontend / runtime can locate asset files).
//!
//! Everything here is read-only and side-effect-free except for
//! [`read_asset`], which reads a file under the plugin directory with
//! a path-traversal guard.

use crate::plugins::manifest::{
    AiProviderContribution, CommandContribution, LocaleContribution, PdfActionContribution,
    ThemeContribution,
};
use crate::plugins::registry::PluginRegistry;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// A theme contributed by a plugin, with the plugin's directory and ID
/// carried for asset lookup.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveTheme {
    pub plugin_id: String,
    pub plugin_dir: PathBuf,
    #[serde(flatten)]
    pub theme: ThemeContribution,
}

/// A locale bundle contributed by a plugin.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveLocale {
    pub plugin_id: String,
    pub plugin_dir: PathBuf,
    #[serde(flatten)]
    pub locale: LocaleContribution,
}

/// A PDF action contributed by a plugin.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActivePdfAction {
    pub plugin_id: String,
    pub plugin_dir: PathBuf,
    #[serde(flatten)]
    pub action: PdfActionContribution,
}

/// A custom command (palette entry) contributed by a plugin.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveCommand {
    pub plugin_id: String,
    pub plugin_dir: PathBuf,
    #[serde(flatten)]
    pub command: CommandContribution,
}

/// An AI provider contributed by a plugin.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveAiProvider {
    pub plugin_id: String,
    pub plugin_dir: PathBuf,
    #[serde(flatten)]
    pub provider: AiProviderContribution,
}

pub fn active_themes(reg: &PluginRegistry) -> Vec<ActiveTheme> {
    let mut out = Vec::new();
    for p in reg.active() {
        if let Some(m) = &p.manifest {
            for t in &m.contributions.themes {
                out.push(ActiveTheme {
                    plugin_id: p.id.clone(),
                    plugin_dir: p.dir.clone(),
                    theme: t.clone(),
                });
            }
        }
    }
    out
}

pub fn active_locales(reg: &PluginRegistry) -> Vec<ActiveLocale> {
    let mut out = Vec::new();
    for p in reg.active() {
        if let Some(m) = &p.manifest {
            for l in &m.contributions.locales {
                out.push(ActiveLocale {
                    plugin_id: p.id.clone(),
                    plugin_dir: p.dir.clone(),
                    locale: l.clone(),
                });
            }
        }
    }
    out
}

pub fn active_pdf_actions(reg: &PluginRegistry) -> Vec<ActivePdfAction> {
    let mut out = Vec::new();
    for p in reg.active() {
        if let Some(m) = &p.manifest {
            for a in &m.contributions.pdf_actions {
                out.push(ActivePdfAction {
                    plugin_id: p.id.clone(),
                    plugin_dir: p.dir.clone(),
                    action: a.clone(),
                });
            }
        }
    }
    out
}

pub fn active_commands(reg: &PluginRegistry) -> Vec<ActiveCommand> {
    let mut out = Vec::new();
    for p in reg.active() {
        if let Some(m) = &p.manifest {
            for c in &m.contributions.commands {
                out.push(ActiveCommand {
                    plugin_id: p.id.clone(),
                    plugin_dir: p.dir.clone(),
                    command: c.clone(),
                });
            }
        }
    }
    out
}

pub fn active_ai_providers(reg: &PluginRegistry) -> Vec<ActiveAiProvider> {
    let mut out = Vec::new();
    for p in reg.active() {
        if let Some(m) = &p.manifest {
            for a in &m.contributions.ai_providers {
                out.push(ActiveAiProvider {
                    plugin_id: p.id.clone(),
                    plugin_dir: p.dir.clone(),
                    provider: a.clone(),
                });
            }
        }
    }
    out
}

/// Read an asset file declared by a plugin (theme CSS, locale JSON…)
/// relative to the plugin directory. Path-traversal is rejected: the
/// canonicalized path must stay inside the canonicalized plugin dir.
///
/// Returns the file contents as a string. Binary assets are not
/// expected here — themes are CSS and locales are JSON.
pub fn read_asset(plugin_dir: &Path, relative: &str) -> Result<String, String> {
    // Reject absolute and parent-escaping relative paths up front.
    let rel = Path::new(relative);
    if rel.is_absolute() {
        return Err(format!(
            "asset path must be relative to plugin dir, got {relative:?}"
        ));
    }
    let full = plugin_dir.join(rel);
    let plugin_canon = plugin_dir
        .canonicalize()
        .map_err(|e| format!("plugin dir not accessible: {e}"))?;
    let full_canon = full
        .canonicalize()
        .map_err(|e| format!("asset not found: {e}"))?;
    if !full_canon.starts_with(&plugin_canon) {
        return Err(format!("asset path escapes plugin dir: {relative:?}"));
    }
    fs::read_to_string(&full_canon).map_err(|e| format!("could not read asset: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::registry::EnabledState;
    use std::fs;
    use tempfile::TempDir;

    fn write_plugin(root: &Path, name: &str, toml: &str) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.toml"), toml).unwrap();
        dir
    }

    fn make_registry_with_full_plugin(tmp: &TempDir) -> PluginRegistry {
        let dir = write_plugin(
            tmp.path(),
            "full",
            r#"
                id = "com.example.full"
                name = "Full"
                version = "0.1.0"
                slab_compat = ">=1.3.0"

                [[contributions.themes]]
                id = "dim"
                label = "Dim"
                css = "themes/dim.css"
                dark = true

                [[contributions.locales]]
                locale = "ja"
                bundle = "locales/ja.json"

                [[contributions.pdf_actions]]
                id = "qpdf-linearize"
                label = "Linearize"
                cli = "qpdf"
                args = ["--linearize", "{in}", "{out}"]

                [[contributions.commands]]
                id = "open-docs"
                label = "Open docs"
                url = "https://example.com"

                [[contributions.ai_providers]]
                id = "local-llamacpp"
                label = "Local"
                kind = "openai_compat"
                base_url = "http://127.0.0.1:8080/v1"
                default_model = "qwen2.5"
            "#,
        );
        fs::create_dir_all(dir.join("themes")).unwrap();
        fs::write(dir.join("themes/dim.css"), ":root { --bg: #111; }").unwrap();
        fs::create_dir_all(dir.join("locales")).unwrap();
        fs::write(dir.join("locales/ja.json"), r#"{"hello":"こんにちは"}"#).unwrap();
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        reg
    }

    #[test]
    fn active_themes_returns_one_per_theme_contribution() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let themes = active_themes(&reg);
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].plugin_id, "com.example.full");
        assert_eq!(themes[0].theme.id, "dim");
        assert!(themes[0].theme.dark);
    }

    #[test]
    fn active_locales_returns_one_per_locale_contribution() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let locales = active_locales(&reg);
        assert_eq!(locales.len(), 1);
        assert_eq!(locales[0].locale.locale, "ja");
    }

    #[test]
    fn active_pdf_actions_returns_one_per_action() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let acts = active_pdf_actions(&reg);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].action.id, "qpdf-linearize");
        assert_eq!(acts[0].action.cli, "qpdf");
    }

    #[test]
    fn active_commands_returns_one_per_command() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let cmds = active_commands(&reg);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].command.id, "open-docs");
    }

    #[test]
    fn active_ai_providers_returns_one_per_provider() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let ai = active_ai_providers(&reg);
        assert_eq!(ai.len(), 1);
        assert_eq!(ai[0].provider.id, "local-llamacpp");
    }

    #[test]
    fn disabled_plugin_contributes_nothing() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        reg.set_enabled("com.example.full", false);
        assert!(active_themes(&reg).is_empty());
        assert!(active_locales(&reg).is_empty());
        assert!(active_pdf_actions(&reg).is_empty());
        assert!(active_commands(&reg).is_empty());
        assert!(active_ai_providers(&reg).is_empty());
    }

    #[test]
    fn read_asset_returns_file_contents() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let p = reg.get("com.example.full").unwrap();
        let css = read_asset(&p.dir, "themes/dim.css").unwrap();
        assert!(css.contains("--bg"));
    }

    #[test]
    fn read_asset_rejects_parent_traversal() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let p = reg.get("com.example.full").unwrap();
        // Create a sibling file we should NOT be able to read.
        fs::write(tmp.path().join("secrets.txt"), "nope").unwrap();
        let err = read_asset(&p.dir, "../secrets.txt").unwrap_err();
        // Either "escapes plugin dir" (after canonicalize succeeds) or
        // "asset not found" (if the relative path doesn't resolve) is
        // acceptable — both prevent the leak.
        assert!(
            err.contains("escapes") || err.contains("not found"),
            "expected traversal rejection, got: {err}"
        );
    }

    #[test]
    fn read_asset_rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let p = reg.get("com.example.full").unwrap();
        let err = read_asset(&p.dir, "/etc/passwd").unwrap_err();
        assert!(err.contains("relative"));
    }

    #[test]
    fn read_asset_returns_error_for_missing_file() {
        let tmp = TempDir::new().unwrap();
        let reg = make_registry_with_full_plugin(&tmp);
        let p = reg.get("com.example.full").unwrap();
        let err = read_asset(&p.dir, "themes/missing.css").unwrap_err();
        assert!(err.contains("not found"));
    }
}
