// Slab Beacon — Voice Mode session manager.
//
// Holds a single "current speaker" Child handle and exposes thread-safe
// start/stop ops. The Tauri command surface wraps an `Arc<VoiceSession>`
// in `tauri::State<>` and dispatches all speak/stop calls through it.
//
// Why a single slot? We deliberately only allow ONE active speech at a
// time. If the user hits the speak button on a second message while
// the first is still speaking, the second click cancels the first
// (matches macOS Preview's "Read Aloud" behaviour). This avoids
// audio-collision chaos.

use std::process::Child;
use std::sync::Mutex;

use crate::ai::voice::{speak, SpeakOpts, TtsEngine, VoiceError};

/// Single-slot speaker session. Safe to share across threads via Arc.
pub struct VoiceSession {
    current: Mutex<Option<Child>>,
}

impl VoiceSession {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    /// Cancel any in-flight speech and start a new one. Returns the new
    /// PID for the UI to display "speaking…" state.
    ///
    /// We `kill()` the previous child but DON'T wait for it; the OS
    /// reaps it asynchronously. The new child takes its slot
    /// immediately so the next `stop()` call hits the right process.
    pub fn speak(&self, eng: TtsEngine, text: &str, opts: &SpeakOpts) -> Result<u32, VoiceError> {
        let mut slot = self.current.lock().expect("VoiceSession mutex poisoned");
        if let Some(mut prev) = slot.take() {
            let _ = prev.kill();
            // Best-effort reap so we don't leak zombies on long sessions.
            let _ = prev.wait();
        }
        let child = speak(eng, text, opts)?;
        let pid = child.id();
        *slot = Some(child);
        Ok(pid)
    }

    /// Cancel any in-flight speech. No-op if nothing is speaking.
    /// Returns `true` if a process was actually killed.
    pub fn stop(&self) -> bool {
        let mut slot = self.current.lock().expect("VoiceSession mutex poisoned");
        if let Some(mut prev) = slot.take() {
            let _ = prev.kill();
            let _ = prev.wait();
            true
        } else {
            false
        }
    }

    /// Returns `true` iff a child handle is currently held. Note that
    /// the child may have already exited naturally — we don't poll
    /// `try_wait()` here because the cost of doing it on every UI
    /// query outweighs the precision. The frontend polls only on
    /// button-press, so the staleness window is tiny.
    pub fn is_speaking(&self) -> bool {
        let mut slot = self.current.lock().expect("VoiceSession mutex poisoned");
        // Opportunistically reap exited children so the UI's view of
        // "speaking" stays accurate.
        if let Some(child) = slot.as_mut() {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    *slot = None;
                    false
                }
                Ok(None) => true,
                Err(_) => true,
            }
        } else {
            false
        }
    }
}

impl Default for VoiceSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A no-op stop on a fresh session must report `false` (nothing was
    /// killed) and not panic.
    #[test]
    fn stop_on_empty_session_is_noop() {
        let s = VoiceSession::new();
        assert!(!s.stop());
        assert!(!s.is_speaking());
    }

    /// `is_speaking` must report false on a fresh session.
    #[test]
    fn fresh_session_not_speaking() {
        let s = VoiceSession::new();
        assert!(!s.is_speaking());
    }

    /// Default impl yields the same shape as `new()`.
    #[test]
    fn default_matches_new() {
        let s = VoiceSession::default();
        assert!(!s.is_speaking());
    }

    /// Speak → stop round-trip with a deliberately-fake engine path.
    /// We invoke `sh -c 'sleep 5'` impersonating a long-running speaker
    /// by injecting a custom `Child` directly into the slot, then
    /// stopping it. Validates that stop() actually kills the child.
    #[test]
    fn stop_kills_held_child() {
        let s = VoiceSession::new();
        // Manually plant a long-sleeping process — equivalent to a
        // real `say` mid-utterance — and confirm stop() reaps it.
        let child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawning sh sleep should work on unix-likes");
        {
            let mut slot = s.current.lock().unwrap();
            *slot = Some(child);
        }
        assert!(s.is_speaking());
        assert!(s.stop());
        assert!(!s.is_speaking());
    }

    /// is_speaking() must opportunistically clear an exited child so
    /// the UI doesn't keep showing "speaking…" forever after a quick
    /// utterance finishes.
    #[test]
    fn is_speaking_reaps_exited_child() {
        let s = VoiceSession::new();
        let child = std::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawning sh exit should work");
        {
            let mut slot = s.current.lock().unwrap();
            *slot = Some(child);
        }
        // Tiny delay so the process can exit.
        std::thread::sleep(std::time::Duration::from_millis(50));
        // First call reaps it, returns false.
        assert!(!s.is_speaking());
        // Subsequent calls remain false.
        assert!(!s.is_speaking());
    }
}
