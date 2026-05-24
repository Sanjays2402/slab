//! Batch signing — sign every `*.pdf` in a folder, with progress events.
//!
//! Acrobat Pro's "Batch Sign" feature costs $239/yr and only runs on Windows.
//! Slab's version runs offline, parallel via rayon, on every desktop platform,
//! and surfaces progress events the UI can subscribe to.
//!
//! Task 5 of the v3.11.0 Signet Pro plan. The driver here is signer-agnostic:
//! it walks paths, applies a [`Signer`] callback per file, and aggregates a
//! [`BatchReport`]. The real callback (CMS signing) is wired in by
//! `signet::sign::sign_pdf` in a follow-up tick; here we land the planner,
//! the progress channel, the rayon-parallel driver, and the test surface.

#![allow(dead_code)] // Wired into a Tauri command in a follow-up tick.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use walkdir::WalkDir;

/// One row in a [`BatchReport`].
#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub input: PathBuf,
    pub output: PathBuf,
    pub ok: bool,
    pub error: Option<String>,
    pub elapsed: Duration,
}

impl BatchEntry {
    /// Marketing-grade single-line status — good for log lines.
    pub fn status_line(&self) -> String {
        let tag = if self.ok { "ok" } else { "FAIL" };
        let elapsed_ms = self.elapsed.as_millis();
        match &self.error {
            Some(e) => format!(
                "[{tag}] {} → {} ({elapsed_ms}ms) — {e}",
                self.input.display(),
                self.output.display()
            ),
            None => format!(
                "[{tag}] {} → {} ({elapsed_ms}ms)",
                self.input.display(),
                self.output.display()
            ),
        }
    }
}

/// Aggregated result of [`sign_folder`] / [`run_batch`].
#[derive(Debug, Clone, Default)]
pub struct BatchReport {
    pub entries: Vec<BatchEntry>,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub elapsed: Duration,
}

impl BatchReport {
    /// Fraction of jobs that finished without error. Returns 0.0 when total=0.
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.succeeded as f64 / self.total as f64
        }
    }

    /// True iff every input was signed successfully.
    pub fn fully_succeeded(&self) -> bool {
        self.total > 0 && self.failed == 0
    }

    /// Iterator over only the failed entries — handy for UI red-row rendering.
    pub fn failures(&self) -> impl Iterator<Item = &BatchEntry> {
        self.entries.iter().filter(|e| !e.ok)
    }
}

/// One planned job before it executes — pairs an input PDF path with the
/// output path it will be written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchJob {
    pub input: PathBuf,
    pub output: PathBuf,
}

/// How to derive `output_path` from `input_path` and `output_dir`.
#[derive(Debug, Clone, Copy, Default)]
pub enum NameStrategy {
    /// `report.pdf` → `report-signed.pdf` in output_dir.
    #[default]
    SuffixSigned,
    /// `report.pdf` → `report.pdf` in output_dir (preserves the original
    /// filename; collides if input_dir == output_dir).
    Mirror,
}

/// Options passed to [`plan_batch`] / [`run_batch`].
#[derive(Debug, Clone)]
pub struct BatchOptions {
    pub recursive: bool,
    pub naming: NameStrategy,
    /// Skip files whose target output already exists. Lets users re-run the
    /// batch and only re-sign what changed.
    pub skip_if_output_exists: bool,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            naming: NameStrategy::SuffixSigned,
            skip_if_output_exists: false,
        }
    }
}

/// Walk `input_dir` for `*.pdf` files and pair each with the output path the
/// driver will write it to. Sorted by input path for stable iteration order
/// (so progress UI rows don't shuffle between runs).
pub fn plan_batch(
    input_dir: &Path,
    output_dir: &Path,
    opts: &BatchOptions,
) -> std::io::Result<Vec<BatchJob>> {
    let max_depth = if opts.recursive { usize::MAX } else { 1 };
    let mut jobs: Vec<BatchJob> = WalkDir::new(input_dir)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false)
        })
        .map(|e| {
            let input = e.path().to_path_buf();
            let output = derive_output_path(&input, output_dir, opts.naming);
            BatchJob { input, output }
        })
        .collect();

    if opts.skip_if_output_exists {
        jobs.retain(|j| !j.output.exists());
    }
    jobs.sort_by(|a, b| a.input.cmp(&b.input));
    Ok(jobs)
}

/// Map an input file path to its output file path under the destination dir.
pub fn derive_output_path(input: &Path, output_dir: &Path, naming: NameStrategy) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("pdf");
    let filename = match naming {
        NameStrategy::SuffixSigned => format!("{stem}-signed.{ext}"),
        NameStrategy::Mirror => format!("{stem}.{ext}"),
    };
    output_dir.join(filename)
}

/// Per-job callback. Returns `Ok(())` on success or an error message string.
///
/// Trait alias so callers can pass a fn pointer, a closure, or a heavier
/// signer object. `Send + Sync` because rayon shards the iterator across
/// the global thread pool.
pub trait Signer: Fn(&BatchJob) -> Result<(), String> + Send + Sync {}
impl<T> Signer for T where T: Fn(&BatchJob) -> Result<(), String> + Send + Sync {}

/// Progress callback fired after every job completes. Receives
/// `(done, total)`. The driver guarantees `done` is monotonically increasing.
pub trait ProgressSink: Fn(usize, usize) + Send + Sync {}
impl<T> ProgressSink for T where T: Fn(usize, usize) + Send + Sync {}

/// Execute the planned jobs in parallel with rayon's global pool.
///
/// Per-job timing is captured inside the closure so we don't pay for the
/// global-clock skew between worker threads. The progress sink is invoked
/// after each job; it sees a strictly increasing `done` counter even though
/// jobs finish out of order.
pub fn run_batch<S: Signer, P: ProgressSink>(
    jobs: Vec<BatchJob>,
    signer: S,
    progress: P,
) -> BatchReport {
    let total = jobs.len();
    let started = Instant::now();
    let done = Arc::new(AtomicUsize::new(0));
    let progress = Arc::new(progress);
    let entries = Arc::new(Mutex::new(Vec::with_capacity(total)));

    jobs.into_par_iter().for_each(|job| {
        let job_started = Instant::now();
        let result = signer(&job);
        let elapsed = job_started.elapsed();
        let entry = match result {
            Ok(()) => BatchEntry {
                input: job.input.clone(),
                output: job.output.clone(),
                ok: true,
                error: None,
                elapsed,
            },
            Err(e) => BatchEntry {
                input: job.input.clone(),
                output: job.output.clone(),
                ok: false,
                error: Some(e),
                elapsed,
            },
        };
        entries.lock().expect("mutex poison").push(entry);
        let now_done = done.fetch_add(1, Ordering::SeqCst) + 1;
        progress(now_done, total);
    });

    let mut entries = match Arc::try_unwrap(entries) {
        Ok(m) => m.into_inner().expect("mutex poison"),
        Err(_) => panic!("dangling Arc — driver leaked a reference"),
    };
    // Stable order for callers/UI: by input path.
    entries.sort_by(|a, b| a.input.cmp(&b.input));

    let succeeded = entries.iter().filter(|e| e.ok).count();
    let failed = total - succeeded;
    BatchReport {
        entries,
        total,
        succeeded,
        failed,
        elapsed: started.elapsed(),
    }
}

/// Convenience wrapper: plan + run in one call.
pub fn sign_folder<S: Signer, P: ProgressSink>(
    input_dir: &Path,
    output_dir: &Path,
    opts: &BatchOptions,
    signer: S,
    progress: P,
) -> std::io::Result<BatchReport> {
    let jobs = plan_batch(input_dir, output_dir, opts)?;
    Ok(run_batch(jobs, signer, progress))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tmp")
    }

    fn write_pdf(p: &Path) {
        fs::write(p, b"%PDF-1.7\n%fake\n").unwrap();
    }

    #[test]
    fn derive_output_suffix_signed() {
        let out = derive_output_path(
            Path::new("/in/report.pdf"),
            Path::new("/out"),
            NameStrategy::SuffixSigned,
        );
        assert_eq!(out, PathBuf::from("/out/report-signed.pdf"));
    }

    #[test]
    fn derive_output_mirror() {
        let out = derive_output_path(
            Path::new("/in/report.pdf"),
            Path::new("/out"),
            NameStrategy::Mirror,
        );
        assert_eq!(out, PathBuf::from("/out/report.pdf"));
    }

    #[test]
    fn plan_finds_only_pdf_files() {
        let dir = tmp();
        write_pdf(&dir.path().join("a.pdf"));
        write_pdf(&dir.path().join("B.PDF")); // case-insensitive
        fs::write(dir.path().join("notes.txt"), b"hi").unwrap();
        fs::write(dir.path().join("report.docx"), b"x").unwrap();

        let out = tmp();
        let jobs = plan_batch(dir.path(), out.path(), &BatchOptions::default()).unwrap();
        assert_eq!(jobs.len(), 2);
        let names: Vec<_> = jobs
            .iter()
            .map(|j| j.input.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"a.pdf".to_string()));
        assert!(names.contains(&"B.PDF".to_string()));
    }

    #[test]
    fn plan_respects_recursive_flag() {
        let dir = tmp();
        write_pdf(&dir.path().join("top.pdf"));
        let sub = dir.path().join("nested");
        fs::create_dir(&sub).unwrap();
        write_pdf(&sub.join("inner.pdf"));

        let out = tmp();
        let shallow = plan_batch(dir.path(), out.path(), &BatchOptions::default()).unwrap();
        assert_eq!(shallow.len(), 1);

        let deep = plan_batch(
            dir.path(),
            out.path(),
            &BatchOptions {
                recursive: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(deep.len(), 2);
    }

    #[test]
    fn plan_skips_if_output_exists() {
        let in_dir = tmp();
        let out_dir = tmp();
        write_pdf(&in_dir.path().join("alpha.pdf"));
        write_pdf(&in_dir.path().join("beta.pdf"));
        // Pre-create the alpha output to simulate a prior run.
        write_pdf(&out_dir.path().join("alpha-signed.pdf"));

        let jobs = plan_batch(
            in_dir.path(),
            out_dir.path(),
            &BatchOptions {
                skip_if_output_exists: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].input.file_name().unwrap() == "beta.pdf");
    }

    #[test]
    fn run_batch_aggregates_success_and_failure() {
        let dir = tmp();
        let inputs: Vec<_> = (0..5)
            .map(|i| {
                let p = dir.path().join(format!("doc{i}.pdf"));
                write_pdf(&p);
                p
            })
            .collect();
        let out = tmp();
        let jobs = plan_batch(dir.path(), out.path(), &BatchOptions::default()).unwrap();
        assert_eq!(jobs.len(), inputs.len());

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = Arc::clone(&calls);

        let report = run_batch(
            jobs,
            move |job| {
                if job.input.file_name().unwrap() == "doc2.pdf" {
                    Err("simulated CMS encoding error".into())
                } else {
                    Ok(())
                }
            },
            move |done, total| {
                assert!(done <= total);
                calls_clone.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert_eq!(report.total, 5);
        assert_eq!(report.succeeded, 4);
        assert_eq!(report.failed, 1);
        assert!(!report.fully_succeeded());
        assert!((report.success_rate() - 0.8).abs() < 1e-9);
        assert_eq!(report.failures().count(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn run_batch_handles_empty_input() {
        let report = run_batch(vec![], |_| Ok(()), |_, _| {});
        assert_eq!(report.total, 0);
        assert!(!report.fully_succeeded());
        assert_eq!(report.success_rate(), 0.0);
    }

    #[test]
    fn sign_folder_end_to_end_smoke() {
        let in_dir = tmp();
        let out_dir = tmp();
        for name in ["a.pdf", "b.pdf", "c.pdf"] {
            write_pdf(&in_dir.path().join(name));
        }

        let report = sign_folder(
            in_dir.path(),
            out_dir.path(),
            &BatchOptions::default(),
            |job| {
                // Pretend-sign: just touch the output file.
                fs::write(&job.output, b"%PDF-1.7\nsigned\n").map_err(|e| e.to_string())
            },
            |_, _| {},
        )
        .unwrap();

        assert!(report.fully_succeeded());
        assert_eq!(report.total, 3);
        for name in ["a-signed.pdf", "b-signed.pdf", "c-signed.pdf"] {
            assert!(out_dir.path().join(name).exists(), "missing output: {name}");
        }
    }

    #[test]
    fn batch_entry_status_line_includes_error() {
        let e = BatchEntry {
            input: PathBuf::from("/in/x.pdf"),
            output: PathBuf::from("/out/x-signed.pdf"),
            ok: false,
            error: Some("bad cert".into()),
            elapsed: Duration::from_millis(42),
        };
        let line = e.status_line();
        assert!(line.starts_with("[FAIL]"));
        assert!(line.contains("bad cert"));
        assert!(line.contains("42ms"));
    }
}
