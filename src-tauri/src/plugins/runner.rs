//! PDF action runner (Slice 6).
//!
//! Each `[[contributions.pdf_actions]]` declares a CLI tool with
//! argument template (`{in}` / `{out}` placeholders), and a timeout.
//! The runner:
//!   1. Copies the user's input PDF to a tempfile (so we never hand
//!      the original to the plugin's CLI — keeps the source pristine).
//!   2. Allocates an empty output tempfile.
//!   3. Substitutes the placeholders in the template args.
//!   4. Spawns the CLI under a wall-clock timeout. On timeout we kill
//!      the child and return an error.
//!   5. Copies the output tempfile to the user's chosen destination.
//!
//! No shell — args are passed as separate argv entries so quoting and
//! injection aren't a concern. Stdout/stderr are captured and surfaced
//! to the UI so the user can see what the CLI complained about.

use crate::plugins::contributions::ActivePdfAction;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActionReport {
    pub action_id: String,
    pub plugin_id: String,
    pub status: ActionStatus,
    /// Output file (only populated on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<PathBuf>,
    pub stdout: String,
    pub stderr: String,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    Ok,
    NonZeroExit,
    Timeout,
    SpawnFailed,
    NoOutput,
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("input PDF does not exist: {0}")]
    InputMissing(PathBuf),
    #[error("could not prepare tempfiles: {0}")]
    TempfileFailed(std::io::Error),
    #[error("could not write output: {0}")]
    OutputWriteFailed(std::io::Error),
}

/// Run a plugin PDF action against `input_pdf`, writing the result to
/// `output_pdf`. Returns a [`ActionReport`] describing what happened
/// (the report's `status` field distinguishes timeout vs non-zero exit
/// vs missing output etc.).
pub fn run_pdf_action(
    action: &ActivePdfAction,
    input_pdf: &Path,
    output_pdf: &Path,
) -> Result<ActionReport, ActionError> {
    if !input_pdf.is_file() {
        return Err(ActionError::InputMissing(input_pdf.to_path_buf()));
    }
    let temp = tempfile::tempdir().map_err(ActionError::TempfileFailed)?;
    let in_tmp = temp.path().join("input.pdf");
    let out_tmp = temp.path().join("output.pdf");
    fs::copy(input_pdf, &in_tmp).map_err(ActionError::TempfileFailed)?;

    let argv = substitute_args(&action.action.args, &in_tmp, &out_tmp);
    let timeout = Duration::from_millis(action.action.timeout_ms.max(1));

    let started = Instant::now();
    let spawn_res = Command::new(&action.action.cli)
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn_res {
        Ok(c) => c,
        Err(e) => {
            return Ok(ActionReport {
                action_id: action.action.id.clone(),
                plugin_id: action.plugin_id.clone(),
                status: ActionStatus::SpawnFailed,
                output_path: None,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
                duration_ms: started.elapsed().as_millis() as u64,
            });
        }
    };

    // Poll for completion with a wall-clock timeout.
    let exit_status = wait_with_timeout(&mut child, timeout);

    // If we timed out, kill the child FIRST so the subsequent reads
    // don't block waiting for it to finish writing on its own.
    if matches!(exit_status, WaitOutcome::Timeout) {
        let _ = child.kill();
        let _ = child.wait();
    }

    let mut stdout_buf = String::new();
    let mut stderr_buf = String::new();
    if let Some(out) = child.stdout.take() {
        let _ = read_to_string(out, &mut stdout_buf);
    }
    if let Some(err) = child.stderr.take() {
        let _ = read_to_string(err, &mut stderr_buf);
    }

    let duration_ms = started.elapsed().as_millis() as u64;

    match exit_status {
        WaitOutcome::Exited(status) if status.success() => {
            if !out_tmp.is_file() {
                return Ok(ActionReport {
                    action_id: action.action.id.clone(),
                    plugin_id: action.plugin_id.clone(),
                    status: ActionStatus::NoOutput,
                    output_path: None,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                    duration_ms,
                });
            }
            if let Some(parent) = output_pdf.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(ActionError::OutputWriteFailed)?;
                }
            }
            fs::copy(&out_tmp, output_pdf).map_err(ActionError::OutputWriteFailed)?;
            Ok(ActionReport {
                action_id: action.action.id.clone(),
                plugin_id: action.plugin_id.clone(),
                status: ActionStatus::Ok,
                output_path: Some(output_pdf.to_path_buf()),
                stdout: stdout_buf,
                stderr: stderr_buf,
                duration_ms,
            })
        }
        WaitOutcome::Exited(status) => Ok(ActionReport {
            action_id: action.action.id.clone(),
            plugin_id: action.plugin_id.clone(),
            status: ActionStatus::NonZeroExit,
            output_path: None,
            stdout: stdout_buf,
            stderr: format!(
                "{stderr_buf}\n(exit code: {})",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "<signal>".into())
            ),
            duration_ms,
        }),
        WaitOutcome::Timeout => Ok(ActionReport {
            action_id: action.action.id.clone(),
            plugin_id: action.plugin_id.clone(),
            status: ActionStatus::Timeout,
            output_path: None,
            stdout: stdout_buf,
            stderr: format!(
                "{stderr_buf}\n(killed after {}ms timeout)",
                action.action.timeout_ms
            ),
            duration_ms,
        }),
    }
}

/// Substitute `{in}` and `{out}` placeholders in the args. Unknown
/// placeholders are left as-is so plugin authors can debug typos.
pub fn substitute_args(template: &[String], in_path: &Path, out_path: &Path) -> Vec<String> {
    let ins = in_path.to_string_lossy().to_string();
    let outs = out_path.to_string_lossy().to_string();
    template
        .iter()
        .map(|a| a.replace("{in}", &ins).replace("{out}", &outs))
        .collect()
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
    use crate::plugins::manifest::PdfActionContribution;
    use std::fs;
    use tempfile::TempDir;

    fn fake_action(cli: &str, args: Vec<&str>, timeout_ms: u64) -> ActivePdfAction {
        ActivePdfAction {
            plugin_id: "com.example.test".into(),
            plugin_dir: PathBuf::from("/tmp"),
            action: PdfActionContribution {
                id: "t".into(),
                label: "Test".into(),
                cli: cli.into(),
                args: args.into_iter().map(String::from).collect(),
                timeout_ms,
            },
        }
    }

    fn make_dummy_pdf(dir: &Path) -> PathBuf {
        // We don't need a valid PDF for these tests — the spawned CLI
        // is a stand-in (cp / true / sleep). Just give the runner a
        // real file to copy into the tempdir.
        let p = dir.join("in.pdf");
        fs::write(&p, b"%PDF-1.4 fake").unwrap();
        p
    }

    #[test]
    fn substitute_args_replaces_in_and_out() {
        let args = vec!["--linearize".into(), "{in}".into(), "{out}".into()];
        let got = substitute_args(&args, Path::new("/a/in.pdf"), Path::new("/a/out.pdf"));
        assert_eq!(got, vec!["--linearize", "/a/in.pdf", "/a/out.pdf"]);
    }

    #[test]
    fn substitute_args_leaves_unknown_placeholders() {
        let args = vec!["--mode={mode}".into()];
        let got = substitute_args(&args, Path::new("/a"), Path::new("/b"));
        assert_eq!(got, vec!["--mode={mode}"]);
    }

    #[test]
    fn input_missing_returns_error() {
        let tmp = TempDir::new().unwrap();
        let action = fake_action("true", vec![], 5_000);
        let err = run_pdf_action(
            &action,
            &tmp.path().join("missing.pdf"),
            &tmp.path().join("out.pdf"),
        )
        .unwrap_err();
        match err {
            ActionError::InputMissing(_) => (),
            other => panic!("expected InputMissing, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn successful_run_with_cp_copies_to_output() {
        let tmp = TempDir::new().unwrap();
        let input = make_dummy_pdf(tmp.path());
        let output = tmp.path().join("done.pdf");
        // `cp {in} {out}` — should succeed and produce a non-empty output.
        let action = fake_action("cp", vec!["{in}", "{out}"], 5_000);
        let report = run_pdf_action(&action, &input, &output).unwrap();
        assert_eq!(
            report.status,
            ActionStatus::Ok,
            "stderr was: {}",
            report.stderr
        );
        assert!(output.is_file(), "output PDF should exist after Ok");
        assert_eq!(report.output_path.as_deref(), Some(output.as_path()));
    }

    #[cfg(unix)]
    #[test]
    fn non_zero_exit_is_captured() {
        let tmp = TempDir::new().unwrap();
        let input = make_dummy_pdf(tmp.path());
        let output = tmp.path().join("done.pdf");
        // `false` always exits non-zero.
        let action = fake_action("false", vec![], 5_000);
        let report = run_pdf_action(&action, &input, &output).unwrap();
        assert_eq!(report.status, ActionStatus::NonZeroExit);
        assert!(!output.exists(), "output must not be written on failure");
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_long_running_cli() {
        let tmp = TempDir::new().unwrap();
        let input = make_dummy_pdf(tmp.path());
        let output = tmp.path().join("done.pdf");
        // `sleep 10` with a 200ms budget must time out.
        let action = fake_action("sleep", vec!["10"], 200);
        let started = Instant::now();
        let report = run_pdf_action(&action, &input, &output).unwrap();
        assert_eq!(report.status, ActionStatus::Timeout);
        // Should return well before sleep finishes.
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "timeout should fire promptly, elapsed = {:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn spawn_failure_reported() {
        let tmp = TempDir::new().unwrap();
        let input = make_dummy_pdf(tmp.path());
        let output = tmp.path().join("done.pdf");
        let action = fake_action("/definitely/does/not/exist/zzz", vec![], 5_000);
        let report = run_pdf_action(&action, &input, &output).unwrap();
        assert_eq!(report.status, ActionStatus::SpawnFailed);
    }

    #[cfg(unix)]
    #[test]
    fn no_output_when_cli_ignores_out_placeholder() {
        let tmp = TempDir::new().unwrap();
        let input = make_dummy_pdf(tmp.path());
        let output = tmp.path().join("done.pdf");
        // `true` exits 0 but writes nothing — runner should report NoOutput.
        let action = fake_action("true", vec![], 5_000);
        let report = run_pdf_action(&action, &input, &output).unwrap();
        assert_eq!(report.status, ActionStatus::NoOutput);
    }
}
