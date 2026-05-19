//! Plugin discovery + in-memory registry (Slice 2).
//!
//! Scans `~/.slab/plugins/*/plugin.toml` at boot (and on demand via
//! `reload`), parses each manifest with [`Manifest::from_toml`], and
//! stores the result alongside the plugin's on-disk path and an
//! enabled/disabled flag. The registry is held in a Tauri `State` so any
//! backend command can query it without plumbing arguments through.
//!
//! Errors are *per-plugin*: one broken manifest doesn't take down the
//! whole load. The error message is kept on the [`Plugin`] entry so the
//! UI (Slice 9) can surface it next to the directory name.
//!
//! Enabled-state persistence lives in `~/.slab/plugin-state.toml` — a
//! tiny `{ "com.example.foo" = true, … }` map. Plugins default to
//! enabled on first discovery, then their explicit on/off persists.

use crate::plugins::manifest::{Manifest, ManifestError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A single discovered plugin. `manifest` is `None` if parsing failed —
/// in that case `error` carries the human-readable reason and the
/// plugin is effectively disabled regardless of `enabled`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plugin {
    /// Plugin directory (absolute), e.g. `~/.slab/plugins/com.example.foo`.
    pub dir: PathBuf,
    /// Manifest ID copied up for fast lookup. For broken manifests this
    /// falls back to the directory name.
    pub id: String,
    /// Parsed manifest, or `None` if parsing/validation failed.
    pub manifest: Option<Manifest>,
    /// User-controllable on/off flag (persists to `plugin-state.toml`).
    pub enabled: bool,
    /// Last parse error, if any. `None` for healthy plugins.
    pub error: Option<String>,
    /// v2.0.0 Workshop: verified bytes of the plugin's JS entry file,
    /// when the manifest declared a `[runtime]` section AND the
    /// on-disk SHA-256 matched the pinned hash. `None` for pure-
    /// declarative plugins and for plugins whose runtime check failed
    /// (in the failure case `error` carries the reason).
    ///
    /// Skipped from serde so the in-memory bytes (possibly large) don't
    /// leak into IPC payloads. The frontend has no business reading the
    /// raw script anyway — it only needs to know whether the plugin
    /// loaded.
    #[serde(skip)]
    pub script_bytes: Option<Vec<u8>>,
}

impl Plugin {
    /// Convenience: is this plugin actually usable right now?
    pub fn is_active(&self) -> bool {
        self.enabled && self.manifest.is_some() && self.error.is_none()
    }
}

/// In-memory plugin registry. Cheap to clone via `list()`; mutating
/// operations go through `Mutex`. Stored via `tauri::Manager::manage`
/// so command handlers grab it through `tauri::State<'_, PluginRegistry>`.
#[derive(Default)]
pub struct PluginRegistry {
    inner: Mutex<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    /// Keyed by plugin ID (or directory name if parse failed).
    plugins: HashMap<String, Plugin>,
    /// Root dir scanned by `discover()`. Set on first load so `reload`
    /// can re-scan without a second arg.
    root: Option<PathBuf>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load all plugins under `root` (typically `~/.slab/plugins`).
    /// Applies persisted enabled-state from `enabled_state` (load it
    /// once from `plugin-state.toml` via [`read_enabled_state`]).
    ///
    /// Replaces any previous contents. Missing root is treated as
    /// "zero plugins" rather than an error — most users won't have a
    /// plugins dir on first boot.
    pub fn discover(&self, root: &Path, enabled_state: &EnabledState) {
        let mut found: HashMap<String, Plugin> = HashMap::new();
        if let Ok(rd) = fs::read_dir(root) {
            for entry in rd.flatten() {
                let dir = entry.path();
                if !dir.is_dir() {
                    continue;
                }
                let manifest_path = dir.join("plugin.toml");
                if !manifest_path.is_file() {
                    continue;
                }
                let plugin = load_one(&dir, &manifest_path, enabled_state);
                found.insert(plugin.id.clone(), plugin);
            }
        }
        let mut g = self.inner.lock().unwrap();
        g.plugins = found;
        g.root = Some(root.to_path_buf());
    }

    /// Re-run discovery using the previously-set root. No-op if
    /// `discover` was never called (returns false).
    pub fn reload(&self, enabled_state: &EnabledState) -> bool {
        let root = { self.inner.lock().unwrap().root.clone() };
        match root {
            Some(r) => {
                self.discover(&r, enabled_state);
                true
            }
            None => false,
        }
    }

    /// Snapshot of all known plugins, sorted by ID for stable UI order.
    pub fn list(&self) -> Vec<Plugin> {
        let g = self.inner.lock().unwrap();
        let mut v: Vec<Plugin> = g.plugins.values().cloned().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    /// Snapshot of *active* plugins only (enabled + parsed cleanly).
    /// Use this from feature code that consumes contributions.
    pub fn active(&self) -> Vec<Plugin> {
        self.list().into_iter().filter(|p| p.is_active()).collect()
    }

    pub fn get(&self, id: &str) -> Option<Plugin> {
        self.inner.lock().unwrap().plugins.get(id).cloned()
    }

    /// Flip enabled flag. Returns true if the plugin exists, false if
    /// the ID is unknown. The caller is responsible for persisting the
    /// new state via [`write_enabled_state`].
    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.plugins.get_mut(id) {
            Some(p) => {
                p.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// Snapshot of the enabled-state map for persistence.
    pub fn enabled_state(&self) -> EnabledState {
        let g = self.inner.lock().unwrap();
        EnabledState {
            plugins: g
                .plugins
                .iter()
                .map(|(k, v)| (k.clone(), v.enabled))
                .collect(),
        }
    }
}

fn load_one(dir: &Path, manifest_path: &Path, enabled_state: &EnabledState) -> Plugin {
    let fallback_id = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<invalid>")
        .to_string();
    match fs::read_to_string(manifest_path) {
        Ok(src) => match Manifest::from_toml(&src) {
            Ok(m) => {
                let id = m.id.clone();
                let enabled = enabled_state.get(&id).unwrap_or(true);
                // v2.0.0 Workshop: if manifest declares a [runtime]
                // section, read+hash the JS entry file and verify it
                // matches the pinned hash. Hash mismatch is a hard
                // failure (treated like a parse error).
                let script_outcome = match &m.runtime {
                    Some(rt) => load_and_verify_script(dir, rt),
                    None => ScriptOutcome::None,
                };
                match script_outcome {
                    ScriptOutcome::None => Plugin {
                        dir: dir.to_path_buf(),
                        id,
                        manifest: Some(m),
                        enabled,
                        error: None,
                        script_bytes: None,
                    },
                    ScriptOutcome::Ok(bytes) => Plugin {
                        dir: dir.to_path_buf(),
                        id,
                        manifest: Some(m),
                        enabled,
                        error: None,
                        script_bytes: Some(bytes),
                    },
                    ScriptOutcome::Err(reason) => Plugin {
                        dir: dir.to_path_buf(),
                        id,
                        manifest: None,
                        enabled: false,
                        error: Some(reason),
                        script_bytes: None,
                    },
                }
            }
            Err(e) => Plugin {
                dir: dir.to_path_buf(),
                id: fallback_id,
                manifest: None,
                enabled: false,
                error: Some(format_manifest_error(&e)),
                script_bytes: None,
            },
        },
        Err(io) => Plugin {
            dir: dir.to_path_buf(),
            id: fallback_id,
            manifest: None,
            enabled: false,
            error: Some(format!("could not read plugin.toml: {io}")),
            script_bytes: None,
        },
    }
}

/// Outcome of attempting to load the JS payload for a runtime-using plugin.
enum ScriptOutcome {
    /// No `[runtime]` section in the manifest — pure-declarative plugin.
    None,
    /// Script file existed and matched the pinned hash. Bytes attached.
    Ok(Vec<u8>),
    /// Script file missing, unreadable, or hash mismatch. Reason is
    /// human-readable for the Cabinet UI.
    Err(String),
}

/// Read the plugin's JS entry file, compute SHA-256, compare to the
/// manifest-pinned hash. Returns the verified bytes on match, an error
/// string otherwise. Pure: no panics, no `unwrap` on user input.
fn load_and_verify_script(
    dir: &Path,
    rt: &crate::plugins::manifest::RuntimeManifest,
) -> ScriptOutcome {
    let script_path = dir.join(&rt.entry);
    let bytes = match fs::read(&script_path) {
        Ok(b) => b,
        Err(e) => {
            return ScriptOutcome::Err(format!(
                "runtime.entry {:?} could not be read: {e}",
                rt.entry
            ));
        }
    };
    let actual = hex_sha256(&bytes);
    // Manifest validation already enforced lowercase + 64-char hex,
    // so a direct string compare is correct.
    if actual != rt.sha256 {
        return ScriptOutcome::Err(format!(
            "runtime.sha256 mismatch for {:?}: manifest says {}, on-disk is {}",
            rt.entry, rt.sha256, actual
        ));
    }
    ScriptOutcome::Ok(bytes)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn format_manifest_error(e: &ManifestError) -> String {
    match e {
        ManifestError::Parse(p) => format!("parse error: {p}"),
        ManifestError::Validation(v) => format!("validation: {v}"),
    }
}

// ---------- Enabled-state persistence ----------

/// Persisted enabled/disabled flags for known plugin IDs. Stored at
/// `~/.slab/plugin-state.toml` as a flat TOML table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EnabledState {
    #[serde(flatten)]
    pub plugins: HashMap<String, bool>,
}

impl EnabledState {
    pub fn get(&self, id: &str) -> Option<bool> {
        self.plugins.get(id).copied()
    }
}

/// Default location for the enabled-state file. Returns `None` when
/// `HOME` is unset, in which case persistence is a no-op.
pub fn default_state_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".slab").join("plugin-state.toml"))
}

/// Default plugins root (`~/.slab/plugins`).
pub fn default_plugins_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".slab").join("plugins"))
}

pub fn read_enabled_state(path: &Path) -> EnabledState {
    match fs::read_to_string(path) {
        Ok(src) => toml::from_str(&src).unwrap_or_default(),
        Err(_) => EnabledState::default(),
    }
}

pub fn write_enabled_state(path: &Path, state: &EnabledState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let src = toml::to_string_pretty(state).unwrap_or_default();
    fs::write(path, src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_plugin(root: &Path, name: &str, toml: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.toml"), toml).unwrap();
    }

    /// Helper for v2.0.0 runtime tests: write a plugin dir containing
    /// both `plugin.toml` and `script.js`. Returns the actual SHA-256
    /// of the script so tests can decide whether to pin it correctly
    /// or wrongly.
    fn write_runtime_plugin(root: &Path, name: &str, script_body: &str) -> (PathBuf, String) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("script.js"), script_body).unwrap();
        let actual = hex_sha256(script_body.as_bytes());
        (dir, actual)
    }

    #[test]
    fn discovers_zero_plugins_when_root_missing() {
        let tmp = TempDir::new().unwrap();
        let reg = PluginRegistry::new();
        reg.discover(&tmp.path().join("does-not-exist"), &EnabledState::default());
        assert!(reg.list().is_empty());
    }

    #[test]
    fn discovers_valid_plugin() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "hello",
            r#"
                id = "com.example.hello"
                name = "Hello"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugins = reg.list();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "com.example.hello");
        assert!(plugins[0].manifest.is_some());
        assert!(plugins[0].enabled);
        assert!(plugins[0].error.is_none());
        assert!(plugins[0].is_active());
    }

    #[test]
    fn captures_parse_error_per_plugin() {
        let tmp = TempDir::new().unwrap();
        // One healthy plugin
        write_plugin(
            tmp.path(),
            "good",
            r#"
                id = "com.example.good"
                name = "Good"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        // One broken plugin (id missing dots → fails reverse-DNS check)
        write_plugin(
            tmp.path(),
            "broken",
            r#"
                id = "no-dots"
                name = "Broken"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugins = reg.list();
        assert_eq!(plugins.len(), 2);
        let good = plugins.iter().find(|p| p.id == "com.example.good").unwrap();
        let broken = plugins.iter().find(|p| p.id == "broken").unwrap();
        assert!(good.is_active());
        assert!(broken.error.is_some());
        assert!(broken.manifest.is_none());
        assert!(!broken.is_active());
    }

    #[test]
    fn skips_dirs_without_manifest() {
        let tmp = TempDir::new().unwrap();
        // Empty dir under root — should be ignored, not erroneous.
        fs::create_dir_all(tmp.path().join("not-a-plugin")).unwrap();
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        assert!(reg.list().is_empty());
    }

    #[test]
    fn enabled_state_persists_round_trip() {
        let tmp = TempDir::new().unwrap();
        let state_path = tmp.path().join("plugin-state.toml");
        let mut state = EnabledState::default();
        state.plugins.insert("com.example.a".into(), true);
        state.plugins.insert("com.example.b".into(), false);
        write_enabled_state(&state_path, &state).unwrap();
        let loaded = read_enabled_state(&state_path);
        assert_eq!(loaded.get("com.example.a"), Some(true));
        assert_eq!(loaded.get("com.example.b"), Some(false));
        assert_eq!(loaded.get("com.example.missing"), None);
    }

    #[test]
    fn discover_applies_persisted_enabled_state() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "off",
            r#"
                id = "com.example.off"
                name = "Off"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let mut state = EnabledState::default();
        state.plugins.insert("com.example.off".into(), false);

        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &state);
        let plugins = reg.list();
        assert_eq!(plugins.len(), 1);
        assert!(!plugins[0].enabled);
        assert!(
            !plugins[0].is_active(),
            "disabled plugin must not be active"
        );
    }

    #[test]
    fn set_enabled_flips_flag() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "x",
            r#"
                id = "com.example.x"
                name = "X"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        assert!(reg.get("com.example.x").unwrap().enabled);
        assert!(reg.set_enabled("com.example.x", false));
        assert!(!reg.get("com.example.x").unwrap().enabled);
        assert!(!reg.set_enabled("com.example.unknown", true));
    }

    #[test]
    fn reload_rescans_dir() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "one",
            r#"
                id = "com.example.one"
                name = "One"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        assert_eq!(reg.list().len(), 1);

        write_plugin(
            tmp.path(),
            "two",
            r#"
                id = "com.example.two"
                name = "Two"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        assert!(reg.reload(&EnabledState::default()));
        assert_eq!(reg.list().len(), 2);
    }

    #[test]
    fn reload_without_prior_discover_returns_false() {
        let reg = PluginRegistry::new();
        assert!(!reg.reload(&EnabledState::default()));
    }

    #[test]
    fn enabled_state_snapshot_after_changes() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "a",
            r#"
                id = "com.example.a"
                name = "A"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        write_plugin(
            tmp.path(),
            "b",
            r#"
                id = "com.example.b"
                name = "B"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        reg.set_enabled("com.example.b", false);
        let snap = reg.enabled_state();
        assert_eq!(snap.get("com.example.a"), Some(true));
        assert_eq!(snap.get("com.example.b"), Some(false));
    }

    // -------- v2.0.0 Workshop runtime tests --------

    /// Backward-compatibility guard: a declarative-only plugin must
    /// load with `script_bytes = None`. Loader must not read or hash
    /// anything in this case.
    #[test]
    fn declarative_plugin_loads_with_no_script_bytes() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "decl",
            r#"
                id = "com.example.decl"
                name = "Decl"
                version = "0.1.0"
                slab_compat = ">=1.3.0"
            "#,
        );
        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugin = reg.get("com.example.decl").unwrap();
        assert!(plugin.script_bytes.is_none());
        assert!(
            plugin.is_active(),
            "declarative plugin must still be active"
        );
    }

    #[test]
    fn script_load_verifies_sha256_match() {
        let tmp = TempDir::new().unwrap();
        let script_body = "console.log('hi from workshop');";
        let (dir, sha) = write_runtime_plugin(tmp.path(), "ok", script_body);
        let toml = format!(
            r#"
                id = "com.example.ok"
                name = "Ok"
                version = "0.2.0"
                slab_compat = ">=2.0.0"

                [runtime]
                entry = "script.js"
                sha256 = "{sha}"
            "#
        );
        fs::write(dir.join("plugin.toml"), toml).unwrap();

        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugin = reg.get("com.example.ok").unwrap();
        assert!(plugin.is_active());
        assert_eq!(plugin.error, None);
        assert_eq!(
            plugin.script_bytes.as_deref(),
            Some(script_body.as_bytes()),
            "script bytes must be attached when hash matches"
        );
    }

    #[test]
    fn script_load_rejects_sha256_mismatch() {
        let tmp = TempDir::new().unwrap();
        let (dir, _real_sha) = write_runtime_plugin(tmp.path(), "bad", "let answer = 42;");
        // Pin a hash that does NOT match the real one.
        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        let toml = format!(
            r#"
                id = "com.example.bad"
                name = "Bad"
                version = "0.2.0"
                slab_compat = ">=2.0.0"

                [runtime]
                entry = "script.js"
                sha256 = "{wrong}"
            "#
        );
        fs::write(dir.join("plugin.toml"), toml).unwrap();

        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugin = reg.get("com.example.bad").unwrap();
        assert!(
            !plugin.is_active(),
            "mismatched-hash plugin must be inactive"
        );
        assert!(plugin.manifest.is_none());
        assert!(plugin.script_bytes.is_none());
        let err = plugin.error.as_deref().unwrap_or("");
        assert!(err.contains("sha256 mismatch"), "got: {err}");
        assert!(
            err.contains(wrong),
            "error should name pinned hash, got: {err}"
        );
    }

    #[test]
    fn script_load_handles_missing_script_file() {
        let tmp = TempDir::new().unwrap();
        // Plugin dir exists but no script.js.
        let dir = tmp.path().join("missing");
        fs::create_dir_all(&dir).unwrap();
        let toml = r#"
            id = "com.example.missing"
            name = "Missing"
            version = "0.2.0"
            slab_compat = ">=2.0.0"

            [runtime]
            entry = "script.js"
            sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        "#;
        fs::write(dir.join("plugin.toml"), toml).unwrap();

        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugin = reg.get("com.example.missing").unwrap();
        assert!(!plugin.is_active());
        let err = plugin.error.as_deref().unwrap_or("");
        assert!(err.contains("could not be read"), "got: {err}");
    }

    #[test]
    fn script_load_succeeds_with_empty_script() {
        // Edge case: empty file. SHA-256 of empty is well-known:
        // e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let tmp = TempDir::new().unwrap();
        let (dir, sha) = write_runtime_plugin(tmp.path(), "empty", "");
        assert_eq!(
            sha,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let toml = format!(
            r#"
                id = "com.example.empty"
                name = "Empty"
                version = "0.2.0"
                slab_compat = ">=2.0.0"

                [runtime]
                entry = "script.js"
                sha256 = "{sha}"
            "#
        );
        fs::write(dir.join("plugin.toml"), toml).unwrap();

        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugin = reg.get("com.example.empty").unwrap();
        assert!(plugin.is_active());
        assert_eq!(plugin.script_bytes.as_deref(), Some(&[][..]));
    }

    #[test]
    fn hex_sha256_matches_known_vector() {
        // "" -> well-known empty digest
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // "abc" -> NIST FIPS 180-2 test vector
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
