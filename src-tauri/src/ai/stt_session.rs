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
    pub fn start(&self, engine: SttEngine) -> Result<(), SttError> {
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
        } = in_flight;
        let duration_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        // Kill + reap recorder. We don't propagate kill errors — the
        // OS occasionally fails kill() if the child already exited
        // naturally, which is fine.
        let _ = recording.child.kill();
        let _ = recording.child.wait();

        // Transcribe.
        let result = transcribe(&recording.wav_path, engine, duration_ms);

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

/// Run the transcription engine on a captured WAV. Public so the
/// Tauri command layer (Task 4) can invoke it directly for the
/// "transcribe existing file" workflow (e.g. drag-and-drop a voice
/// memo in v1.9.2+).
pub fn transcribe(
    wav: &std::path::Path,
    engine: SttEngine,
    duration_ms: u64,
) -> Result<Transcript, SttError> {
    match engine {
        SttEngine::WhisperCpp => transcribe_whisper_cpp(wav, duration_ms),
    }
}

fn transcribe_whisper_cpp(wav: &std::path::Path, duration_ms: u64) -> Result<Transcript, SttError> {
    let bin = locate_whisper_cli().ok_or(SttError::WhisperNotInstalled)?;

    // Run whisper-cli with sensible defaults:
    //   -f <wav>          — input file
    //   -nt               — no timestamps in stdout (cleaner parse)
    //   -l auto           — auto-detect language; print to stderr
    //   -m <model>        — optional override via $WHISPER_MODEL
    //
    // We deliberately omit `-otxt`/`-ovtt` flags — they'd write extra
    // files. Stdout is the source of truth.
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("-f").arg(wav);
    cmd.arg("-nt");
    cmd.arg("-l").arg("auto");
    if let Ok(model) = std::env::var("WHISPER_MODEL") {
        if !model.is_empty() {
            cmd.arg("-m").arg(model);
        }
    }
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
}
