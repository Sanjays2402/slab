//! Slab Lens dependency preflight — single source of truth for
//! "does my host have everything Lens features need?".
//!
//! Lens (v0.13.0) introduced several features that shell out to
//! external binaries or talk to a local AI provider:
//!
//! | Feature                  | Dep                        |
//! |--------------------------|----------------------------|
//! | `pdf::ocr` (raster + OCR)| `pdftoppm` + `tesseract`   |
//! | `pdf::table_extract`     | Poppler `pdftotext` (xpdf-flavored does **not** work) |
//! | `ai::vision`             | Ollama HTTP endpoint with a vision-capable model |
//!
//! Surface all probes through a single struct so the CLI (`slab lens
//! preflight`) and any future UI panel can print one consolidated
//! report instead of running each check ad-hoc.
//!
//! All probes are **non-fatal and offline-safe** — they never panic
//! and never raise; the report's per-dep `Status` carries success or
//! a human-readable hint.

use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

/// Outcome of probing a single external dependency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Status {
    /// Dependency found and usable. `detail` is a short version string
    /// or a flavor note (e.g. `"poppler"` vs `"xpdf"`).
    Ok { detail: String },
    /// Dependency found but unusable as-is. `detail` explains why.
    Wrong { detail: String },
    /// Dependency missing. `hint` is an actionable install command.
    Missing { hint: String },
}

impl Status {
    pub fn is_ok(&self) -> bool {
        matches!(self, Status::Ok { .. })
    }
}

/// One row of the preflight report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    /// Stable id: `pdftoppm`, `tesseract`, `pdftotext`, `ollama`.
    pub id: String,
    /// Human label: `"Poppler pdftoppm"`, `"Tesseract OCR"`, etc.
    pub label: String,
    /// Which Lens features need this dep (comma-separated user labels).
    pub features: String,
    pub status: Status,
}

/// Full report — a list of checks plus a derived top-level summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub checks: Vec<Check>,
    /// Number of checks that are `Status::Ok`.
    pub ok: usize,
    /// Total checks.
    pub total: usize,
}

impl PreflightReport {
    pub fn all_ok(&self) -> bool {
        self.ok == self.total
    }
}

/// Options for the Ollama HTTP probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightOpts {
    /// Ollama base URL (e.g. `http://localhost:11434`). None → skip
    /// the network probe entirely and report it as `Missing`.
    pub ollama_url: Option<String>,
    /// Timeout for the Ollama probe.
    pub ollama_timeout: Duration,
}

impl Default for PreflightOpts {
    fn default() -> Self {
        Self {
            ollama_url: Some("http://localhost:11434".to_string()),
            ollama_timeout: Duration::from_millis(500),
        }
    }
}

/// Run every Lens dep check and assemble a report.
///
/// Never fails — every probe maps its own errors into a `Status`.
pub fn preflight(opts: &PreflightOpts) -> PreflightReport {
    let checks = vec![
        check_pdftoppm(),
        check_tesseract(),
        check_pdftotext_poppler(),
        check_ollama(opts),
    ];
    let ok = checks.iter().filter(|c| c.status.is_ok()).count();
    let total = checks.len();
    PreflightReport { checks, ok, total }
}

// ---------- per-dep probes ----------

fn check_pdftoppm() -> Check {
    let id = "pdftoppm".to_string();
    let label = "Poppler pdftoppm (PDF→PPM rasterizer)".to_string();
    let features = "OCR pipeline".to_string();
    let status = match Command::new("pdftoppm").arg("-v").output() {
        Ok(out) => {
            let mut blob = Vec::with_capacity(out.stdout.len() + out.stderr.len());
            blob.extend_from_slice(&out.stdout);
            blob.extend_from_slice(&out.stderr);
            let txt = String::from_utf8_lossy(&blob).trim().to_string();
            let first_line = txt.lines().next().unwrap_or("(unknown version)");
            Status::Ok {
                detail: first_line.to_string(),
            }
        }
        Err(_) => Status::Missing {
            hint: install_hint(
                "Poppler",
                "poppler",
                "poppler-utils",
                "scoop install poppler",
            ),
        },
    };
    Check {
        id,
        label,
        features,
        status,
    }
}

fn check_tesseract() -> Check {
    let id = "tesseract".to_string();
    let label = "Tesseract OCR".to_string();
    let features = "OCR pipeline, Library auto-OCR queue".to_string();
    let status = match Command::new("tesseract").arg("--version").output() {
        Ok(out) => {
            let mut blob = Vec::with_capacity(out.stdout.len() + out.stderr.len());
            blob.extend_from_slice(&out.stdout);
            blob.extend_from_slice(&out.stderr);
            let txt = String::from_utf8_lossy(&blob).trim().to_string();
            let first_line = txt.lines().next().unwrap_or("(unknown version)");
            Status::Ok {
                detail: first_line.to_string(),
            }
        }
        Err(_) => Status::Missing {
            hint: install_hint(
                "Tesseract",
                "tesseract",
                "tesseract-ocr",
                "scoop install tesseract",
            ),
        },
    };
    Check {
        id,
        label,
        features,
        status,
    }
}

fn check_pdftotext_poppler() -> Check {
    let id = "pdftotext".to_string();
    let label = "Poppler pdftotext (with -bbox-layout)".to_string();
    let features = "Table extraction".to_string();

    // Step 1: any binary?
    if Command::new("pdftotext").arg("-v").output().is_err() {
        return Check {
            id,
            label,
            features,
            status: Status::Missing {
                hint: install_hint(
                    "Poppler",
                    "poppler",
                    "poppler-utils",
                    "scoop install poppler",
                ),
            },
        };
    }
    // Step 2: Poppler flavor? Probe `-h` for `-bbox-layout`.
    let help = match Command::new("pdftotext").arg("-h").output() {
        Ok(o) => o,
        Err(e) => {
            return Check {
                id,
                label,
                features,
                status: Status::Wrong {
                    detail: format!("could not probe pdftotext -h: {e}"),
                },
            };
        }
    };
    let mut blob = Vec::with_capacity(help.stdout.len() + help.stderr.len());
    blob.extend_from_slice(&help.stdout);
    blob.extend_from_slice(&help.stderr);
    let txt = String::from_utf8_lossy(&blob);
    if txt.contains("-bbox-layout") {
        // Try to extract a one-line flavor marker from `-v`.
        let detail = Command::new("pdftotext")
            .arg("-v")
            .output()
            .ok()
            .map(|o| {
                let mut b = Vec::with_capacity(o.stdout.len() + o.stderr.len());
                b.extend_from_slice(&o.stdout);
                b.extend_from_slice(&o.stderr);
                String::from_utf8_lossy(&b)
                    .lines()
                    .next()
                    .unwrap_or("poppler")
                    .to_string()
            })
            .unwrap_or_else(|| "poppler".to_string());
        Check {
            id,
            label,
            features,
            status: Status::Ok { detail },
        }
    } else {
        Check {
            id,
            label,
            features,
            status: Status::Wrong {
                detail: "looks like the xpdf flavor (no -bbox-layout). \
                         Install Poppler instead (brew install poppler, \
                         apt install poppler-utils, or scoop install poppler)."
                    .to_string(),
            },
        }
    }
}

fn check_ollama(opts: &PreflightOpts) -> Check {
    let id = "ollama".to_string();
    let label = "Ollama HTTP endpoint".to_string();
    let features = "Beacon AI chat, summary, semantic search, vision Q&A, auto-tag".to_string();

    let url = match &opts.ollama_url {
        Some(u) => u.trim_end_matches('/').to_string(),
        None => {
            return Check {
                id,
                label,
                features,
                status: Status::Missing {
                    hint: "no ollama url configured — Beacon AI features will fall back to OpenAI-compatible if configured".to_string(),
                },
            };
        }
    };
    // Parse host:port out of the URL. We only care about reachability,
    // not HTTP semantics, so a raw TCP connect with a tight timeout is
    // perfect — works for both http://localhost:11434 and remote hosts.
    let host_port = parse_host_port(&url);
    let Some(hp) = host_port else {
        return Check {
            id,
            label,
            features,
            status: Status::Wrong {
                detail: format!("could not parse host:port from ollama url `{url}`"),
            },
        };
    };
    let addrs = match hp.to_socket_addrs() {
        Ok(it) => it.collect::<Vec<_>>(),
        Err(_) => {
            return Check {
                id,
                label,
                features,
                status: Status::Missing {
                    hint: format!(
                        "could not resolve {hp}. Install Ollama: https://ollama.com/download"
                    ),
                },
            };
        }
    };
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, opts.ollama_timeout).is_ok() {
            return Check {
                id,
                label,
                features,
                status: Status::Ok {
                    detail: format!("{url} reachable (TCP {addr})"),
                },
            };
        }
    }
    Check {
        id,
        label,
        features,
        status: Status::Missing {
            hint: format!(
                "Ollama not reachable at {url}. \
                 Install: https://ollama.com/download — then `ollama pull llama3.2`."
            ),
        },
    }
}

/// Extract `host:port` from a URL like `http://localhost:11434` or
/// `https://ollama.example.com`. Defaults port to 80/443 based on
/// scheme. Returns None on malformed input.
fn parse_host_port(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host_and_path = rest.split_once('/').map(|(h, _)| h).unwrap_or(rest);
    if host_and_path.contains(':') {
        return Some(host_and_path.to_string());
    }
    let port = match scheme.to_ascii_lowercase().as_str() {
        "https" => 443,
        _ => 80,
    };
    Some(format!("{host_and_path}:{port}"))
}

fn install_hint(human: &str, brew: &str, apt: &str, scoop: &str) -> String {
    format!(
        "{human} not found. macOS: `brew install {brew}`. \
         Debian/Ubuntu: `sudo apt install {apt}`. Windows: `{scoop}`."
    )
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_ok_is_ok() {
        let s = Status::Ok {
            detail: "v1".into(),
        };
        assert!(s.is_ok());
    }

    #[test]
    fn status_wrong_is_not_ok() {
        let s = Status::Wrong { detail: "x".into() };
        assert!(!s.is_ok());
    }

    #[test]
    fn status_missing_is_not_ok() {
        let s = Status::Missing { hint: "x".into() };
        assert!(!s.is_ok());
    }

    #[test]
    fn report_all_ok_arithmetic() {
        let r = PreflightReport {
            checks: vec![
                Check {
                    id: "a".into(),
                    label: "A".into(),
                    features: "".into(),
                    status: Status::Ok { detail: "".into() },
                },
                Check {
                    id: "b".into(),
                    label: "B".into(),
                    features: "".into(),
                    status: Status::Ok { detail: "".into() },
                },
            ],
            ok: 2,
            total: 2,
        };
        assert!(r.all_ok());
    }

    #[test]
    fn report_partial_is_not_all_ok() {
        let r = PreflightReport {
            checks: vec![],
            ok: 1,
            total: 2,
        };
        assert!(!r.all_ok());
    }

    #[test]
    fn preflight_runs_to_completion_and_classifies_everything() {
        // The runtime test — every probe must return a Status without
        // panicking, regardless of whether the host has the binaries.
        let opts = PreflightOpts {
            ollama_url: Some("http://127.0.0.1:1".to_string()), // closed port
            ollama_timeout: Duration::from_millis(50),
        };
        let r = preflight(&opts);
        assert_eq!(r.total, 4);
        assert_eq!(r.checks.len(), 4);
        // Every check has a non-empty id and label.
        for c in &r.checks {
            assert!(!c.id.is_empty(), "check missing id: {c:?}");
            assert!(!c.label.is_empty(), "check missing label: {c:?}");
        }
        // The Ollama probe on a closed port must classify as Missing
        // (since `ureq` connection-refused maps to Err).
        let ollama = r.checks.iter().find(|c| c.id == "ollama").unwrap();
        assert!(
            matches!(ollama.status, Status::Missing { .. }),
            "expected Missing for closed-port ollama, got {:?}",
            ollama.status
        );
    }

    #[test]
    fn preflight_with_no_ollama_url_marks_ollama_missing() {
        let opts = PreflightOpts {
            ollama_url: None,
            ollama_timeout: Duration::from_millis(10),
        };
        let r = preflight(&opts);
        let ollama = r.checks.iter().find(|c| c.id == "ollama").unwrap();
        assert!(matches!(ollama.status, Status::Missing { .. }));
    }

    #[test]
    fn install_hint_mentions_all_three_oses() {
        let h = install_hint("Foo", "foo", "foo", "scoop install foo");
        assert!(h.contains("macOS"));
        assert!(h.contains("Debian"));
        assert!(h.contains("Windows"));
    }

    #[test]
    fn report_serializes_to_stable_json_shape() {
        let r = PreflightReport {
            checks: vec![Check {
                id: "x".into(),
                label: "X".into(),
                features: "f".into(),
                status: Status::Ok { detail: "v".into() },
            }],
            ok: 1,
            total: 1,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"checks\""), "missing checks: {s}");
        assert!(s.contains("\"kind\":\"ok\""), "tag/rename wrong: {s}");
        assert!(s.contains("\"detail\":\"v\""), "missing detail: {s}");
    }
}
