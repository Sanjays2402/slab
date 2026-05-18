//! Plugin command runner (Slice 7).
//!
//! Each `[[contributions.commands]]` declares either a `shell` line
//! (executed via `/bin/sh -c` on unix, `cmd /C` on Windows) or a `url`
//! (handed back to the caller for opener dispatch). Manifest
//! validation already enforces exactly-one-of those two fields, so the
//! runner just dispatches on which is set.
//!
//! Shell commands run under a wall-clock timeout (default 30s) and
//! return stdout/stderr captured for the UI. URL commands return a
//! `Url` outcome carrying the URL string — the frontend is responsible
//! for opening it via `tauri_plugin_opener` (so this module stays
//! decoupled from the Tauri runtime).

use crate::plugins::contributions::ActiveCommand;
use serde::Serialize;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Default wall-clock budget for `shell` commands. Matches the default
/// timeout we picked for `PdfActionContribution` — plugin authors who
/// need longer should make the work non-blocking (a script that
/// schedules background work) rather than asking us to bump this.
pub const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CommandOutcome {
    /// A `shell` command finished (or timed out / crashed). Inspect
    /// `status` for the precise reason.
    Shell(ShellReport),
    /// A `url` command — the frontend should open this with the system
    /// browser via `tauri_plugin_opener::open_url`.
    Url { url: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ShellReport {
    pub command_id: String,
    pub plugin_id: String,
    pub status: CommandStatus,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommandStatus {
    Ok,
    NonZeroExit,
    Timeout,
    SpawnFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("command {0:?} has neither shell nor url (manifest invariant violated)")]
    NoExecutable(String),
    #[error("command {0:?} has both shell and url (manifest invariant violated)")]
    AmbiguousDispatch(String),
}

/// Dispatch a single contributed command. URL commands return
/// immediately with a `Url` outcome; shell commands spawn `/bin/sh -c`
/// (Windows: `cmd /C`) under [`DEFAULT_COMMAND_TIMEOUT_MS`].
pub fn run_command(cmd: &ActiveCommand) -> Result<CommandOutcome, CommandError> {
    let c = &cmd.command;
    match (c.shell.as_deref(), c.url.as_deref()) {
        (Some(s), None) => Ok(CommandOutcome::Shell(run_shell(
            &cmd.plugin_id,
            &c.id,
            s,
            DEFAULT_COMMAND_TIMEOUT_MS,
        ))),
        (None, Some(u)) => Ok(CommandOutcome::Url { url: u.to_string() }),
        (None, None) => Err(CommandError::NoExecutable(c.id.clone())),
        (Some(_), Some(_)) => Err(CommandError::AmbiguousDispatch(c.id.clone())),
    }
}

fn run_shell(plugin_id: &str, command_id: &str, line: &str, timeout_ms: u64) -> ShellReport {
    let started = Instant::now();
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(line);
        c
    } else {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(line);
        c
    };
    let spawn_res = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn_res {
        Ok(c) => c,
        Err(e) => {
            return ShellReport {
                command_id: command_id.to_string(),
                plugin_id: plugin_id.to_string(),
                status: CommandStatus::SpawnFailed,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
                duration_ms: started.elapsed().as_millis() as u64,
            };
        }
    };

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let outcome = wait_with_timeout(&mut child, timeout);
    let timed_out = matches!(outcome, WaitOutcome::Timeout);
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
    }

    let mut stdout_buf = String::new();
    let mut stderr_buf = String::new();
    if !timed_out {
        if let Some(out) = child.stdout.take() {
            let _ = read_to_string(out, &mut stdout_buf);
        }
        if let Some(err) = child.stderr.take() {
            let _ = read_to_string(err, &mut stderr_buf);
        }
    } else {
        // On timeout, deliberately drop the pipe handles WITHOUT
        // reading. /bin/sh on Linux (dash) doesn't propagate SIGTERM
        // to its exec'd children, so a grandchild like `sleep` keeps
        // the stdout/stderr pipes open until it finishes on its own.
        // Reading here would stall the whole timeout path for the
        // remainder of the grandchild's lifetime (~9.8s in the
        // 200ms-timeout regression test). The Timeout arm of the
        // match below produces a synthetic stderr message anyway, so
        // we weren't surfacing pipe contents to the caller on
        // timeout in any case.
        drop(child.stdout.take());
        drop(child.stderr.take());
    }
    let duration_ms = started.elapsed().as_millis() as u64;

    match outcome {
        WaitOutcome::Exited(status) if status.success() => ShellReport {
            command_id: command_id.to_string(),
            plugin_id: plugin_id.to_string(),
            status: CommandStatus::Ok,
            stdout: stdout_buf,
            stderr: stderr_buf,
            duration_ms,
        },
        WaitOutcome::Exited(status) => ShellReport {
            command_id: command_id.to_string(),
            plugin_id: plugin_id.to_string(),
            status: CommandStatus::NonZeroExit,
            stdout: stdout_buf,
            stderr: format!(
                "{stderr_buf}\n(exit code: {})",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "<signal>".into())
            ),
            duration_ms,
        },
        WaitOutcome::Timeout => ShellReport {
            command_id: command_id.to_string(),
            plugin_id: plugin_id.to_string(),
            status: CommandStatus::Timeout,
            stdout: stdout_buf,
            stderr: format!("{stderr_buf}\n(killed after {timeout_ms}ms timeout)"),
            duration_ms,
        },
    }
}

enum WaitOutcome {
    Exited(std::process::ExitStatus),
    Timeout,
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> WaitOutcome {
    let deadline = Instant::now() + timeout;
    let poll = Duration::from_millis(25);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    return WaitOutcome::Timeout;
                }
                std::thread::sleep(poll);
            }
            Err(_) => return WaitOutcome::Timeout,
        }
    }
}

fn read_to_string<R: std::io::Read>(mut r: R, dst: &mut String) -> std::io::Result<()> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf)?;
    dst.push_str(&String::from_utf8_lossy(&buf));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::CommandContribution;
    use std::path::PathBuf;

    fn cmd(id: &str, shell: Option<&str>, url: Option<&str>) -> ActiveCommand {
        ActiveCommand {
            plugin_id: "com.example.test".into(),
            plugin_dir: PathBuf::from("/tmp"),
            command: CommandContribution {
                id: id.into(),
                label: id.into(),
                shell: shell.map(String::from),
                url: url.map(String::from),
                default_keymap: None,
            },
        }
    }

    #[test]
    fn url_command_returns_url_outcome() {
        let c = cmd("open-docs", None, Some("https://example.com/docs"));
        let out = run_command(&c).unwrap();
        match out {
            CommandOutcome::Url { url } => assert_eq!(url, "https://example.com/docs"),
            other => panic!("expected Url, got {other:?}"),
        }
    }

    #[test]
    fn neither_shell_nor_url_is_error() {
        let c = cmd("broken", None, None);
        let err = run_command(&c).unwrap_err();
        match err {
            CommandError::NoExecutable(id) => assert_eq!(id, "broken"),
            other => panic!("expected NoExecutable, got {other:?}"),
        }
    }

    #[test]
    fn both_shell_and_url_is_error() {
        let c = cmd("both", Some("echo hi"), Some("https://example.com"));
        let err = run_command(&c).unwrap_err();
        match err {
            CommandError::AmbiguousDispatch(id) => assert_eq!(id, "both"),
            other => panic!("expected AmbiguousDispatch, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_echo_captures_stdout() {
        let c = cmd("greet", Some("echo hello-from-slab"), None);
        let out = run_command(&c).unwrap();
        match out {
            CommandOutcome::Shell(r) => {
                assert_eq!(r.status, CommandStatus::Ok, "stderr was: {}", r.stderr);
                assert!(
                    r.stdout.contains("hello-from-slab"),
                    "stdout: {:?}",
                    r.stdout
                );
                assert_eq!(r.command_id, "greet");
                assert_eq!(r.plugin_id, "com.example.test");
            }
            other => panic!("expected Shell, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_false_reports_non_zero_exit() {
        let c = cmd("fail", Some("false"), None);
        let out = run_command(&c).unwrap();
        match out {
            CommandOutcome::Shell(r) => {
                assert_eq!(r.status, CommandStatus::NonZeroExit);
                assert!(r.stderr.contains("exit code"), "stderr: {:?}", r.stderr);
            }
            other => panic!("expected Shell, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_stderr_is_captured() {
        let c = cmd("warn", Some("echo whoops 1>&2"), None);
        let out = run_command(&c).unwrap();
        match out {
            CommandOutcome::Shell(r) => {
                assert_eq!(r.status, CommandStatus::Ok, "stderr was: {}", r.stderr);
                assert!(r.stderr.contains("whoops"), "stderr: {:?}", r.stderr);
            }
            other => panic!("expected Shell, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_timeout_kills_long_running() {
        let started = Instant::now();
        let report = run_shell("p", "slow", "sleep 10", 200);
        assert_eq!(report.status, CommandStatus::Timeout);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "timeout should fire promptly, elapsed = {:?}",
            started.elapsed()
        );
        assert!(
            report.stderr.contains("200ms"),
            "stderr: {:?}",
            report.stderr
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_multi_line_works() {
        // Compound shell line — exercises the `/bin/sh -c` quoting path.
        let c = cmd("multi", Some("echo a; echo b; echo c"), None);
        let out = run_command(&c).unwrap();
        match out {
            CommandOutcome::Shell(r) => {
                assert_eq!(r.status, CommandStatus::Ok);
                assert!(r.stdout.contains("a"));
                assert!(r.stdout.contains("b"));
                assert!(r.stdout.contains("c"));
            }
            other => panic!("expected Shell, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_spawn_failure_when_shell_missing() {
        // Force a spawn failure by calling run_shell with a deliberately
        // broken first arg via the public path: we use a `cmd` invocation
        // that runs and exits 127 ("command not found") which is still a
        // valid spawn (the shell itself starts). To exercise SpawnFailed,
        // we'd need to override the shell binary — instead we test the
        // 127 case as NonZeroExit (the more common real-world failure).
        let c = cmd(
            "nope",
            Some("definitely-not-a-real-binary-xyzzy-9999"),
            None,
        );
        let out = run_command(&c).unwrap();
        match out {
            CommandOutcome::Shell(r) => {
                assert_eq!(r.status, CommandStatus::NonZeroExit);
            }
            other => panic!("expected Shell, got {other:?}"),
        }
    }
}
