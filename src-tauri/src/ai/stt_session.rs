//! Slab Beacon — Voice Mode (STT session — v1.9.1 Task 3).
//!
//! Single-slot start/stop session manager. Mirrors `VoiceSession`
//! (TTS) so the Tauri command surface looks identical from the
//! frontend's perspective.
//!
//! Lifecycle:
//!
//! ```text
//!     start()                        stop()
//! ┌─────────────┐               ┌──────────────────┐
//! │  spawn      │               │ kill recorder    │
//! │  recorder   │ ── recording ──►│ wait recorder    │
//! │  to *.wav   │               │ spawn whisper-cli│
//! └─────────────┘               │ on the WAV file  │
//!                               │ parse stdout     │
//!                               │ delete WAV       │
//!                               │ return Transcript│
//!                               └──────────────────┘
//! ```
//!
//! **Privacy**: the WAV file is **always** unlinked before `stop()`
//! returns, even when whisper-cli fails. We never persist audio
//! bytes, we never send them off-device.
//!
//! Calling `start()` on a session that's already recording cancels
//! the previous recording (the prior WAV is dropped without
//! transcription) — matches the v1.9.0 TTS "kill prev on speak"
//! semantics.

use std::sync::Mutex;
use std::time::Instant;

use crate::ai::stt::{SttEngine, Transcript};
use crate::ai::stt_recorder::{locate_whisper_cli, start_recording, RecorderError, Recording};

/// Errors specific to STT sessions. Pulls in `RecorderError` for
/// fan-out clarity in the Tauri command layer (frontend can show a
/// different toast for "no recorder" vs "whisper-cli failed").
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    /// `stop()` called when nothing was recording.
    #[error("no recording in progress")]
    NotRecording,
    /// Underlying recorder failed.
    #[error("recorder: {0}")]
    Recorder(#[from] RecorderError),
    /// whisper-cli binary not found on `$PATH` (or `$WHISPER_CLI`).
    #[error("whisper-cli not installed (install whisper.cpp and ensure `whisper-cli` is on PATH)")]
    WhisperNotInstalled,
    /// whisper-cli ran but exited non-zero.
    #[error("whisper-cli failed: {0}")]
    WhisperFailed(String),
    /// whisper-cli stdout didn't parse into any text.
    #[error("transcription produced no text")]
    Empty,
}

/// Snapshot of an in-flight recording. Held in a Mutex inside the
/// session.
struct InFlight {
    /// The recorder subprocess. `kill()`ed in `stop()`.
    recording: Recording,
    /// Wall-clock start of recording. Used to fill `Transcript.duration_ms`.
    started_at: Instant,
    /// Which engine to use at transcribe-time. Captured at `start()`
    /// so the user can't surprise us by changing the config
    /// mid-recording.
    engine: SttEngine,
    /// Optional explicit whisper.cpp model override. `None` falls
    /// through to `$WHISPER_MODEL` then whisper's compiled-in default.
    /// Captured at `start()` for the same reason as `engine`.
    model: Option<String>,
}

/// Single-slot STT session. Safe to share across threads via `Arc`.
pub struct SttSession {
    current: Mutex<Option<InFlight>>,
}

impl SttSession {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    /// Start a fresh recording. If one is already in flight, it's
    /// **cancelled** (process killed, WAV unlinked) — no transcript.
    /// Returns `()` on success; PID is not exposed because the
    /// frontend doesn't need it (single-slot model).
    ///
    /// `model` (v1.9.2): optional whisper.cpp model id ("base.en" etc.)
    /// or absolute path to a `.bin`. `None` defers to `$WHISPER_MODEL`
    /// then whisper's compiled default.
    pub fn start(&self, engine: SttEngine, model: Option<String>) -> Result<(), SttError> {
        let mut slot = self.current.lock().expect("SttSession mutex poisoned");
        if let Some(prev) = slot.take() {
            cancel_inflight(prev);
        }
        let rec = start_recording()?;
        let now = Instant::now();
        *slot = Some(InFlight {
            recording: rec,
            started_at: now,
            engine,
            model,
        });
        Ok(())
    }

    /// Stop the recording and transcribe. The WAV is always deleted
    /// before this returns.
    ///
    /// Returns `Err(NotRecording)` if `start()` wasn't called first.
    pub fn stop(&self) -> Result<Transcript, SttError> {
        let in_flight = {
            let mut slot = self.current.lock().expect("SttSession mutex poisoned");
            slot.take().ok_or(SttError::NotRecording)?
        };

        let InFlight {
            mut recording,
            started_at,
            engine,
            model,
        } = in_flight;
        let duration_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        // Kill + reap recorder. We don't propagate kill errors — the
        // OS occasionally fails kill() if the child already exited
        // naturally, which is fine.
        let _ = recording.child.kill();
        let _ = recording.child.wait();

        // Transcribe.
        let result = transcribe(&recording.wav_path, engine, duration_ms, model.as_deref());

        // Always unlink the WAV. Privacy.
        let _ = std::fs::remove_file(&recording.wav_path);

        result
    }

    /// True iff a recording slot is currently held. We don't
    /// `try_wait()` here because a self-exited recorder is rare and
    /// the next `stop()` cleans up anyway.
    pub fn is_recording(&self) -> bool {
        let slot = self.current.lock().expect("SttSession mutex poisoned");
        slot.is_some()
    }

    /// Cancel an in-flight recording: kill the recorder subprocess,
    /// unlink the WAV, drop the slot. Returns silently (no error,
    /// no transcript) regardless of state — calling `cancel()` on an
    /// idle session is a deliberate no-op so the frontend can wire
    /// ESC unconditionally.
    ///
    /// v1.9.2 — pairs with the new `slab_beacon_voice_stt_cancel`
    /// Tauri command. Discards audio bytes immediately for privacy.
    pub fn cancel(&self) {
        let prev = {
            let mut slot = self.current.lock().expect("SttSession mutex poisoned");
            slot.take()
        };
        if let Some(in_flight) = prev {
            cancel_inflight(in_flight);
        }
    }

    /// Test-only helper: stuff a dummy `InFlight` into the session so
    /// we can exercise the cancel/stop paths without spawning a real
    /// recorder. Uses `sleep 30` (or `cmd /c timeout`) as a stand-in
    /// for a long-lived recorder process. The WAV path is unique per
    /// call so parallel tests don't collide.
    #[cfg(test)]
    pub(crate) fn install_fake_inflight_for_test(&self) {
        use std::process::{Command, Stdio};
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let wav_path = std::env::temp_dir().join(format!(
            "slab-stt-fake-test-{}-{}.wav",
            std::process::id(),
            seq
        ));
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/c", "ping -n 30 127.0.0.1 > nul"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = cmd.spawn().expect("spawn fake recorder for test");
        // Create the dummy WAV so the unlink branch has something to chew on.
        std::fs::write(&wav_path, b"RIFF").expect("write fake wav");
        let mut slot = self.current.lock().expect("SttSession mutex poisoned");
        *slot = Some(InFlight {
            recording: Recording { child, wav_path },
            started_at: Instant::now(),
            engine: SttEngine::WhisperCpp,
            model: None,
        });
    }
}

impl Default for SttSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancel an in-flight recording: kill subprocess, unlink WAV.
/// Best-effort; errors are swallowed because we're throwing the
/// recording away.
fn cancel_inflight(mut in_flight: InFlight) {
    let _ = in_flight.recording.child.kill();
    let _ = in_flight.recording.child.wait();
    let _ = std::fs::remove_file(&in_flight.recording.wav_path);
}

/// Pure helper (v1.9.2): if `text` ends with `trigger`
/// (case-insensitive, whitespace-boundary-aware, ignoring a single
/// trailing punctuation char), return the text with the trigger
/// stripped + `true`. Otherwise return the trimmed text + `false`.
///
/// `trigger == None` always yields `(trimmed_text, false)` — that's
/// the "auto-send disabled" path.
///
/// Used by `BeaconChatPanel` after `stop()` to decide whether to
/// invoke `send()` automatically. Lives here (not in the panel) so
/// the algorithm can be exhaustively unit-tested without spinning up
/// the frontend, and so a future server-side auto-send command can
/// reuse the same logic.
pub fn detect_send_trigger(text: &str, trigger: Option<&str>) -> (String, bool) {
    let trimmed = text.trim();
    let Some(trig) = trigger else {
        return (trimmed.to_string(), false);
    };
    let trig = trig.trim();
    if trig.is_empty() || trimmed.is_empty() {
        return (trimmed.to_string(), false);
    }

    // Strip a single trailing punctuation char (.,!?;:) so transcripts
    // like "...send it." still fire. whisper.cpp commonly appends a
    // period to short utterances.
    let body = trimmed
        .strip_suffix(['.', ',', '!', '?', ';', ':'])
        .unwrap_or(trimmed)
        .trim_end();

    let body_lc = body.to_lowercase();
    let trig_lc = trig.to_lowercase();
    if !body_lc.ends_with(&trig_lc) {
        return (trimmed.to_string(), false);
    }
    // Word-boundary check: the char immediately before the suffix
    // must be whitespace or absent (whole-text-is-trigger case). Stops
    // "Tango" from firing a trigger of "go".
    let prefix_len = body_lc.len() - trig_lc.len();
    if prefix_len > 0 {
        let prev = body[..prefix_len].chars().next_back();
        match prev {
            Some(c) if c.is_whitespace() => {}
            _ => return (trimmed.to_string(), false),
        }
    }
    let cleaned = body[..prefix_len].trim_end().to_string();
    (cleaned, true)
}

/// Run the transcription engine on a captured WAV. Public so the
/// Tauri command layer (Task 4) can invoke it directly for the
/// "transcribe existing file" workflow (e.g. drag-and-drop a voice
/// memo in v1.9.2+).
///
/// `model` (v1.9.2): optional explicit whisper.cpp model. Precedence:
///   explicit arg → `$WHISPER_MODEL` env → omit (whisper default).
pub fn transcribe(
    wav: &std::path::Path,
    engine: SttEngine,
    duration_ms: u64,
    model: Option<&str>,
) -> Result<Transcript, SttError> {
    match engine {
        SttEngine::WhisperCpp => transcribe_whisper_cpp(wav, duration_ms, model),
    }
}

fn transcribe_whisper_cpp(
    wav: &std::path::Path,
    duration_ms: u64,
    model: Option<&str>,
) -> Result<Transcript, SttError> {
    let bin = locate_whisper_cli().ok_or(SttError::WhisperNotInstalled)?;
    let args = build_whisper_cmd(&bin, wav, model);

    let mut cmd = std::process::Command::new(&bin);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::null());

    let output = cmd
        .output()
        .map_err(|e| SttError::WhisperFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SttError::WhisperFailed(
            stderr.lines().last().unwrap_or("").into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let language = extract_language(&stderr);

    let t = parse_whisper_output(&stdout, language, duration_ms);
    if t.text.is_empty() {
        Err(SttError::Empty)
    } else {
        Ok(t)
    }
}

/// Pure argv builder for whisper-cli. Extracted so unit tests can
/// pin the exact flag order without spawning a subprocess.
///
/// Flags emitted (in order):
///   -f <wav>      input file
///   -nt           no timestamps in stdout
///   -l auto       auto-detect language
///   -m <model>    only if `model.is_some()` OR `$WHISPER_MODEL` set
///
/// `_bin` is ignored at the moment but kept in the signature for
/// future per-binary quirks (e.g. macOS-installed whisper.cpp vs.
/// Homebrew differs on some flags).
pub(crate) fn build_whisper_cmd(
    _bin: &str,
    wav: &std::path::Path,
    model: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::with_capacity(8);
    args.push("-f".into());
    args.push(wav.to_string_lossy().to_string());
    args.push("-nt".into());
    args.push("-l".into());
    args.push("auto".into());

    // Precedence: explicit arg → $WHISPER_MODEL env → omit.
    let resolved_model: Option<String> = match model {
        Some(m) if !m.is_empty() => Some(m.to_string()),
        _ => std::env::var("WHISPER_MODEL")
            .ok()
            .filter(|s| !s.is_empty()),
    };
    if let Some(m) = resolved_model {
        args.push("-m".into());
        args.push(m);
    }
    args
}

/// Parse `whisper-cli -nt` stdout. With `-nt`, each line is just the
/// transcribed segment (no timestamp prefix). We strip blank lines,
/// trim each segment, and join with a single space.
///
/// Defensive: if a build of whisper-cli ignores `-nt` and prints
/// timestamps anyway (`[00:00:00.000 --> 00:00:02.500]  text`), we
/// strip the bracketed prefix here too.
pub(crate) fn parse_whisper_output(
    stdout: &str,
    language: Option<String>,
    duration_ms: u64,
) -> Transcript {
    let mut parts: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Strip `[hh:mm:ss.fff --> hh:mm:ss.fff]` prefix if present.
        let cleaned = if line.starts_with('[') {
            match line.find(']') {
                Some(idx) => line[idx + 1..].trim(),
                None => line,
            }
        } else {
            line
        };
        if !cleaned.is_empty() {
            parts.push(cleaned.to_string());
        }
    }
    Transcript {
        text: parts.join(" "),
        language,
        duration_ms,
    }
}

/// Pull the detected language out of whisper-cli stderr. The line we
/// look for is shaped like:
///
///   `whisper_full: auto-detected language: en (p = 0.999)`
///
/// Returns `None` if we don't see that pattern.
fn extract_language(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        if let Some(idx) = line.find("auto-detected language:") {
            let rest = &line[idx + "auto-detected language:".len()..];
            let code = rest.split_whitespace().next()?;
            if !code.is_empty() && code.len() <= 5 {
                return Some(code.to_lowercase());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_recording_false_before_start() {
        let s = SttSession::new();
        assert!(!s.is_recording());
    }

    #[test]
    fn stop_on_empty_session_is_noop_error() {
        let s = SttSession::new();
        assert!(matches!(s.stop(), Err(SttError::NotRecording)));
        assert!(!s.is_recording());
    }

    #[test]
    fn parse_whisper_output_no_timestamps() {
        // Typical `-nt` output: one line per segment, just text.
        let stdout = " Hello world.\n How are you?\n";
        let t = parse_whisper_output(stdout, Some("en".into()), 2500);
        assert_eq!(t.text, "Hello world. How are you?");
        assert_eq!(t.language.as_deref(), Some("en"));
        assert_eq!(t.duration_ms, 2500);
    }

    #[test]
    fn parse_whisper_output_strips_timestamps_if_present() {
        // Defensive: some whisper.cpp builds ignore `-nt`.
        let stdout = "\
[00:00:00.000 --> 00:00:02.500]  Hello.
[00:00:02.500 --> 00:00:05.000]  World.
";
        let t = parse_whisper_output(stdout, None, 5000);
        assert_eq!(t.text, "Hello. World.");
        assert_eq!(t.language, None);
        assert_eq!(t.duration_ms, 5000);
    }

    #[test]
    fn parse_whisper_output_skips_blank_lines() {
        let stdout = "\n\n  Hello.\n\n  World.\n\n";
        let t = parse_whisper_output(stdout, None, 1000);
        assert_eq!(t.text, "Hello. World.");
    }

    #[test]
    fn parse_whisper_output_empty_text() {
        let t = parse_whisper_output("", None, 0);
        assert!(t.text.is_empty());
        assert_eq!(t.duration_ms, 0);
    }

    #[test]
    fn parse_whisper_output_unclosed_bracket_kept_as_is() {
        // Pathological input: `[` with no matching `]`. Keep the line.
        let stdout = "[ no closing bracket here\n";
        let t = parse_whisper_output(stdout, None, 0);
        assert_eq!(t.text, "[ no closing bracket here");
    }

    #[test]
    fn extract_language_finds_iso_code() {
        let stderr = "\
whisper_init_from_file_with_params_no_state: loading model
whisper_full: auto-detected language: en (p = 0.9876)
whisper_print_timings: total time = 1234.5 ms
";
        assert_eq!(extract_language(stderr), Some("en".into()));
    }

    #[test]
    fn extract_language_none_when_absent() {
        let stderr = "some unrelated output\n";
        assert_eq!(extract_language(stderr), None);
    }

    #[test]
    fn extract_language_rejects_garbage() {
        // Long token after the prefix → not a language code.
        let stderr = "whisper_full: auto-detected language: thisistoolong\n";
        assert_eq!(extract_language(stderr), None);
    }

    #[test]
    fn extract_language_handles_pt_br_style_locales() {
        // 5-char locales accepted (pt-BR is the realistic upper bound).
        let stderr = "whisper_full: auto-detected language: pt-br (p = 0.5)\n";
        assert_eq!(extract_language(stderr), Some("pt-br".into()));
    }

    #[test]
    fn stt_error_messages_user_grade() {
        // Sanity-check the user-facing strings — the frontend
        // doesn't rewrite them, it shows them verbatim in toasts.
        let e = SttError::NotRecording;
        assert!(e.to_string().contains("no recording"));
        let e = SttError::WhisperNotInstalled;
        assert!(e.to_string().contains("whisper-cli"));
        let e = SttError::Empty;
        assert!(e.to_string().contains("no text"));
    }

    // ---- v1.9.2 — cancel() tests ----------------------------------

    /// `cancel()` on an idle session must be a deliberate no-op:
    /// no panic, no state change. Lets the frontend wire ESC
    /// unconditionally.
    #[test]
    fn cancel_on_idle_session_is_noop() {
        let s = SttSession::new();
        assert!(!s.is_recording());
        s.cancel();
        assert!(!s.is_recording(), "cancel must not synthesise a slot");
    }

    /// `cancel()` after a (fake) start clears the in-flight slot and
    /// unlinks the WAV file. Exercises the kill + reap + unlink path
    /// without depending on real audio hardware.
    #[test]
    fn cancel_clears_recording_flag_and_unlinks_wav() {
        let s = SttSession::new();
        s.install_fake_inflight_for_test();
        assert!(s.is_recording());

        // Snapshot the WAV path so we can check unlink afterwards.
        let wav_path = {
            let slot = s.current.lock().expect("poisoned");
            slot.as_ref()
                .map(|f| f.recording.wav_path.clone())
                .expect("slot populated")
        };
        assert!(wav_path.exists(), "fake WAV should exist pre-cancel");

        s.cancel();
        assert!(!s.is_recording(), "cancel must drop the slot");
        assert!(
            !wav_path.exists(),
            "cancel must unlink the WAV (privacy guarantee)"
        );
    }

    /// `cancel()` twice in a row is fine — the second call is a no-op.
    /// Idempotency matters because the frontend may double-fire ESC if
    /// the user spams it.
    #[test]
    fn cancel_is_idempotent() {
        let s = SttSession::new();
        s.install_fake_inflight_for_test();
        s.cancel();
        s.cancel();
        assert!(!s.is_recording());
    }

    /// Cancelling then stopping returns NotRecording — confirms that
    /// the slot was actually cleared (not just the recording flag).
    /// This is what the Tauri command surface relies on for the
    /// "ESC after start, then no more transcribes" UX.
    #[test]
    fn cancel_then_stop_returns_not_recording() {
        let s = SttSession::new();
        s.install_fake_inflight_for_test();
        assert!(s.is_recording());
        s.cancel();
        assert!(matches!(s.stop(), Err(SttError::NotRecording)));
    }

    // ---- v1.9.2 — detect_send_trigger() tests ----------------------

    /// Basic happy path: trigger appears at the end with a space
    /// boundary. Strip the phrase, return `fire == true`.
    #[test]
    fn detect_send_trigger_matches_trailing_phrase() {
        let (text, fire) = detect_send_trigger("Summarise this please send it", Some("send it"));
        assert!(fire);
        assert_eq!(text, "Summarise this please");
    }

    /// Match is case-insensitive on both sides. "Go" with trigger
    /// "go" must fire.
    #[test]
    fn detect_send_trigger_case_insensitive() {
        let (text, fire) = detect_send_trigger("Tell me what page 5 says Go", Some("go"));
        assert!(fire);
        assert_eq!(text, "Tell me what page 5 says");
    }

    /// Word-boundary check: trigger "go" must NOT fire on "Tango".
    /// This is the failure mode that motivates the boundary check.
    #[test]
    fn detect_send_trigger_requires_word_boundary() {
        let (text, fire) = detect_send_trigger("I love Tango", Some("go"));
        assert!(!fire, "trigger must require a whitespace boundary");
        assert_eq!(text, "I love Tango");
    }

    /// No trigger configured → never fire, return the trimmed text.
    /// This is the "auto-send disabled" code path.
    #[test]
    fn detect_send_trigger_no_trigger_configured() {
        let (text, fire) = detect_send_trigger("hello send it", None);
        assert!(!fire);
        assert_eq!(text, "hello send it");
    }

    /// Trailing period (whisper.cpp often adds one) doesn't block
    /// detection. The stripped text retains commas/etc. that aren't
    /// the final char.
    #[test]
    fn detect_send_trigger_trims_trailing_punctuation() {
        let (text, fire) = detect_send_trigger("page 3 summary, send it.", Some("send it"));
        assert!(fire);
        assert_eq!(text, "page 3 summary,");
    }

    /// Trigger appearing mid-sentence is not a send signal. Only the
    /// trailing position counts.
    #[test]
    fn detect_send_trigger_not_at_end_no_fire() {
        let (text, fire) = detect_send_trigger("send it tomorrow", Some("send it"));
        assert!(!fire);
        assert_eq!(text, "send it tomorrow");
    }

    /// Empty input → never fires. Defensive: avoids index math on an
    /// empty slice.
    #[test]
    fn detect_send_trigger_empty_input() {
        let (text, fire) = detect_send_trigger("", Some("go"));
        assert!(!fire);
        assert_eq!(text, "");
    }

    /// Whitespace-only input is treated as empty after trimming.
    #[test]
    fn detect_send_trigger_whitespace_only() {
        let (text, fire) = detect_send_trigger("   ", Some("go"));
        assert!(!fire);
        assert_eq!(text, "");
    }

    /// Whole-text-is-trigger edge case. The cleaned text becomes
    /// empty but `fire == true` so the caller can decide what to do
    /// (we currently treat it as "user wanted to send their existing
    /// composer contents").
    #[test]
    fn detect_send_trigger_whole_text_is_trigger() {
        let (text, fire) = detect_send_trigger("send it", Some("send it"));
        assert!(fire);
        assert_eq!(text, "");
    }

    /// Empty trigger string is treated the same as `None` — never
    /// fires. Defensive against accidentally-empty config values.
    #[test]
    fn detect_send_trigger_empty_trigger_no_fire() {
        let (text, fire) = detect_send_trigger("hello world", Some(""));
        assert!(!fire);
        assert_eq!(text, "hello world");
    }

    // ── v1.9.2 Task 4: build_whisper_cmd ────────────────────────────

    /// All env-touching tests serialize on this mutex — `cargo test`
    /// runs in parallel by default and unsynchronised `set_var`/
    /// `remove_var` from multiple threads is unsound on edition 2024+.
    /// We use a sync mutex regardless to keep behaviour identical
    /// across editions.
    static WHISPER_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Default invocation: no explicit model, no env. Emits only the
    /// base flags; no `-m` appears.
    #[test]
    fn build_whisper_cmd_no_model_omits_dash_m() {
        let _g = WHISPER_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("WHISPER_MODEL").ok();
        std::env::remove_var("WHISPER_MODEL");

        let wav = std::path::PathBuf::from("/tmp/sample.wav");
        let args = build_whisper_cmd("/usr/local/bin/whisper-cli", &wav, None);
        assert_eq!(
            args,
            vec!["-f", "/tmp/sample.wav", "-nt", "-l", "auto"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert!(!args.iter().any(|a| a == "-m"));

        if let Some(v) = prev {
            std::env::set_var("WHISPER_MODEL", v);
        }
    }

    /// Explicit model arg always wins, even over `$WHISPER_MODEL`.
    #[test]
    fn build_whisper_cmd_explicit_model_overrides_env() {
        let _g = WHISPER_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("WHISPER_MODEL").ok();
        std::env::set_var("WHISPER_MODEL", "tiny.en");

        let wav = std::path::PathBuf::from("/tmp/sample.wav");
        let args = build_whisper_cmd("whisper-cli", &wav, Some("base.en"));
        let m_idx = args.iter().position(|a| a == "-m").expect("must have -m");
        assert_eq!(args[m_idx + 1], "base.en");
        assert!(!args.iter().any(|a| a == "tiny.en"));

        match prev {
            Some(v) => std::env::set_var("WHISPER_MODEL", v),
            None => std::env::remove_var("WHISPER_MODEL"),
        }
    }

    /// Env variable used when no explicit arg is given.
    #[test]
    fn build_whisper_cmd_env_used_when_no_explicit() {
        let _g = WHISPER_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("WHISPER_MODEL").ok();
        std::env::set_var("WHISPER_MODEL", "small.en");

        let wav = std::path::PathBuf::from("/tmp/sample.wav");
        let args = build_whisper_cmd("whisper-cli", &wav, None);
        let m_idx = args.iter().position(|a| a == "-m").expect("must have -m");
        assert_eq!(args[m_idx + 1], "small.en");

        match prev {
            Some(v) => std::env::set_var("WHISPER_MODEL", v),
            None => std::env::remove_var("WHISPER_MODEL"),
        }
    }

    /// Empty string passed as `model` is treated as `None` (defensive
    /// against IPC-deserialized empties from the frontend).
    #[test]
    fn build_whisper_cmd_empty_model_falls_back_to_env() {
        let _g = WHISPER_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("WHISPER_MODEL").ok();
        std::env::remove_var("WHISPER_MODEL");

        let wav = std::path::PathBuf::from("/tmp/sample.wav");
        let args = build_whisper_cmd("whisper-cli", &wav, Some(""));
        assert!(!args.iter().any(|a| a == "-m"));

        if let Some(v) = prev {
            std::env::set_var("WHISPER_MODEL", v);
        }
    }
}
