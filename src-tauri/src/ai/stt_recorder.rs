//! Slab Beacon — Voice Mode (STT recorder — v1.9.1 Task 2).
//!
//! Cross-platform mic capture by shelling out to native recorders.
//! We deliberately avoid `cpal` / audio crate bindings so that:
//!
//!   * Hermetic CI works: command-builder unit tests don't need real
//!     audio hardware. Same pattern as v1.9.0 TTS engines.
//!   * Binary size stays small: no codec bundles.
//!   * Platform conventions are honoured (sox on mac, arecord on
//!     linux, PowerShell `MediaCapture` on Windows).
//!
//! Format choice: **16-kHz mono signed-16-bit PCM WAV**. Required by
//! whisper.cpp; small files (32 KB/s ≈ 2 MB/minute); no resampling
//! cost.
//!
//! All recorders are configured to record indefinitely; the session
//! layer (Task 3) is responsible for killing the child to stop.
//! `start_recording()` returns a `Child` plus the absolute path of
//! the WAV file the child is writing to.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::ai::stt::{recorder_binary, which};

/// Sample rate whisper.cpp wants by default.
pub(crate) const SAMPLE_RATE_HZ: u32 = 16_000;
/// Mono.
pub(crate) const CHANNELS: u32 = 1;
/// 16-bit signed PCM. Most engines accept this; whisper.cpp loves it.
pub(crate) const BIT_DEPTH: u32 = 16;

/// Build the argv for `sox` (or `rec`) to record 16-kHz mono WAV from
/// the default audio input until killed.
///
/// `sox -t coreaudio default -c 1 -r 16000 -b 16 <path>`
///
/// We use `-t coreaudio default` on macOS so sox uses the system
/// default input device automatically. On Linux sox-from-distro uses
/// pulseaudio/alsa via `-d`, but `-t alsa default` works there too.
pub(crate) fn sox_args(path: &Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if cfg!(target_os = "macos") {
        args.push("-t".into());
        args.push("coreaudio".into());
        args.push("default".into());
    } else {
        args.push("-d".into());
    }
    args.push("-c".into());
    args.push(CHANNELS.to_string());
    args.push("-r".into());
    args.push(SAMPLE_RATE_HZ.to_string());
    args.push("-b".into());
    args.push(BIT_DEPTH.to_string());
    args.push(path.to_string_lossy().into_owned());
    args
}

/// Build the argv for `arecord` (alsa-utils, Linux):
///
/// `arecord -f S16_LE -r 16000 -c 1 <path>`
pub(crate) fn arecord_args(path: &Path) -> Vec<String> {
    vec![
        "-f".into(),
        "S16_LE".into(),
        "-r".into(),
        SAMPLE_RATE_HZ.to_string(),
        "-c".into(),
        CHANNELS.to_string(),
        path.to_string_lossy().into_owned(),
    ]
}

/// Render the inline PowerShell script that records WAV with
/// `System.Speech` — wait, no: `System.Speech` is TTS-only. For
/// recording we use `Windows.Media.Capture` UWP namespace via
/// PowerShell, which is broadly available on Windows 10+.
///
/// We emit a self-contained script that records indefinitely until
/// the host process is killed (matches sox / arecord semantics).
pub(crate) fn powershell_record_script(path: &Path) -> String {
    // `Get-WindowsCapability`... is overkill. The simplest path is the
    // `NAudio` PInvoke shim — but that's third-party. Use waveIn via
    // the .NET BCL by leaning on `System.Net.WebSockets`? No.
    //
    // Reality: Windows ships `Windows.Media.Capture.MediaCapture` as
    // a UWP class accessible from PowerShell with `Add-Type` + WinRT
    // projection. It's nuanced but reliable.
    //
    // For v1.9.1 we keep it dead simple: invoke the standard
    // `SoundRecorder.exe` (Windows built-in) command-line variant via
    // `tlocode` does not exist. Use the `[Windows.Media.Capture.MediaCapture, ContentType = WindowsRuntime]`
    // projection. The script below records indefinitely until
    // terminated.
    //
    // NOTE: This script blocks waiting on `[Console]::ReadLine()` so
    // killing the PowerShell process cleanly stops the recording.
    let p = path.to_string_lossy().replace('\'', "''");
    format!(
        r#"$ErrorActionPreference='Stop'; \
[void][Windows.Media.Capture.MediaCapture, Windows.Media, ContentType = WindowsRuntime]; \
[void][Windows.Media.MediaProperties.MediaEncodingProfile, Windows.Media, ContentType = WindowsRuntime]; \
[void][Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]; \
$mc = New-Object Windows.Media.Capture.MediaCapture; \
$settings = New-Object Windows.Media.Capture.MediaCaptureInitializationSettings; \
$settings.StreamingCaptureMode = [Windows.Media.Capture.StreamingCaptureMode]::Audio; \
$mc.InitializeAsync($settings).AsTask().Wait(); \
$folder = [Windows.Storage.StorageFolder]::GetFolderFromPathAsync((Split-Path '{p}')).AsTask().GetAwaiter().GetResult(); \
$file = $folder.CreateFileAsync((Split-Path -Leaf '{p}'), [Windows.Storage.CreationCollisionOption]::ReplaceExisting).AsTask().GetAwaiter().GetResult(); \
$profile = [Windows.Media.MediaProperties.MediaEncodingProfile]::CreateWav([Windows.Media.MediaProperties.AudioEncodingQuality]::Low); \
$profile.Audio.SampleRate = {sr}; \
$profile.Audio.ChannelCount = {ch}; \
$profile.Audio.BitsPerSample = {bd}; \
$mc.StartRecordToStorageFileAsync($profile, $file).AsTask().Wait(); \
[Console]::ReadLine() | Out-Null; \
$mc.StopRecordAsync().AsTask().Wait()"#,
        p = p,
        sr = SAMPLE_RATE_HZ,
        ch = CHANNELS,
        bd = BIT_DEPTH,
    )
}

/// Result of `start_recording`: the running child + the temp WAV
/// path it will be filling.
pub struct Recording {
    pub child: Child,
    pub wav_path: PathBuf,
}

/// Errors specific to mic capture.
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    /// No supported recorder binary on PATH (sox/arecord/PowerShell).
    #[error("no recorder binary available on PATH (try `brew install sox` on macOS, `apt install alsa-utils` on Linux)")]
    NoRecorder,
    /// Spawn failed (binary present but exec'd weirdly).
    #[error("recorder spawn failed: {0}")]
    Spawn(String),
    /// Tempfile creation failed.
    #[error("tempfile error: {0}")]
    Tempfile(String),
}

/// Create a fresh tempfile path for the WAV the recorder will fill.
/// We don't create the file itself — the recorder does. We just pick
/// a name in `std::env::temp_dir()` that's not in use.
pub(crate) fn fresh_wav_path() -> Result<PathBuf, RecorderError> {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    p.push(format!("slab-stt-{pid}-{nanos}.wav"));
    Ok(p)
}

/// Pick the right recorder for the current OS and spawn it,
/// recording to a fresh tempfile. The session layer (Task 3) is
/// responsible for killing the returned child to stop, then reading
/// the WAV, then deleting it.
pub fn start_recording() -> Result<Recording, RecorderError> {
    let bin = recorder_binary().ok_or(RecorderError::NoRecorder)?;
    let wav_path = fresh_wav_path()?;

    let child = if cfg!(target_os = "windows") {
        // PowerShell -NoProfile -Command "<script>"
        let script = powershell_record_script(&wav_path);
        Command::new(&bin)
            .args(["-NoProfile", "-Command", &script])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RecorderError::Spawn(e.to_string()))?
    } else if bin.ends_with("arecord") {
        Command::new(&bin)
            .args(arecord_args(&wav_path))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RecorderError::Spawn(e.to_string()))?
    } else {
        // sox / rec
        Command::new(&bin)
            .args(sox_args(&wav_path))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RecorderError::Spawn(e.to_string()))?
    };

    Ok(Recording { child, wav_path })
}

/// Locate the whisper.cpp CLI binary on `$PATH` (and `$WHISPER_CLI`
/// override). Used by the session layer in Task 3 — exposed here so
/// the capability probe in `stt.rs` and the session layer agree on
/// the lookup logic.
pub fn locate_whisper_cli() -> Option<String> {
    if let Ok(p) = std::env::var("WHISPER_CLI") {
        if !p.is_empty() && std::path::Path::new(&p).is_file() {
            return Some(p);
        }
    }
    which("whisper-cli")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sox_args_use_16khz_mono_wav() {
        let args = sox_args(Path::new("/tmp/x.wav"));
        assert!(args.iter().any(|a| a == "-r"));
        assert!(args.iter().any(|a| a == "16000"));
        assert!(args.iter().any(|a| a == "-c"));
        assert!(args.iter().any(|a| a == "1"));
        assert!(args.iter().any(|a| a == "-b"));
        assert!(args.iter().any(|a| a == "16"));
        assert!(args.last().unwrap().ends_with("x.wav"));
    }

    #[test]
    fn sox_args_on_mac_use_coreaudio_default() {
        // Only assert when actually compiled on mac — sox_args is
        // cfg-conditional. On other OSes we use `-d`.
        let args = sox_args(Path::new("/tmp/y.wav"));
        if cfg!(target_os = "macos") {
            assert!(args.iter().any(|a| a == "coreaudio"));
            assert!(args.iter().any(|a| a == "default"));
        } else {
            assert!(args.iter().any(|a| a == "-d"));
        }
    }

    #[test]
    fn arecord_args_use_s16le_16khz_mono() {
        let args = arecord_args(Path::new("/tmp/z.wav"));
        assert!(args.iter().any(|a| a == "-f"));
        assert!(args.iter().any(|a| a == "S16_LE"));
        assert!(args.iter().any(|a| a == "-r"));
        assert!(args.iter().any(|a| a == "16000"));
        assert!(args.iter().any(|a| a == "-c"));
        assert!(args.iter().any(|a| a == "1"));
        assert!(args.last().unwrap().ends_with("z.wav"));
    }

    #[test]
    fn powershell_script_records_to_wav() {
        let script = powershell_record_script(Path::new(r"C:\tmp\x.wav"));
        assert!(script.contains("MediaCapture"));
        assert!(script.contains(r"C:\tmp\x.wav"));
        // 16-kHz mono 16-bit must appear in script.
        assert!(script.contains("16000"));
        assert!(script.contains("1"));
        assert!(script.contains("16"));
        // Must block on stdin so killing the proc stops recording.
        assert!(script.contains("ReadLine"));
    }

    #[test]
    fn powershell_script_escapes_single_quotes_in_path() {
        // A path with an apostrophe must not break the PS string
        // literal. We escape with the standard PS doubled-quote.
        let script = powershell_record_script(Path::new(r"C:\Users\bob's\x.wav"));
        // Doubled single-quote inside the script:
        assert!(script.contains(r"bob''s"));
    }

    #[test]
    fn fresh_wav_path_lives_in_tempdir_and_is_unique() {
        let p1 = fresh_wav_path().unwrap();
        // Different nanos guarantee uniqueness — even back-to-back
        // calls produce distinct names because nanos has resolution.
        std::thread::sleep(std::time::Duration::from_nanos(2));
        let p2 = fresh_wav_path().unwrap();
        assert_ne!(p1, p2);
        assert!(p1.starts_with(std::env::temp_dir()));
        assert!(p1.extension().map(|e| e == "wav").unwrap_or(false));
    }

    // Process-global mutex so the two WHISPER_CLI tests don't race —
    // cargo runs tests in parallel by default, and both mutate the same
    // process env var. Without serialisation, one test's overwrite would
    // bleed into the other's read window (observed flaking CI run
    // 26283755466).
    static WHISPER_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn locate_whisper_cli_respects_env_override() {
        let _guard = WHISPER_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Point WHISPER_CLI at a file that definitely exists (this
        // very test binary). Then we should get that path back.
        let exe = std::env::current_exe().unwrap();
        let orig = std::env::var("WHISPER_CLI").ok();
        std::env::set_var("WHISPER_CLI", exe.as_os_str());
        let got = locate_whisper_cli();
        match orig {
            Some(v) => std::env::set_var("WHISPER_CLI", v),
            None => std::env::remove_var("WHISPER_CLI"),
        }
        assert_eq!(got.as_deref(), Some(exe.to_str().unwrap()));
    }

    #[test]
    fn locate_whisper_cli_ignores_nonexistent_env_override() {
        let _guard = WHISPER_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let orig = std::env::var("WHISPER_CLI").ok();
        std::env::set_var("WHISPER_CLI", "/definitely/not/a/real/path/whisper-cli");
        let got = locate_whisper_cli();
        match orig {
            Some(v) => std::env::set_var("WHISPER_CLI", v),
            None => std::env::remove_var("WHISPER_CLI"),
        }
        // Falls through to $PATH lookup, which probably also fails on CI.
        // Just assert it didn't honour the bogus override.
        assert_ne!(
            got.as_deref(),
            Some("/definitely/not/a/real/path/whisper-cli")
        );
    }
}
