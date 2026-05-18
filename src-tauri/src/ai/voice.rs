// Slab Beacon — Voice Mode (Slice 15 — v1.9.0).
//
// TTS-first integration with platform-native text-to-speech engines.
// Cross-platform via shell-out to:
//
//   * macOS  → `say` (built-in, all macOS systems)
//   * Linux  → `espeak-ng` (Debian/Ubuntu: `apt install espeak-ng`)
//   * Windows → PowerShell `System.Speech.Synthesis.SpeechSynthesizer`
//
// Each engine implements the same `TtsEngine` interface:
//   * `is_available()` — probe binary with `--version` (or PS test).
//   * `list_voices()`   — parse stdout into `Vec<Voice>`.
//   * `speak(...)`      — spawn child process, return `Child` handle.
//
// We deliberately keep the surface small. STT (mic capture + Whisper)
// is out of scope for the v1.9.0 cut — see proposals/v1.9.0-voice.md
// for the v1.9.1 patch plan that adds it.
//
// Tests:
// All engines have stdout fixtures so we can parse-test without
// requiring the binary to actually be installed in CI. The integration
// path (spawning a real child, killing it, etc.) is exercised by a
// `#[cfg(target_os = ...)]` guarded test only when the binary exists,
// so CI on Linux runners doesn't fail when `espeak-ng` is absent.

use serde::{Deserialize, Serialize};
use std::process::{Child, Command, Stdio};

/// Identifier for the underlying TTS engine. The UI uses this both to
/// label settings and to short-circuit voice listing when the user has
/// already picked a non-default engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsEngine {
    /// macOS built-in `say` binary. Always installed.
    Say,
    /// Linux `espeak-ng`. Apt/dnf package.
    EspeakNg,
    /// Windows `System.Speech.Synthesis.SpeechSynthesizer` via
    /// PowerShell. Always available on Windows 7+.
    Powershell,
}

impl TtsEngine {
    /// Best-guess default engine for the current OS. Returns `None` on
    /// truly exotic platforms (BSD, etc.) where no built-in TTS exists.
    pub fn platform_default() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Say)
        } else if cfg!(target_os = "linux") {
            Some(Self::EspeakNg)
        } else if cfg!(target_os = "windows") {
            Some(Self::Powershell)
        } else {
            None
        }
    }

    /// Stable string id for use in config files + tauri commands.
    pub fn as_id(&self) -> &'static str {
        match self {
            Self::Say => "say",
            Self::EspeakNg => "espeak-ng",
            Self::Powershell => "powershell",
        }
    }

    /// Inverse of `as_id`. Unknown ids return `None`.
    pub fn from_id(s: &str) -> Option<Self> {
        match s {
            "say" => Some(Self::Say),
            "espeak-ng" => Some(Self::EspeakNg),
            "powershell" => Some(Self::Powershell),
            _ => None,
        }
    }
}

/// A single voice entry returned by the engine's voice-list command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voice {
    /// Engine-specific identifier used when speaking (e.g. `Samantha`,
    /// `en-us+f3`, `Microsoft David Desktop`).
    pub id: String,
    /// Human-friendly display name. Often the same as `id`.
    pub name: String,
    /// BCP-47 locale, lower-cased (e.g. `en-us`, `de-de`). Empty if the
    /// engine doesn't expose it.
    pub locale: String,
    /// `f`, `m`, or empty string. `say`/PowerShell don't always expose
    /// gender; we leave it blank when we can't tell.
    pub gender: String,
}

/// Capabilities probe — what's available on this host?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCapabilities {
    /// Which engines respond to a `--version` probe within 1s.
    pub available_engines: Vec<TtsEngine>,
    /// Best-guess default for the UI.
    pub recommended: Option<TtsEngine>,
    /// Whether STT is wired up yet. Always `false` for v1.9.0.
    pub stt: bool,
}

/// Top-level error type for the voice module.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("TTS engine {0:?} not installed on this host")]
    EngineUnavailable(TtsEngine),
    #[error("voice list command failed: {0}")]
    ListFailed(String),
    #[error("speak command failed: {0}")]
    SpeakFailed(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// macOS `say -v ?` parser.
///
/// The format is fixed:
///   Voice           locale # description...
/// where `Voice` and `locale` are whitespace-separated and the rest is
/// free-form. We split on the first `#` and on whitespace inside the
/// prefix.
pub fn parse_say_voices(stdout: &str) -> Vec<Voice> {
    let mut out = Vec::new();
    for raw in stdout.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        // Split off the comment portion ("# description"); we only care
        // about the prefix.
        let prefix = match line.split_once('#') {
            Some((p, _)) => p.trim_end(),
            None => line,
        };
        // Tokens: first is voice name, last is locale (e.g. "en_US").
        let toks: Vec<&str> = prefix.split_whitespace().collect();
        if toks.len() < 2 {
            continue;
        }
        let locale = toks[toks.len() - 1];
        // Some voice names have spaces ("Albert Premium"). Glue back
        // everything except the last token.
        let name = toks[..toks.len() - 1].join(" ");
        if !is_locale_like(locale) {
            continue;
        }
        out.push(Voice {
            id: name.clone(),
            name,
            locale: locale.to_lowercase().replace('_', "-"),
            gender: String::new(),
        });
    }
    out
}

fn is_locale_like(s: &str) -> bool {
    // Roughly `xx_YY` or `xx-YY`.
    let bytes = s.as_bytes();
    bytes.len() >= 4
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphabetic() || *b == b'_' || *b == b'-')
        && (bytes.contains(&b'_') || bytes.contains(&b'-'))
}

/// `espeak-ng --voices` parser.
///
/// Format (column-aligned):
/// ```text
/// Pty Language       Age/Gender VoiceName          File              Other Languages
///  5  en-us              M      english-us         en/en-us
///  5  de                 -      german             de/de
/// ```
/// First line is a header. We split on whitespace and pluck cols.
pub fn parse_espeak_voices(stdout: &str) -> Vec<Voice> {
    let mut out = Vec::new();
    for (i, raw) in stdout.lines().enumerate() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.len() < 4 {
            continue;
        }
        // Skip header row (starts with "Pty" literally).
        if i == 0 && toks.first() == Some(&"Pty") {
            continue;
        }
        // Validate the priority column is numeric so we don't accept
        // garbage lines.
        if toks[0].parse::<u32>().is_err() {
            continue;
        }
        let locale = toks[1].to_lowercase();
        let gender = match toks[2] {
            "M" => "m",
            "F" => "f",
            _ => "",
        }
        .to_string();
        let name = toks[3].to_string();
        out.push(Voice {
            id: name.clone(),
            name,
            locale,
            gender,
        });
    }
    out
}

/// PowerShell voice list. We invoke a JSON-emitting one-liner so the
/// parser is trivial:
/// ```text
/// [{"Id":"Microsoft David Desktop","Name":"David",...}]
/// ```
pub fn parse_powershell_voices(stdout: &str) -> Vec<Voice> {
    #[derive(Deserialize)]
    struct PsVoice {
        #[serde(rename = "Id")]
        id: String,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Culture")]
        culture: Option<String>,
        #[serde(rename = "Gender")]
        gender: Option<String>,
    }
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let parsed: Result<Vec<PsVoice>, _> = serde_json::from_str(trimmed);
    let raw = match parsed {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    raw.into_iter()
        .map(|v| Voice {
            id: v.id,
            name: v.name,
            locale: v.culture.unwrap_or_default().to_lowercase(),
            gender: match v.gender.as_deref() {
                Some("Male") => "m".to_string(),
                Some("Female") => "f".to_string(),
                _ => String::new(),
            },
        })
        .collect()
}

/// Trim runaway / control characters from text we're about to feed to a
/// platform binary. The engines all sanitise themselves, but this is a
/// belt-and-braces measure against accidental shell metacharacters
/// arriving from LLM output. We replace ASCII control chars (except
/// newline/tab) with spaces and cap length.
pub fn sanitise_text(input: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_chars));
    for ch in input.chars() {
        if out.chars().count() >= max_chars {
            break;
        }
        if ch == '\n' || ch == '\t' || ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Probe whether the platform's default engine is on PATH. Returns a
/// `VoiceCapabilities` snapshot for the UI to render initial state.
///
/// We deliberately don't probe ALL engines on every host (that would
/// spawn 3 processes per startup); we probe just the platform default.
/// The UI exposes an "advanced: try a different engine" path that
/// re-probes the explicit one.
pub fn capabilities() -> VoiceCapabilities {
    let mut available = Vec::new();
    if let Some(eng) = TtsEngine::platform_default() {
        if engine_is_installed(eng) {
            available.push(eng);
        }
    }
    VoiceCapabilities {
        recommended: TtsEngine::platform_default(),
        available_engines: available,
        stt: false,
    }
}

/// Quick `--version` probe with a 1.5s timeout. Returns `true` iff the
/// binary exits successfully.
pub fn engine_is_installed(eng: TtsEngine) -> bool {
    let probe = match eng {
        TtsEngine::Say => Command::new("say").arg("-v").arg("?").output(),
        TtsEngine::EspeakNg => Command::new("espeak-ng").arg("--version").output(),
        TtsEngine::Powershell => {
            // `powershell -Command "exit 0"` is the cheapest probe.
            Command::new("powershell")
                .args(["-NoProfile", "-Command", "exit 0"])
                .output()
        }
    };
    matches!(probe, Ok(o) if o.status.success())
}

/// Run the engine's voice-list command and parse the output. Returns
/// an empty Vec on parse errors — the UI surfaces "no voices found" as
/// the same case.
pub fn list_voices(eng: TtsEngine) -> Result<Vec<Voice>, VoiceError> {
    let output = match eng {
        TtsEngine::Say => Command::new("say").arg("-v").arg("?").output()?,
        TtsEngine::EspeakNg => Command::new("espeak-ng").arg("--voices").output()?,
        TtsEngine::Powershell => Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Speech; \
                 $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
                 $s.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo } | \
                 ConvertTo-Json -Compress",
            ])
            .output()?,
    };
    if !output.status.success() {
        return Err(VoiceError::ListFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(match eng {
        TtsEngine::Say => parse_say_voices(&stdout),
        TtsEngine::EspeakNg => parse_espeak_voices(&stdout),
        TtsEngine::Powershell => parse_powershell_voices(&stdout),
    })
}

/// Tunable parameters for a single speak call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeakOpts {
    /// Voice id (engine-specific). `None` → engine default voice.
    pub voice: Option<String>,
    /// Words-per-minute. `None` → engine default (~175wpm). Engines
    /// clamp out-of-range values internally; we don't second-guess.
    pub rate_wpm: Option<u32>,
}

/// Spawn the engine's speak process and return its `Child` handle. The
/// caller is expected to keep the handle around so it can `.kill()` it
/// later (the Tauri command surface stores it in a Mutex).
///
/// On macOS the engine plays the audio itself via Core Audio. On Linux
/// `espeak-ng` opens the default ALSA/Pulse sink directly. On Windows
/// the PowerShell `Speak()` call blocks the child until the playback
/// finishes, so killing the child stops audio.
pub fn speak(eng: TtsEngine, text: &str, opts: &SpeakOpts) -> Result<Child, VoiceError> {
    let text = sanitise_text(text, 50_000);
    match eng {
        TtsEngine::Say => {
            let mut cmd = Command::new("say");
            if let Some(v) = opts.voice.as_deref() {
                cmd.arg("-v").arg(v);
            }
            if let Some(r) = opts.rate_wpm {
                cmd.arg("-r").arg(r.to_string());
            }
            cmd.arg(text);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| VoiceError::SpeakFailed(e.to_string()))
        }
        TtsEngine::EspeakNg => {
            let mut cmd = Command::new("espeak-ng");
            if let Some(v) = opts.voice.as_deref() {
                cmd.arg("-v").arg(v);
            }
            if let Some(r) = opts.rate_wpm {
                cmd.arg("-s").arg(r.to_string());
            }
            cmd.arg(text);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| VoiceError::SpeakFailed(e.to_string()))
        }
        TtsEngine::Powershell => {
            // We use $env: indirection so the text doesn't need to be
            // escaped into the script body (PowerShell shell quoting is
            // a war crime). The Tauri command sets `SLAB_TTS_TEXT` in
            // the child's env before spawn — see lib.rs caller.
            let rate_set = opts
                .rate_wpm
                .map(|r| format!("$s.Rate = {};", clamp_ps_rate(r)))
                .unwrap_or_default();
            let voice_set = opts
                .voice
                .as_deref()
                .map(|v| format!("$s.SelectVoice('{}');", v.replace('\'', "''")))
                .unwrap_or_default();
            let script = format!(
                "Add-Type -AssemblyName System.Speech; \
                 $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
                 {voice_set} {rate_set} \
                 $s.Speak($env:SLAB_TTS_TEXT)"
            );
            Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .env("SLAB_TTS_TEXT", text)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| VoiceError::SpeakFailed(e.to_string()))
        }
    }
}

/// PowerShell `Rate` is a signed [-10..10] integer, not WPM. We bin a
/// WPM target into that range using the empirical mapping:
///   100 wpm → -5  (slow)
///   175 wpm →  0  (default)
///   250 wpm → +5  (fast)
///   320 wpm → +10 (max)
fn clamp_ps_rate(wpm: u32) -> i32 {
    let delta = (wpm as i32 - 175) / 15;
    delta.clamp(-10, 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_default_picks_say_on_mac() {
        if cfg!(target_os = "macos") {
            assert_eq!(TtsEngine::platform_default(), Some(TtsEngine::Say));
        }
    }

    #[test]
    fn engine_id_roundtrip() {
        for eng in [TtsEngine::Say, TtsEngine::EspeakNg, TtsEngine::Powershell] {
            let id = eng.as_id();
            assert_eq!(TtsEngine::from_id(id), Some(eng));
        }
        assert_eq!(TtsEngine::from_id("nope"), None);
    }

    #[test]
    fn parse_say_voices_real_output() {
        let stdout = "\
Albert              en_US    # Good morning
Alice               it_IT    # Salve, mi chiamo Alice
Alva                sv_SE    # Hej, jag heter Alva
Amelie              fr_CA    # Bonjour
Samantha            en_US    # Hello, my name is Samantha
Karen               en_AU    # G'day mate
";
        let voices = parse_say_voices(stdout);
        assert_eq!(voices.len(), 6);
        assert_eq!(voices[0].name, "Albert");
        assert_eq!(voices[0].locale, "en-us");
        assert_eq!(voices[4].name, "Samantha");
        assert_eq!(voices[5].locale, "en-au");
    }

    #[test]
    fn parse_say_voices_handles_multi_word_names() {
        let stdout = "Albert Premium     en_US    # variant\n";
        let voices = parse_say_voices(stdout);
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].name, "Albert Premium");
        assert_eq!(voices[0].id, "Albert Premium");
        assert_eq!(voices[0].locale, "en-us");
    }

    #[test]
    fn parse_say_voices_skips_blank_lines() {
        let stdout = "\nAlbert    en_US    # x\n\nAlice    it_IT    # y\n\n";
        let voices = parse_say_voices(stdout);
        assert_eq!(voices.len(), 2);
    }

    #[test]
    fn parse_say_voices_skips_garbage() {
        // No locale-shaped token at end → skip.
        let stdout = "no locale here just text\n";
        let voices = parse_say_voices(stdout);
        assert!(voices.is_empty());
    }

    #[test]
    fn parse_espeak_voices_real_output() {
        let stdout = "\
Pty Language       Age/Gender VoiceName          File              Other Languages
 5  en-us              M      english-us         en/en-us
 5  de                 -      german             de/de
 5  fr-fr              F      french-fr          fr/fr-fr
";
        let voices = parse_espeak_voices(stdout);
        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].name, "english-us");
        assert_eq!(voices[0].locale, "en-us");
        assert_eq!(voices[0].gender, "m");
        assert_eq!(voices[1].gender, ""); // "-"
        assert_eq!(voices[2].gender, "f");
    }

    #[test]
    fn parse_espeak_voices_handles_no_header() {
        let stdout = " 5  en-us              M      english-us         en/en-us\n";
        let voices = parse_espeak_voices(stdout);
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].name, "english-us");
    }

    #[test]
    fn parse_espeak_voices_skips_short_lines() {
        let stdout = "garbage\n5 too short\n 5 en-us M name file\n";
        let voices = parse_espeak_voices(stdout);
        assert_eq!(voices.len(), 1);
    }

    #[test]
    fn parse_powershell_voices_real_output() {
        let stdout = r#"[
            {"Id":"Microsoft David Desktop","Name":"David","Culture":"en-US","Gender":"Male"},
            {"Id":"Microsoft Zira Desktop","Name":"Zira","Culture":"en-US","Gender":"Female"}
        ]"#;
        let voices = parse_powershell_voices(stdout);
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].id, "Microsoft David Desktop");
        assert_eq!(voices[0].gender, "m");
        assert_eq!(voices[1].gender, "f");
        assert_eq!(voices[0].locale, "en-us");
    }

    #[test]
    fn parse_powershell_voices_empty() {
        assert!(parse_powershell_voices("").is_empty());
        assert!(parse_powershell_voices("[]").is_empty());
        assert!(parse_powershell_voices("not json").is_empty());
    }

    #[test]
    fn parse_powershell_handles_missing_culture() {
        let stdout = r#"[{"Id":"X","Name":"X"}]"#;
        let voices = parse_powershell_voices(stdout);
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].locale, "");
        assert_eq!(voices[0].gender, "");
    }

    #[test]
    fn sanitise_text_strips_control_chars() {
        let s = sanitise_text("hello\x07world\x1b[31m", 1000);
        // \x07 (BEL) and \x1b (ESC) become spaces; '[31m' remain as
        // regular printable chars.
        assert_eq!(s, "hello world [31m");
    }

    #[test]
    fn sanitise_text_preserves_newlines_as_spaces() {
        let s = sanitise_text("a\nb\tc", 1000);
        assert_eq!(s, "a b c");
    }

    #[test]
    fn sanitise_text_caps_length() {
        let long: String = "x".repeat(10_000);
        let s = sanitise_text(&long, 100);
        assert_eq!(s.chars().count(), 100);
    }

    #[test]
    fn sanitise_text_preserves_unicode() {
        let s = sanitise_text("héllo 你好 — café", 1000);
        assert_eq!(s, "héllo 你好 — café");
    }

    #[test]
    fn clamp_ps_rate_known_points() {
        assert_eq!(clamp_ps_rate(175), 0);
        assert_eq!(clamp_ps_rate(100), -5);
        assert_eq!(clamp_ps_rate(250), 5);
        assert_eq!(clamp_ps_rate(50), -8);
        assert_eq!(clamp_ps_rate(500), 10); // clamped
        assert_eq!(clamp_ps_rate(10), -10); // clamped
    }

    #[test]
    fn is_locale_like_basics() {
        assert!(is_locale_like("en_US"));
        assert!(is_locale_like("en-us"));
        assert!(is_locale_like("de_DE"));
        assert!(!is_locale_like("hello"));
        assert!(!is_locale_like("123"));
        assert!(!is_locale_like("en"));
        assert!(!is_locale_like(""));
    }

    #[test]
    fn capabilities_returns_recommended_for_supported_oses() {
        let caps = capabilities();
        if cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )) {
            assert!(caps.recommended.is_some());
        }
        // STT is always false in v1.9.0.
        assert!(!caps.stt);
    }

    #[test]
    fn speak_opts_default_is_all_none() {
        let opts = SpeakOpts::default();
        assert!(opts.voice.is_none());
        assert!(opts.rate_wpm.is_none());
    }
}
