// Beacon configuration.
//
// Lives at `~/.slab/config.toml`. Owns provider selection so the UI can
// switch between Ollama (default, local) and any OpenAI-compatible
// service without code changes.
//
// File schema:
//   [beacon]
//   provider = "ollama"            # or "openai"
//   chat_model = "llama3.2:3b"     # provider-specific model id
//   embed_model = "nomic-embed-text"
//   base_url = "http://localhost:11434"
//   api_key_env = "OPENAI_API_KEY" # name of env var holding the secret
//
// The API key is *never* written to the config file — only the name of
// the env var that holds it. This keeps `~/.slab/config.toml` shareable
// across machines without leaking credentials.
//
// Defaults are picked so a brand-new user with Ollama running locally
// gets a working Beacon experience with zero setup.

use super::ollama::OllamaProvider;
use super::openai_compat::OpenAiCompatibleProvider;
use super::{AiError, AiProvider};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Top-level config file. We wrap `BeaconConfig` in a parent struct so
/// future sections (UI prefs, telemetry opt-out, …) can land alongside
/// without breaking the `[beacon]` namespace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SlabConfig {
    #[serde(default)]
    pub beacon: BeaconConfig,
    /// UI preferences (theme, accent, density). Added in v1.0.0 "Glass".
    #[serde(default)]
    pub ui: UiConfig,
    /// Customizable keyboard shortcuts. Added in v1.0.0 "Glass" Slice 7.
    /// Absent in legacy configs → defaults reconstituted at materialise time.
    #[serde(default)]
    pub keymap: crate::keymap::KeymapConfig,
}

/// UI preferences. Persisted alongside Beacon settings in
/// `~/.slab/config.toml` under `[ui]`. Frontend reads on boot and
/// writes on every change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UiConfig {
    /// Light / dark / follow-system. Defaults to `auto`.
    #[serde(default)]
    pub theme: ThemeMode,
    /// Accent colour (Slab orange by default). User can swap to one of
    /// a handful of curated tints.
    #[serde(default)]
    pub accent: AccentColor,
    /// Visual density. `comfortable` is the v0.x default; `compact`
    /// shrinks spacing for power users / small screens.
    #[serde(default)]
    pub density: Density,
    /// True once the user has dismissed the first-launch onboarding tour.
    /// Defaults to `false` so brand-new installs see the walkthrough.
    /// Users can re-trigger via the Command Palette → "Show onboarding tour".
    #[serde(default)]
    pub onboarded: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the host OS appearance. Default for new users.
    #[default]
    Auto,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccentColor {
    /// Slab orange — the historical default.
    #[default]
    Orange,
    Blue,
    Purple,
    Green,
    Pink,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// The Beacon-specific configuration block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeaconConfig {
    /// Which provider implementation to instantiate.
    #[serde(default)]
    pub provider: ProviderKind,
    /// Provider-specific chat model identifier. `None` → provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_model: Option<String>,
    /// Provider-specific embedding model identifier. `None` → provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_model: Option<String>,
    /// HTTP base URL. For Ollama: usually `http://localhost:11434`.
    /// For OpenAI itself: `https://api.openai.com/v1`. `None` → provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Name of the env variable that holds the API key (e.g.
    /// `OPENAI_API_KEY`). Only used by the OpenAI-compatible provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Voice Mode settings (v1.9.0 Slice 15). Defaults to an empty
    /// `VoiceConfig` so existing configs deserialise cleanly.
    #[serde(default, skip_serializing_if = "VoiceConfig::is_empty")]
    pub voice: VoiceConfig,
}

/// Beacon Voice Mode persisted settings. All fields are optional so the
/// frontend can save partial configurations as the user fills them in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VoiceConfig {
    /// Engine id from `ai::voice::TtsEngine::as_id()`: `"say"`,
    /// `"espeak-ng"`, or `"powershell"`. `None` → use platform default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Engine-specific voice id (e.g. `"Samantha"`, `"en-us+f3"`).
    /// `None` → use engine's default voice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// Words-per-minute. Clamped by the engine; `None` → engine default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_wpm: Option<u32>,
    /// If true, Beacon chat responses are auto-spoken when they finish
    /// streaming. Off by default — the user toggles this from the panel.
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_speak_replies: bool,

    // ---- v1.9.2 — Listen (STT) settings ----------------------------
    // All optional so v1.9.1 configs still deserialise without a
    // [beacon.voice] migration. Surfaced in BeaconVoicePanel's
    // "Listen (STT)" fieldset.
    /// STT engine id (`SttEngine::as_id()`). v1.9.2 still only ships
    /// `"whisper-cpp"`; field exists so future engines slot in without
    /// a config-schema bump.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_engine: Option<String>,
    /// whisper.cpp model name OR absolute path to a `.bin` / `.gguf`
    /// file. When set, takes precedence over `$WHISPER_MODEL`. Bare
    /// names like `"base.en"` are passed straight to whisper-cli's
    /// `-m` flag — whisper.cpp looks up `models/base.en.bin` relative
    /// to its own install root in that case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_model: Option<String>,
    /// Phrase that, if found at the END of a transcript, triggers
    /// auto-send. `None` means no auto-send. Case-insensitive match.
    /// Trailing whitespace + punctuation around the trigger is
    /// stripped before comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_trigger_word: Option<String>,
    /// Master switch for the auto-send feature. Even if a trigger
    /// word is configured, no auto-send happens unless this is true.
    /// Lets users keep their phrase parked while temporarily disabling
    /// the behaviour.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stt_send_on_trigger: bool,
}

impl VoiceConfig {
    /// Used by `skip_serializing_if` to omit empty voice blocks from
    /// config files. Saves on disk noise for users who never enable
    /// voice mode.
    pub fn is_empty(&self) -> bool {
        self.engine.is_none()
            && self.voice.is_none()
            && self.rate_wpm.is_none()
            && !self.auto_speak_replies
            && self.stt_engine.is_none()
            && self.stt_model.is_none()
            && self.stt_trigger_word.is_none()
            && !self.stt_send_on_trigger
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// Default. Local Ollama daemon. No setup beyond `ollama pull`.
    #[default]
    Ollama,
    /// Any OpenAI-compatible HTTP endpoint (OpenAI, Copilot proxy, llama.cpp `server`, …).
    Openai,
}

/// Return the absolute path Slab expects to find its config at.
/// Honours `$SLAB_CONFIG_DIR` so tests can redirect without touching
/// the user's real home directory.
pub fn config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("SLAB_CONFIG_DIR") {
        return PathBuf::from(dir).join("config.toml");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".slab").join("config.toml")
}

/// Load `~/.slab/config.toml`. If the file doesn't exist, return
/// `SlabConfig::default()` — the local-Ollama happy path.
///
/// Parse errors propagate as `AiError::InvalidResponse` so the UI can
/// surface them ("your config.toml has a typo on line 3").
pub fn load() -> Result<SlabConfig, AiError> {
    let path = config_path();
    load_from(&path)
}

/// Read config from a specific path. Exposed for tests.
pub fn load_from(path: &std::path::Path) -> Result<SlabConfig, AiError> {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s)
            .map_err(|e| AiError::InvalidResponse(format!("parsing {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SlabConfig::default()),
        Err(e) => Err(AiError::Network(format!("reading {}: {e}", path.display()))),
    }
}

/// Persist `cfg` to `~/.slab/config.toml`, creating the parent
/// directory if needed.
pub fn save(cfg: &SlabConfig) -> Result<(), AiError> {
    let path = config_path();
    save_to(&path, cfg)
}

/// Write config to a specific path. Exposed for tests.
pub fn save_to(path: &std::path::Path, cfg: &SlabConfig) -> Result<(), AiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AiError::Network(format!("mkdir {}: {e}", parent.display())))?;
    }
    let body = toml::to_string_pretty(cfg)
        .map_err(|e| AiError::InvalidResponse(format!("serialising config: {e}")))?;
    std::fs::write(path, body)
        .map_err(|e| AiError::Network(format!("writing {}: {e}", path.display())))?;
    Ok(())
}

/// Build a provider from a `BeaconConfig`. The returned trait object
/// can be sent across threads (the Tauri command surface needs `Send`).
///
/// Errors only when the OpenAI-compatible provider is selected but the
/// configured `api_key_env` is missing or empty in the environment.
pub fn make_provider(cfg: &BeaconConfig) -> Result<Arc<dyn AiProvider>, AiError> {
    match cfg.provider {
        ProviderKind::Ollama => {
            let base = cfg.base_url.as_deref().unwrap_or("http://localhost:11434");
            let mut p = OllamaProvider::with_base_url(base);
            if let Some(m) = cfg.chat_model.as_deref() {
                p = p.with_chat_model(m);
            }
            if let Some(m) = cfg.embed_model.as_deref() {
                p = p.with_embed_model(m);
            }
            Ok(Arc::new(p))
        }
        ProviderKind::Openai => {
            let base = cfg
                .base_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1");
            let env_name = cfg.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
            let key = std::env::var(env_name).map_err(|_| {
                AiError::ProviderUnavailable(format!(
                    "missing API key: env var ${env_name} is unset. Set it in your shell, \
                     or change `api_key_env` in ~/.slab/config.toml."
                ))
            })?;
            if key.trim().is_empty() {
                return Err(AiError::ProviderUnavailable(format!(
                    "env var ${env_name} is empty"
                )));
            }
            let mut p = OpenAiCompatibleProvider::new(base, key);
            if let Some(m) = cfg.chat_model.as_deref() {
                p = p.with_chat_model(m);
            }
            if let Some(m) = cfg.embed_model.as_deref() {
                p = p.with_embed_model(m);
            }
            Ok(Arc::new(p))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Default config = local Ollama with no overrides. Most users will
    /// never edit this file; defaults must be useful.
    #[test]
    fn default_is_ollama_local() {
        let cfg = SlabConfig::default();
        assert_eq!(cfg.beacon.provider, ProviderKind::Ollama);
        assert!(cfg.beacon.chat_model.is_none());
        assert!(cfg.beacon.base_url.is_none());
    }

    /// Round-trip TOML so the file we write is the file we can read.
    #[test]
    fn save_then_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = SlabConfig {
            beacon: BeaconConfig {
                provider: ProviderKind::Openai,
                chat_model: Some("gpt-4o-mini".into()),
                embed_model: Some("text-embedding-3-small".into()),
                base_url: Some("https://api.openai.com/v1".into()),
                api_key_env: Some("OPENAI_API_KEY".into()),
                voice: VoiceConfig::default(),
            },
            ui: UiConfig::default(),
            keymap: Default::default(),
        };
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded, cfg);
        // Sanity-check: the on-disk file contains the canonical key
        // names (so users can hand-edit).
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("provider = \"openai\""), "got:\n{body}");
        assert!(
            body.contains("api_key_env = \"OPENAI_API_KEY\""),
            "got:\n{body}"
        );
    }

    /// Missing file → defaults, not an error. The cron path:
    /// fresh-install user opens Slab → Beacon "just works" with Ollama.
    #[test]
    fn load_missing_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("does-not-exist.toml");
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg, SlabConfig::default());
    }

    /// Malformed TOML → InvalidResponse with the path baked in so the
    /// UI can render "open this file".
    #[test]
    fn load_malformed_toml_returns_invalid_response() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not = = valid toml [[").unwrap();
        let err = load_from(&path).unwrap_err();
        match err {
            AiError::InvalidResponse(m) => {
                assert!(m.contains("config.toml"), "expected path in error, got {m}");
            }
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    /// make_provider for default config → Ollama provider whose
    /// `name()` is `"ollama"`. We can't introspect base_url from the
    /// trait, but the name + a working construction is enough signal.
    #[test]
    fn make_provider_default_builds_ollama() {
        let p = make_provider(&BeaconConfig::default()).unwrap();
        assert_eq!(p.name(), "ollama");
    }

    /// make_provider for OpenAI with missing env var → ProviderUnavailable.
    /// The error message must name the env var so the user knows what
    /// to set.
    #[test]
    fn make_provider_openai_missing_key_errors() {
        // Pick a name extremely unlikely to be set in any CI environment.
        let env_name = "SLAB_TEST_NEVER_SET_KEY_XYZZY_123";
        // Defensive: ensure it really isn't set.
        std::env::remove_var(env_name);
        let cfg = BeaconConfig {
            provider: ProviderKind::Openai,
            api_key_env: Some(env_name.to_string()),
            ..Default::default()
        };
        let err = match make_provider(&cfg) {
            Ok(_) => panic!("expected ProviderUnavailable, got Ok"),
            Err(e) => e,
        };
        match err {
            AiError::ProviderUnavailable(m) => {
                assert!(m.contains(env_name), "expected env name in error, got {m}");
            }
            other => panic!("expected ProviderUnavailable, got {other:?}"),
        }
    }

    /// make_provider for OpenAI with env var set → returns the
    /// openai-compatible provider.
    #[test]
    fn make_provider_openai_with_key_builds_provider() {
        let env_name = "SLAB_TEST_KEY_PRESENT_XYZZY_456";
        std::env::set_var(env_name, "sk-fake-test-value");
        let cfg = BeaconConfig {
            provider: ProviderKind::Openai,
            api_key_env: Some(env_name.to_string()),
            base_url: Some("https://example.test/v1".to_string()),
            ..Default::default()
        };
        let p = make_provider(&cfg).unwrap();
        assert_eq!(p.name(), "openai-compatible");
        std::env::remove_var(env_name);
    }

    /// config_path honours `$SLAB_CONFIG_DIR` so tests / CI can pin a
    /// scratch location.
    #[test]
    fn config_path_respects_env_override() {
        std::env::set_var("SLAB_CONFIG_DIR", "/tmp/slab-test-cfg-7777");
        let p = config_path();
        assert_eq!(p, PathBuf::from("/tmp/slab-test-cfg-7777/config.toml"));
        std::env::remove_var("SLAB_CONFIG_DIR");
    }

    // ---------- UI prefs (v1.0.0 Glass) ----------

    /// Default UI config = Auto theme, Slab orange, comfortable density,
    /// onboarding tour pending.
    /// Brand new install must look identical to v0.x users on first launch.
    #[test]
    fn default_ui_config() {
        let cfg = SlabConfig::default();
        assert_eq!(cfg.ui.theme, ThemeMode::Auto);
        assert_eq!(cfg.ui.accent, AccentColor::Orange);
        assert_eq!(cfg.ui.density, Density::Comfortable);
        assert!(
            !cfg.ui.onboarded,
            "new installs must see the onboarding tour"
        );
    }

    /// UI config round-trips through TOML with every variant.
    #[test]
    fn ui_config_roundtrip_all_variants() {
        let tmp = TempDir::new().unwrap();
        for theme in [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark] {
            for accent in [
                AccentColor::Orange,
                AccentColor::Blue,
                AccentColor::Purple,
                AccentColor::Green,
                AccentColor::Pink,
            ] {
                for density in [Density::Comfortable, Density::Compact] {
                    let path = tmp
                        .path()
                        .join(format!("config-{theme:?}-{accent:?}-{density:?}.toml"));
                    let cfg = SlabConfig {
                        beacon: BeaconConfig::default(),
                        ui: UiConfig {
                            theme,
                            accent,
                            density,
                            onboarded: false,
                        },
                        keymap: Default::default(),
                    };
                    save_to(&path, &cfg).unwrap();
                    let loaded = load_from(&path).unwrap();
                    assert_eq!(loaded, cfg, "mismatch for {theme:?}/{accent:?}/{density:?}");
                }
            }
        }
    }

    /// UI config block uses canonical lowercase variant names on disk
    /// so users can hand-edit `~/.slab/config.toml` easily.
    #[test]
    fn ui_config_serialises_lowercase() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = SlabConfig {
            beacon: BeaconConfig::default(),
            ui: UiConfig {
                theme: ThemeMode::Dark,
                accent: AccentColor::Blue,
                density: Density::Compact,
                onboarded: true,
            },
            keymap: Default::default(),
        };
        save_to(&path, &cfg).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("theme = \"dark\""), "got:\n{body}");
        assert!(body.contains("accent = \"blue\""), "got:\n{body}");
        assert!(body.contains("density = \"compact\""), "got:\n{body}");
        assert!(body.contains("onboarded = true"), "got:\n{body}");
    }

    /// `onboarded` defaults to false and round-trips both states. New
    /// installs must show the tour; users who dismissed must not see it
    /// reappear after an app restart.
    #[test]
    fn onboarded_defaults_false_and_round_trips() {
        let tmp = TempDir::new().unwrap();
        for flag in [false, true] {
            let path = tmp.path().join(format!("onb-{flag}.toml"));
            let cfg = SlabConfig {
                beacon: BeaconConfig::default(),
                ui: UiConfig {
                    onboarded: flag,
                    ..Default::default()
                },
                keymap: Default::default(),
            };
            save_to(&path, &cfg).unwrap();
            let loaded = load_from(&path).unwrap();
            assert_eq!(loaded.ui.onboarded, flag);
        }
        assert!(!UiConfig::default().onboarded);
    }

    /// Reading a v0.x config that pre-dates the `onboarded` field must
    /// yield `onboarded = false` (so upgrading users get the tour once).
    #[test]
    fn legacy_ui_section_without_onboarded_loads_false() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "[ui]\ntheme = \"dark\"\naccent = \"blue\"\ndensity = \"compact\"\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert!(!cfg.ui.onboarded);
        assert_eq!(cfg.ui.theme, ThemeMode::Dark);
    }

    /// Reading an older config.toml with no [ui] section must yield
    /// default UI prefs (forward-compat for v0.x users upgrading).
    #[test]
    fn legacy_config_without_ui_section_loads_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "[beacon]\nprovider = \"ollama\"\nchat_model = \"llama3.2:3b\"\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.ui, UiConfig::default());
        assert_eq!(cfg.beacon.chat_model.as_deref(), Some("llama3.2:3b"));
    }

    /// Writing a config with a [beacon]+[ui] block produces a file
    /// containing *both* sections, so subsequent edits aren't lossy.
    #[test]
    fn save_emits_both_beacon_and_ui_sections() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = SlabConfig {
            beacon: BeaconConfig {
                provider: ProviderKind::Openai,
                api_key_env: Some("OPENAI_API_KEY".into()),
                ..Default::default()
            },
            ui: UiConfig {
                theme: ThemeMode::Light,
                accent: AccentColor::Pink,
                density: Density::Compact,
                onboarded: true,
            },
            keymap: Default::default(),
        };
        save_to(&path, &cfg).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("[beacon]"), "got:\n{body}");
        assert!(body.contains("[ui]"), "got:\n{body}");
    }

    /// Unknown variant in TOML produces a parse error rather than
    /// silently falling back to default — protects users from typos.
    #[test]
    fn unknown_theme_variant_errors() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[ui]\ntheme = \"highcontrast\"\n").unwrap();
        let err = load_from(&path).unwrap_err();
        match err {
            AiError::InvalidResponse(_) => {}
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    // ─────────── Voice Mode config (v1.9.0 Slice 15) ───────────

    /// Default `VoiceConfig` is the "no voice mode configured" shape.
    #[test]
    fn voice_config_default_is_empty() {
        let v = VoiceConfig::default();
        assert!(v.is_empty());
        assert_eq!(v.engine, None);
        assert_eq!(v.voice, None);
        assert_eq!(v.rate_wpm, None);
        assert!(!v.auto_speak_replies);
    }

    /// is_empty() returns false the moment any field is set.
    #[test]
    fn voice_config_is_empty_flips_on_each_field() {
        let with_engine = VoiceConfig {
            engine: Some("say".into()),
            ..Default::default()
        };
        assert!(!with_engine.is_empty());
        let with_voice = VoiceConfig {
            voice: Some("Samantha".into()),
            ..Default::default()
        };
        assert!(!with_voice.is_empty());
        let with_rate = VoiceConfig {
            rate_wpm: Some(180),
            ..Default::default()
        };
        assert!(!with_rate.is_empty());
        let with_auto = VoiceConfig {
            auto_speak_replies: true,
            ..Default::default()
        };
        assert!(!with_auto.is_empty());
    }

    /// Empty voice config is omitted from saved TOML — keeps existing
    /// users' on-disk configs clean (no surprise lines after upgrade).
    #[test]
    fn empty_voice_config_omitted_from_toml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = SlabConfig::default();
        save_to(&path, &cfg).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(
            !body.contains("[beacon.voice]"),
            "empty voice should not be serialised; got:\n{body}"
        );
    }

    /// Populated voice config round-trips through TOML.
    #[test]
    fn voice_config_roundtrips() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = SlabConfig {
            beacon: BeaconConfig {
                voice: VoiceConfig {
                    engine: Some("say".into()),
                    voice: Some("Samantha".into()),
                    rate_wpm: Some(200),
                    auto_speak_replies: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.beacon.voice.engine.as_deref(), Some("say"));
        assert_eq!(loaded.beacon.voice.voice.as_deref(), Some("Samantha"));
        assert_eq!(loaded.beacon.voice.rate_wpm, Some(200));
        assert!(loaded.beacon.voice.auto_speak_replies);
    }

    /// Legacy v1.8.x config (no [beacon.voice] section) still loads
    /// cleanly and yields a default voice block.
    #[test]
    fn legacy_config_without_voice_section_loads_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "[beacon]\nprovider = \"ollama\"\nchat_model = \"llama3.2:3b\"\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert!(cfg.beacon.voice.is_empty());
    }

    // ---- v1.9.2 — Listen (STT) field tests --------------------------

    /// Default `VoiceConfig` must stay `is_empty()` even after adding
    /// the four STT fields. Guards against accidental serialisation of
    /// the new defaults to old users' configs.
    #[test]
    fn voice_config_stt_fields_default_empty() {
        let v = VoiceConfig::default();
        assert!(v.is_empty(), "default VoiceConfig must still be empty");
        assert_eq!(v.stt_engine, None);
        assert_eq!(v.stt_model, None);
        assert_eq!(v.stt_trigger_word, None);
        assert!(!v.stt_send_on_trigger);
    }

    /// Each STT field flips `is_empty()` to false in isolation. Mirrors
    /// `voice_config_is_empty_flips_on_each_field` for the v1.9.0
    /// fields.
    #[test]
    fn voice_config_stt_fields_flip_is_empty() {
        let v = VoiceConfig {
            stt_engine: Some("whisper-cpp".into()),
            ..Default::default()
        };
        assert!(!v.is_empty(), "stt_engine should flip is_empty");

        let v = VoiceConfig {
            stt_model: Some("base.en".into()),
            ..Default::default()
        };
        assert!(!v.is_empty(), "stt_model should flip is_empty");

        let v = VoiceConfig {
            stt_trigger_word: Some("send it".into()),
            ..Default::default()
        };
        assert!(!v.is_empty(), "stt_trigger_word should flip is_empty");

        let v = VoiceConfig {
            stt_send_on_trigger: true,
            ..Default::default()
        };
        assert!(!v.is_empty(), "stt_send_on_trigger should flip is_empty");
    }

    /// Populated STT fields round-trip through TOML alongside the
    /// existing TTS fields. Guards against typo'd serde renames.
    #[test]
    fn voice_config_stt_fields_roundtrip_toml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = SlabConfig {
            beacon: BeaconConfig {
                voice: VoiceConfig {
                    engine: Some("say".into()),
                    stt_engine: Some("whisper-cpp".into()),
                    stt_model: Some("base.en".into()),
                    stt_trigger_word: Some("go".into()),
                    stt_send_on_trigger: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(
            loaded.beacon.voice.stt_engine.as_deref(),
            Some("whisper-cpp")
        );
        assert_eq!(loaded.beacon.voice.stt_model.as_deref(), Some("base.en"));
        assert_eq!(loaded.beacon.voice.stt_trigger_word.as_deref(), Some("go"));
        assert!(loaded.beacon.voice.stt_send_on_trigger);
    }

    /// v1.9.1 configs (engine/voice/rate but no stt_* fields) must
    /// still deserialise cleanly. Guards the additive-field invariant.
    #[test]
    fn legacy_v191_voice_config_without_stt_fields_still_loads() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "[beacon.voice]\nengine = \"say\"\nvoice = \"Samantha\"\nrate_wpm = 200\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.beacon.voice.engine.as_deref(), Some("say"));
        assert_eq!(cfg.beacon.voice.stt_engine, None);
        assert_eq!(cfg.beacon.voice.stt_model, None);
        assert_eq!(cfg.beacon.voice.stt_trigger_word, None);
        assert!(!cfg.beacon.voice.stt_send_on_trigger);
    }
}
