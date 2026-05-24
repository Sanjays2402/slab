//! `run_recipe_batch` — apply one recipe to every PDF in a folder, in parallel.
//!
//! Uses rayon for parallelism. Each file's progress (start + per-step
//! events + completion/failure) is multiplexed onto a single
//! `BatchProgress` stream the UI can render as a per-file × per-step grid.
//!
//! Failures never abort the batch: a 200-file run with 2 corrupt PDFs
//! still produces 198 output PDFs and reports the 2 failures.

use crate::pdf::atelier::recipe::Recipe;
use crate::pdf::atelier::run::{run_recipe, Progress};
use crate::pdf::PdfError;
use rayon::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum BatchProgress {
    FileStarted {
        file_index: usize,
        path: PathBuf,
    },
    StepProgress {
        file_index: usize,
        inner: Progress,
    },
    FileCompleted {
        file_index: usize,
        path: PathBuf,
    },
    FileFailed {
        file_index: usize,
        path: PathBuf,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchReport {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub failures: Vec<(PathBuf, String)>,
}

/// Discover all `.pdf` files in `in_dir` (non-recursive) and apply
/// `recipe` to each, writing outputs to `out_dir` with the same filename.
///
/// `on_progress` must be `Sync` because it's invoked from rayon worker
/// threads. The simplest way to satisfy this from the Tauri command layer
/// is a closure that calls `Channel::send` (which is `Sync` by design).
pub fn run_recipe_batch(
    in_dir: &Path,
    out_dir: &Path,
    recipe: &Recipe,
    on_progress: &(dyn Fn(BatchProgress) + Sync),
) -> Result<BatchReport, PdfError> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| PdfError::Other(format!("create out_dir: {e}")))?;

    let mut inputs: Vec<PathBuf> = std::fs::read_dir(in_dir)
        .map_err(|e| PdfError::Other(format!("read_dir: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
        })
        .collect();
    inputs.sort();
    let total = inputs.len();

    let failures: Mutex<Vec<(PathBuf, String)>> = Mutex::new(vec![]);
    let succeeded: Mutex<usize> = Mutex::new(0);

    inputs.par_iter().enumerate().for_each(|(idx, input)| {
        on_progress(BatchProgress::FileStarted {
            file_index: idx,
            path: input.clone(),
        });
        let Some(fname) = input.file_name() else {
            failures
                .lock()
                .unwrap()
                .push((input.clone(), "input has no filename".into()));
            on_progress(BatchProgress::FileFailed {
                file_index: idx,
                path: input.clone(),
                error: "input has no filename".into(),
            });
            return;
        };
        let out = out_dir
            .join(fname)
            .with_extension(recipe.output_extension());
        let inner_cb = |p: Progress| {
            on_progress(BatchProgress::StepProgress {
                file_index: idx,
                inner: p,
            });
        };
        match run_recipe(input, &out, recipe, &inner_cb) {
            Ok(_) => {
                *succeeded.lock().unwrap() += 1;
                on_progress(BatchProgress::FileCompleted {
                    file_index: idx,
                    path: input.clone(),
                });
            }
            Err(e) => {
                let msg = e.to_string();
                failures.lock().unwrap().push((input.clone(), msg.clone()));
                on_progress(BatchProgress::FileFailed {
                    file_index: idx,
                    path: input.clone(),
                    error: msg,
                });
            }
        }
    });

    let failures = failures.into_inner().unwrap();
    let succeeded = succeeded.into_inner().unwrap();
    Ok(BatchReport {
        total,
        succeeded,
        failed: failures.len(),
        failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::atelier::recipe::Step;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    #[test]
    fn batch_processes_three_files_with_recipe() {
        let in_dir = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        for i in 0..3 {
            make_n_page_pdf(&in_dir.path().join(format!("doc{i}.pdf")), 1);
        }
        let recipe = Recipe {
            name: "Test".into(),
            version: 1,
            steps: vec![Step::Watermark {
                text: "DRAFT".into(),
                opacity: 0.3,
            }],
        };
        let report = run_recipe_batch(in_dir.path(), out_dir.path(), &recipe, &|_| {}).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.succeeded, 3);
        assert_eq!(report.failed, 0);
        for i in 0..3 {
            assert!(out_dir.path().join(format!("doc{i}.pdf")).exists());
        }
    }

    #[test]
    fn batch_skips_non_pdf_files() {
        let in_dir = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        make_n_page_pdf(&in_dir.path().join("a.pdf"), 1);
        std::fs::write(in_dir.path().join("readme.txt"), b"hi").unwrap();
        std::fs::write(in_dir.path().join("notes.md"), b"hi").unwrap();

        let recipe = Recipe {
            name: "T".into(),
            version: 1,
            steps: vec![],
        };
        let report = run_recipe_batch(in_dir.path(), out_dir.path(), &recipe, &|_| {}).unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(report.succeeded, 1);
    }

    #[test]
    fn batch_records_per_file_failures_without_aborting() {
        let in_dir = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        // 2 valid + 1 corrupt (just bytes, not a real PDF).
        make_n_page_pdf(&in_dir.path().join("good1.pdf"), 1);
        make_n_page_pdf(&in_dir.path().join("good2.pdf"), 1);
        std::fs::write(in_dir.path().join("bad.pdf"), b"not a pdf").unwrap();

        let recipe = Recipe {
            name: "T".into(),
            version: 1,
            steps: vec![Step::Watermark {
                text: "X".into(),
                opacity: 0.3,
            }],
        };
        let report = run_recipe_batch(in_dir.path(), out_dir.path(), &recipe, &|_| {}).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].0.ends_with("bad.pdf"));
    }

    #[test]
    fn batch_streams_progress_events() {
        let in_dir = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        make_n_page_pdf(&in_dir.path().join("a.pdf"), 1);
        make_n_page_pdf(&in_dir.path().join("b.pdf"), 1);

        let recipe = Recipe {
            name: "T".into(),
            version: 1,
            steps: vec![Step::Watermark {
                text: "X".into(),
                opacity: 0.3,
            }],
        };

        let events: Arc<Mutex<Vec<BatchProgress>>> = Arc::new(Mutex::new(vec![]));
        let ev2 = events.clone();
        let report = run_recipe_batch(in_dir.path(), out_dir.path(), &recipe, &move |p| {
            ev2.lock().unwrap().push(p);
        })
        .unwrap();
        assert_eq!(report.succeeded, 2);
        let e = events.lock().unwrap();
        // Expect at least 2 FileStarted + 2 FileCompleted events
        let started = e
            .iter()
            .filter(|p| matches!(p, BatchProgress::FileStarted { .. }))
            .count();
        let completed = e
            .iter()
            .filter(|p| matches!(p, BatchProgress::FileCompleted { .. }))
            .count();
        assert_eq!(started, 2);
        assert_eq!(completed, 2);
    }

    #[test]
    fn empty_input_dir_returns_zero_total() {
        let in_dir = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        let recipe = Recipe {
            name: "T".into(),
            version: 1,
            steps: vec![],
        };
        let report = run_recipe_batch(in_dir.path(), out_dir.path(), &recipe, &|_| {}).unwrap();
        assert_eq!(report.total, 0);
        assert_eq!(report.succeeded, 0);
    }

    #[test]
    fn batch_convert_to_docx_writes_docx_filenames() {
        // The killer paralegal-bulk flow: drop a folder of 3 PDFs, recipe
        // ends in ConvertToDocx, output dir should contain 3 `.docx`s
        // (not `.pdf`s).
        let in_dir = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        for i in 0..3 {
            make_n_page_pdf(&in_dir.path().join(format!("brief{i}.pdf")), 1);
        }
        let recipe = Recipe {
            name: "PDF → Word (batch)".into(),
            version: 1,
            steps: vec![Step::ConvertToDocx {
                detect_tables: true,
                detect_lists: true,
                heading_size_ratio: 1.25,
            }],
        };
        let report = run_recipe_batch(in_dir.path(), out_dir.path(), &recipe, &|_| {}).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.succeeded, 3);
        for i in 0..3 {
            let docx = out_dir.path().join(format!("brief{i}.docx"));
            assert!(docx.exists(), "expected {:?}", docx);
            // No matching .pdf written.
            let pdf = out_dir.path().join(format!("brief{i}.pdf"));
            assert!(!pdf.exists(), "should NOT exist: {:?}", pdf);
            // First bytes are ZIP magic.
            let bytes = std::fs::read(&docx).unwrap();
            assert_eq!(&bytes[0..4], b"PK\x03\x04");
        }
    }
}
