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
}
