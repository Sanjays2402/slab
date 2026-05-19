//! Slab Beacon — Voice Mode (STT — v1.9.1).
//!
//! Speech-to-Text counterpart to `ai::voice` (TTS). Provides:
//!
//!   * `SttEngine` — enum of supported engines. v1.9.1 ships
//!     whisper.cpp only; the shape is ready for additional providers
//!     (Vosk, Deepgram, etc.) in future patches.
//!   * `Transcript` — payload type returned by `stop_recording`.
//!   * `capabilities()` — cheap probe of `$PATH` for the engine
//!     binary and the OS-native recorder.
//!
//! All actual recording / transcription work lives in
//! `ai::stt_recorder` and `ai::stt_session` (later slices); this
//! module is purely types + capability sensing.

use serde::{Deserialize, Serialize};

/// Identifier for the underlying STT engine. The string form
/// (`as_id` / `from_id`) is the stable contract across config files,
/// the Tauri command surface, and the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttEngine {
    /// whisper.cpp — local, hermetic, GGUF model files.
    WhisperCpp,
}

impl SttEngine {
    /// Stable string identifier used everywhere except internal Rust
    /// code. Matches `#[serde(rename_all = "kebab-case")]`.
    pub fn as_id(self) -> &'static str {
        match self {
            Self::WhisperCpp => "whisper-cpp",
        }
    }

    /// Inverse of `as_id`. Returns `None` for unknown strings so the
    /// frontend can recover from a malformed config gracefully.
    pub fn from_id(s: &str) -> Option<Self> {
        match s {
            "whisper-cpp" => Some(Self::WhisperCpp),
            _ => None,
        }
    }

    /// Best-guess default engine for the current OS. v1.9.1 has only
    /// one engine so this is always `WhisperCpp`; the function exists
    /// so the choice is a single place to change later.
    pub fn platform_default() -> Option<Self> {
        Some(Self::WhisperCpp)
    }

    /// Name of the binary we expect to find on `$PATH`.
    /// whisper.cpp renamed `main` to `whisper-cli` in v1.7.x — we
    /// require the newer name.
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::WhisperCpp => "whisper-cli",
        }
    }
}

/// Result of a successful transcription. Returned from
/// `slab_beacon_voice_stt_stop`. Plain data; no resource handles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transcript {
    /// Cleaned-up text — no whisper.cpp timestamps, no leading
    /// whitespace, multi-line segments joined with a single space.
    pub text: String,
    /// Detected ISO 639-1 language code (when the engine reports it).
    /// `None` for engines that don't expose language detection or for
    /// recordings too short to detect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Wall-clock recording duration in milliseconds (from `start`
    /// to `stop`). Useful for the UI to display "Recorded 3.2s".
    pub duration_ms: u64,
}

/// Per-engine capability probe result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SttEngineCapability {
    /// Matches `SttEngine::as_id()`.
    pub id: String,
    /// Whether the engine's binary was found on `$PATH`.
    pub installed: bool,
    /// Absolute path to the binary if found. `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
}

/// What the system as a whole can do for STT right now. Used by the
/// Settings panel to show "whisper-cli not installed" hints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SttCapabilities {
    /// One entry per known engine. Always at least `whisper-cpp`.
    pub engines: Vec<SttEngineCapability>,
    /// Whether SOME OS-native recorder (sox / arecord / PowerShell)
    /// is available. If false, recording cannot start regardless of
    /// engine.
    pub recorder_available: bool,
}

/// Probe each engine's binary on `$PATH`. Cheap (a few subprocess
/// stat calls) — okay to call on every settings-panel render.
pub fn capabilities() -> SttCapabilities {
    let whisper = which(SttEngine::WhisperCpp.binary_name());
    SttCapabilities {
        engines: vec![SttEngineCapability {
            id: SttEngine::WhisperCpp.as_id().into(),
            installed: whisper.is_some(),
            binary_path: whisper,
        }],
        recorder_available: recorder_binary().is_some(),
    }
}

/// Look up an executable on `$PATH`. Returns its absolute path string
/// if found, `None` otherwise. We deliberately don't use the `which`
/// crate to avoid the extra dependency — the logic is six lines.
///
/// On Windows we also check the `.exe` suffix because `PATH` entries
/// rarely include it.
pub(crate) fn which(name: &str) -> Option<String> {
    let path = std::env::var("PATH").ok()?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path.split(sep) {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        if cfg!(windows) {
            let exe = std::path::Path::new(dir).join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Pick the recorder for this OS:
///   * macOS  → `sox` (preferred) or `rec` (sox's alt name)
///   * Linux  → `arecord` (alsa-utils) or `sox`
///   * Windows → PowerShell (always present on Win7+)
///
/// Returns the absolute path or, on Windows, the literal string
/// `"powershell"` (we don't bother probing it — every supported
/// Windows version ships it).
pub(crate) fn recorder_binary() -> Option<String> {
    if cfg!(target_os = "macos") {
        which("sox").or_else(|| which("rec"))
    } else if cfg!(target_os = "linux") {
        which("arecord").or_else(|| which("sox"))
    } else if cfg!(target_os = "windows") {
        Some("powershell".into())
    } else {
        // BSD / illumos / etc. — no built-in.
        None
    }
}

// ── v1.9.2 Task 4: whisper.cpp model catalog ────────────────────────

/// One row in the whisper.cpp model picker. Surfaces both installed
/// on-disk models (under `$SLAB_MODELS_DIR` or `~/.slab/models/`) and
/// well-known built-in suggestions the user *could* download.
///
/// The frontend renders this in `BeaconVoicePanel` as a `<select>`;
/// installed entries are highlighted, missing entries link to the
/// whisper.cpp model download docs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WhisperModelInfo {
    /// Stable identifier used both as the dropdown value and on the
    /// `whisper-cli -m <id>` command line when the model is on-disk
    /// (we substitute `path` instead for absolute-pathed entries).
    pub id: String,
    /// Human-friendly label for the picker. Falls back to `id`.
    pub label: String,
    /// Absolute path to the `.bin` if we found it on disk. `None` for
    /// built-in suggestions that haven't been downloaded yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Convenience flag — true iff `path.is_some()`. Frontend filters
    /// the dropdown on this.
    pub installed: bool,
}

/// Built-in catalog of whisper.cpp models we know about. Kept short
/// because the picker is meant to be opinionated — the user can drop
/// any `.bin` into `~/.slab/models/` to add their own and it'll show
/// up alongside these.
const BUILTIN_MODELS: &[(&str, &str)] = &[
    ("tiny.en", "Tiny (English-only) — ~75 MB"),
    ("base.en", "Base (English-only) — ~140 MB"),
    ("small.en", "Small (English-only) — ~470 MB"),
];

/// Enumerate installed + suggested whisper.cpp models.
///
/// Lookup order:
///   1. `$SLAB_MODELS_DIR` if set and a readable directory.
///   2. `~/.slab/models/` otherwise.
///
/// Any `*.bin` or `ggml-*.bin` file in that directory becomes an
/// installed entry. The built-in suggestion list is then merged in,
/// upgraded to `installed: true` if a matching `.bin` was found
/// (matched by stem — `ggml-base.en.bin` → `base.en`).
///
/// This function is **cheap** (one readdir, no file content reads) —
/// safe to call on every Settings-panel render.
pub fn list_whisper_models() -> Vec<WhisperModelInfo> {
    let dir = resolve_models_dir();
    let mut on_disk: Vec<WhisperModelInfo> = Vec::new();

    if let Some(d) = dir.as_ref() {
        if let Ok(entries) = std::fs::read_dir(d) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let fname = match p.file_name().and_then(|s| s.to_str()) {
                    Some(s) => s,
                    None => continue,
                };
                if !fname.ends_with(".bin") {
                    continue;
                }
                // Strip leading "ggml-" prefix and trailing ".bin" so
                // "ggml-base.en.bin" → id "base.en". If the file
                // doesn't have the conventional prefix, use the full
                // stem.
                let stem = fname.trim_end_matches(".bin");
                let id = stem.strip_prefix("ggml-").unwrap_or(stem).to_string();
                let label = format!("{id} (installed)");
                on_disk.push(WhisperModelInfo {
                    id,
                    label,
                    path: Some(p.to_string_lossy().to_string()),
                    installed: true,
                });
            }
        }
    }

    // Merge with built-ins. If a built-in matches an on-disk id,
    // promote the on-disk entry's label to the friendlier built-in
    // string. Otherwise append the built-in as a not-yet-installed
    // suggestion.
    let mut out: Vec<WhisperModelInfo> = Vec::with_capacity(on_disk.len() + BUILTIN_MODELS.len());
    for (id, label) in BUILTIN_MODELS {
        if let Some(idx) = on_disk.iter().position(|m| m.id == *id) {
            let mut existing = on_disk.remove(idx);
            existing.label = (*label).to_string();
            out.push(existing);
        } else {
            out.push(WhisperModelInfo {
                id: (*id).to_string(),
                label: (*label).to_string(),
                path: None,
                installed: false,
            });
        }
    }
    // Append any user-supplied on-disk models that didn't match a
    // built-in (e.g. "medium.en", "large-v3", custom-trained).
    out.extend(on_disk.into_iter());
    out
}

/// Resolve the directory we scan for `.bin` files. Returns `None`
/// only if we can't determine a home directory (very unusual).
fn resolve_models_dir() -> Option<std::path::PathBuf> {
    if let Ok(custom) = std::env::var("SLAB_MODELS_DIR") {
        if !custom.is_empty() {
            return Some(std::path::PathBuf::from(custom));
        }
    }
    // `~/.slab/models/`.
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())?;
    Some(std::path::PathBuf::from(home).join(".slab").join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_id_roundtrip() {
        assert_eq!(SttEngine::WhisperCpp.as_id(), "whisper-cpp");
        assert_eq!(
            SttEngine::from_id("whisper-cpp"),
            Some(SttEngine::WhisperCpp)
        );
        assert_eq!(SttEngine::from_id("nope"), None);
        assert_eq!(SttEngine::from_id(""), None);
    }

    #[test]
    fn platform_default_is_whisper_cpp() {
        assert_eq!(SttEngine::platform_default(), Some(SttEngine::WhisperCpp));
    }

    #[test]
    fn binary_name_is_whisper_cli() {
        assert_eq!(SttEngine::WhisperCpp.binary_name(), "whisper-cli");
    }

    #[test]
    fn transcript_serialises_round_trip() {
        let t = Transcript {
            text: "hello world".into(),
            language: Some("en".into()),
            duration_ms: 1234,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"text\":\"hello world\""));
        assert!(json.contains("\"language\":\"en\""));
        assert!(json.contains("\"duration_ms\":1234"));

        let back: Transcript = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn transcript_omits_none_language() {
        let t = Transcript {
            text: "x".into(),
            language: None,
            duration_ms: 0,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("language"));
    }

    #[test]
    fn capabilities_returns_known_shape() {
        let caps = capabilities();
        // Always exactly one engine entry in v1.9.1.
        assert_eq!(caps.engines.len(), 1);
        assert_eq!(caps.engines[0].id, "whisper-cpp");
        // installed/recorder_available are environment-dependent;
        // just assert they're sensible bools.
        let _ = caps.engines[0].installed;
        let _ = caps.recorder_available;
    }

    #[test]
    fn which_finds_nothing_for_obvious_garbage() {
        // Pick a name that definitely doesn't exist anywhere.
        assert!(which("__slab_v191_definitely_not_a_real_binary__").is_none());
    }

    #[test]
    fn which_finds_sh_on_unix() {
        // /bin/sh exists on every unix Slab targets. Don't run this
        // on Windows.
        if !cfg!(windows) {
            // Temporarily ensure /bin is in PATH for the test.
            let orig = std::env::var("PATH").unwrap_or_default();
            let new = if orig.split(':').any(|p| p == "/bin") {
                orig.clone()
            } else {
                format!("/bin:{orig}")
            };
            std::env::set_var("PATH", &new);
            let found = which("sh");
            std::env::set_var("PATH", &orig);
            assert!(
                found.is_some(),
                "expected to find sh in PATH; got None. PATH was {new}"
            );
        }
    }

    #[test]
    fn engine_capability_serialises_without_binary_path() {
        let cap = SttEngineCapability {
            id: "whisper-cpp".into(),
            installed: false,
            binary_path: None,
        };
        let json = serde_json::to_string(&cap).unwrap();
        assert!(!json.contains("binary_path"));
    }

    // ── v1.9.2 Task 4: whisper model catalog ────────────────────────

    /// Shared lock for env-touching tests (`SLAB_MODELS_DIR`).
    static MODELS_DIR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// With an empty models dir we should still get the three
    /// built-in suggestions — `tiny.en`, `base.en`, `small.en` — each
    /// marked `installed: false`.
    #[test]
    fn list_whisper_models_returns_builtins_when_dir_empty() {
        let _g = MODELS_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!(
            "slab-models-empty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).expect("mkdir tmp models dir");

        let prev = std::env::var("SLAB_MODELS_DIR").ok();
        std::env::set_var("SLAB_MODELS_DIR", &tmp);

        let models = list_whisper_models();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"tiny.en"));
        assert!(ids.contains(&"base.en"));
        assert!(ids.contains(&"small.en"));
        // None installed — directory is empty.
        for m in &models {
            assert!(!m.installed, "{} should not be installed", m.id);
            assert!(m.path.is_none());
        }

        match prev {
            Some(v) => std::env::set_var("SLAB_MODELS_DIR", v),
            None => std::env::remove_var("SLAB_MODELS_DIR"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A `.bin` on disk should appear as installed, with its absolute
    /// path. We touch `ggml-base.en.bin` and expect the matching
    /// built-in `base.en` slot to be marked installed.
    #[test]
    fn list_whisper_models_picks_up_on_disk_files() {
        let _g = MODELS_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!(
            "slab-models-ondisk-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).expect("mkdir tmp models dir");
        // Drop a fake "base.en" + a non-builtin "medium.en" + a
        // non-bin file that should be ignored.
        std::fs::write(tmp.join("ggml-base.en.bin"), b"fake").unwrap();
        std::fs::write(tmp.join("ggml-medium.en.bin"), b"fake").unwrap();
        std::fs::write(tmp.join("README.md"), b"hi").unwrap();

        let prev = std::env::var("SLAB_MODELS_DIR").ok();
        std::env::set_var("SLAB_MODELS_DIR", &tmp);

        let models = list_whisper_models();
        let base = models
            .iter()
            .find(|m| m.id == "base.en")
            .expect("base.en should appear");
        assert!(base.installed, "base.en should be marked installed");
        assert!(base.path.is_some(), "base.en should have a path");
        // tiny.en wasn't on disk → still listed but not installed.
        let tiny = models.iter().find(|m| m.id == "tiny.en").unwrap();
        assert!(!tiny.installed);
        // medium.en wasn't a builtin — comes from disk only.
        let medium = models
            .iter()
            .find(|m| m.id == "medium.en")
            .expect("medium.en (user-supplied) should be enumerated");
        assert!(medium.installed);
        // README.md (non-bin) should not have been picked up.
        assert!(!models.iter().any(|m| m.id.contains("README")));

        match prev {
            Some(v) => std::env::set_var("SLAB_MODELS_DIR", v),
            None => std::env::remove_var("SLAB_MODELS_DIR"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `WhisperModelInfo` should round-trip cleanly through JSON and
    /// omit `path` when it's `None`.
    #[test]
    fn whisper_model_info_serialises_round_trip() {
        let m = WhisperModelInfo {
            id: "base.en".into(),
            label: "Base".into(),
            path: Some("/tmp/ggml-base.en.bin".into()),
            installed: true,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"id\":\"base.en\""));
        assert!(json.contains("\"installed\":true"));
        let back: WhisperModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);

        let none_path = WhisperModelInfo {
            id: "tiny.en".into(),
            label: "Tiny".into(),
            path: None,
            installed: false,
        };
        let json2 = serde_json::to_string(&none_path).unwrap();
        assert!(!json2.contains("path"));
    }
}
