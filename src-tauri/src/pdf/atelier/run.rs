//! `run_recipe` — pipe one PDF through every step in order.
//!
//! Each step writes to a temp scratch file in the same per-run tempdir.
//! On success the final scratch is copied to the user-requested output.
//! A caller-supplied `on_progress` callback receives one event when a
//! step starts and one when it finishes (or fails). The driver does
//! NOT abort on a recipe-step error — it returns immediately so the
//! batch driver can mark the file failed and move on.

use crate::pdf::atelier::recipe::{Recipe, Step};
use crate::pdf::PdfError;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Progress {
    Started {
        step_index: usize,
        total_steps: usize,
        kind: String,
    },
    Completed {
        step_index: usize,
        kind: String,
    },
    Failed {
        step_index: usize,
        kind: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeReport {
    pub steps_completed: usize,
    pub output_path: PathBuf,
}

/// Apply every step in `recipe` to `input`, writing the final PDF to
/// `output`. Each step is given a fresh scratch file inside an internal
/// `tempfile::TempDir`. Progress events are emitted synchronously via
/// `on_progress`; callers that need a `Send + Sync` closure can wrap
/// in `Arc<Mutex<...>>`.
pub fn run_recipe(
    input: &Path,
    output: &Path,
    recipe: &Recipe,
    on_progress: &dyn Fn(Progress),
) -> Result<RecipeReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }

    let scratch = tempfile::tempdir().map_err(|e| PdfError::Other(format!("tempdir: {e}")))?;
    let mut current: PathBuf = input.to_path_buf();
    let total = recipe.steps.len();

    for (i, step) in recipe.steps.iter().enumerate() {
        let kind = step_kind(step);
        on_progress(Progress::Started {
            step_index: i,
            total_steps: total,
            kind: kind.clone(),
        });
        let next = scratch.path().join(format!("step-{i}.pdf"));
        match apply_step(&current, &next, step) {
            Ok(()) => {
                on_progress(Progress::Completed {
                    step_index: i,
                    kind,
                });
                current = next;
            }
            Err(e) => {
                let msg = e.to_string();
                on_progress(Progress::Failed {
                    step_index: i,
                    kind,
                    error: msg,
                });
                return Err(e);
            }
        }
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PdfError::Other(format!("mkdir output parent: {e}")))?;
    }
    std::fs::copy(&current, output).map_err(|e| PdfError::Other(format!("copy output: {e}")))?;
    Ok(RecipeReport {
        steps_completed: total,
        output_path: output.to_path_buf(),
    })
}

fn step_kind(s: &Step) -> String {
    match s {
        Step::Ocr { .. } => "ocr",
        Step::AutoRedact { .. } => "auto-redact",
        Step::Bates { .. } => "bates",
        Step::Watermark { .. } => "watermark",
        Step::Flatten { .. } => "flatten",
        Step::Compactor => "compactor",
        Step::Linearize => "linearize",
        Step::ConvertToDocx { .. } => "convert-to-docx",
        Step::ConvertToXlsx { .. } => "convert-to-xlsx",
        Step::ConvertToPptx { .. } => "convert-to-pptx",
        Step::ConvertToMarkdown { .. } => "convert-to-markdown",
        Step::ConvertToHtml { .. } => "convert-to-html",
    }
    .into()
}

fn apply_step(input: &Path, output: &Path, step: &Step) -> Result<(), PdfError> {
    match step {
        Step::Watermark { text, opacity } => {
            let opts = crate::pdf::watermark::WatermarkOpts {
                text,
                opacity: *opacity,
                ..Default::default()
            };
            crate::pdf::watermark::watermark(input, output, opts, &[])?;
            Ok(())
        }
        Step::Flatten { dpi } => {
            let opts = crate::pdf::flatten::FlattenOpts {
                mode: crate::pdf::flatten::FlattenMode::Raster { dpi: *dpi },
                ..Default::default()
            };
            crate::pdf::flatten::flatten(input, output, opts)?;
            Ok(())
        }
        Step::Compactor => {
            let opts = crate::pdf::compactor::CompactOptions::default();
            crate::pdf::compactor::compact(input, output, opts)?;
            Ok(())
        }
        Step::Linearize => {
            crate::pdf::streamline::linearize_pdf(input, output)?;
            Ok(())
        }
        Step::Ocr { language } => {
            let opts = crate::pdf::ocr::OcrOpts {
                lang: language.clone(),
                ..Default::default()
            };
            crate::pdf::ocr::ocr(input, output, &opts)?;
            Ok(())
        }
        Step::AutoRedact { patterns, presets } => {
            let opts = crate::pdf::auto_redact::AutoRedactOpts {
                patterns: patterns.clone(),
                presets: presets.clone(),
                gray: 0.0,
            };
            crate::pdf::auto_redact::auto_redact(input, output, opts)?;
            Ok(())
        }
        Step::Bates {
            prefix,
            start,
            digits,
        } => {
            let opts = crate::pdf::bates::BatesOpts {
                prefix: prefix.clone(),
                start_at: *start,
                digits: *digits,
                ..Default::default()
            };
            crate::pdf::bates::apply_bates(input, output, &opts)?;
            Ok(())
        }
        Step::ConvertToDocx {
            detect_tables,
            detect_lists,
            heading_size_ratio,
        } => {
            let opts = crate::pdf::reflow::types::ReflowOptions {
                detect_tables: *detect_tables,
                detect_lists: *detect_lists,
                heading_size_ratio: *heading_size_ratio,
                ..Default::default()
            };
            crate::pdf::reflow::convert_to_docx(input, output, &opts)
                .map_err(|e| PdfError::Other(format!("reflow: {e}")))?;
            Ok(())
        }
        Step::ConvertToXlsx {
            type_numbers,
            type_dates,
            include_non_table_text,
        } => {
            let opts = crate::pdf::tabulate::TabulateOptions {
                type_numbers: *type_numbers,
                type_dates: *type_dates,
                include_non_table_text: *include_non_table_text,
                ..Default::default()
            };
            crate::pdf::tabulate::convert_to_xlsx(input, output, &opts)
                .map_err(|e| PdfError::Other(format!("tabulate: {e}")))?;
            Ok(())
        }
        Step::ConvertToPptx {
            include_speaker_notes,
            detect_titles,
        } => {
            let opts = crate::pdf::slide::SlideOptions {
                include_speaker_notes: *include_speaker_notes,
                detect_titles: *detect_titles,
                embed_page_images: false,
            };
            crate::pdf::slide::convert_to_pptx(input, output, &opts)
                .map_err(|e| PdfError::Other(format!("slide: {e}")))?;
            Ok(())
        }
        Step::ConvertToMarkdown {
            detect_tables,
            detect_lists,
            flavour_gfm,
        } => {
            let opts = crate::pdf::markdown::MarkdownOptions {
                detect_tables: *detect_tables,
                detect_lists: *detect_lists,
                flavour: if *flavour_gfm {
                    crate::pdf::markdown::MarkdownFlavour::Gfm
                } else {
                    crate::pdf::markdown::MarkdownFlavour::CommonMark
                },
                ..Default::default()
            };
            crate::pdf::markdown::convert_to_markdown(input, output, &opts)
                .map_err(|e| PdfError::Other(format!("markdown: {e}")))?;
            Ok(())
        }
        Step::ConvertToHtml {
            detect_tables,
            detect_lists,
            semantic_tags,
            embed_css,
        } => {
            let opts = crate::pdf::markdown::HtmlOptions {
                detect_tables: *detect_tables,
                detect_lists: *detect_lists,
                semantic_tags: *semantic_tags,
                embed_css: *embed_css,
                ..Default::default()
            };
            crate::pdf::markdown::convert_to_html(input, output, &opts)
                .map_err(|e| PdfError::Other(format!("html: {e}")))?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    #[test]
    fn recipe_runs_two_steps_in_order() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 2);
        let output = dir.path().join("out.pdf");

        let recipe = Recipe {
            name: "Test".into(),
            version: 1,
            steps: vec![
                Step::Watermark {
                    text: "DRAFT".into(),
                    opacity: 0.3,
                },
                Step::Bates {
                    prefix: "T".into(),
                    start: 1,
                    digits: 4,
                },
            ],
        };

        let events: Arc<Mutex<Vec<Progress>>> = Arc::new(Mutex::new(vec![]));
        let ev2 = events.clone();
        let report = run_recipe(&input, &output, &recipe, &move |p| {
            ev2.lock().unwrap().push(p);
        })
        .expect("recipe ran");

        assert!(output.exists(), "output PDF written");
        assert!(std::fs::metadata(&output).unwrap().len() > 0);
        assert_eq!(report.steps_completed, 2);
        let e = events.lock().unwrap();
        // 1 Started + 1 Completed per step → exactly 4 events.
        assert_eq!(e.len(), 4, "events = {:?}", e.len());
        // First event must be Started for step 0.
        assert!(matches!(e[0], Progress::Started { step_index: 0, .. }));
        // Last event must be Completed for step 1.
        assert!(matches!(e[3], Progress::Completed { step_index: 1, .. }));
    }

    #[test]
    fn recipe_runs_linearize_step() {
        // Atelier wiring for Step::Linearize — proves you can drop a folder
        // of PDFs through Atelier and get Fast Web View output as a step.
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        // Need a non-trivial document so the linearizer has real objects to walk.
        make_n_page_pdf(&input, 3);
        let output = dir.path().join("out.pdf");

        let recipe = Recipe {
            name: "Linearize only".into(),
            version: 1,
            steps: vec![Step::Linearize],
        };

        let report = run_recipe(&input, &output, &recipe, &|_| {}).expect("recipe ran");
        assert_eq!(report.steps_completed, 1);
        assert!(output.exists(), "linearized output written");

        // Round-trip: the inspector should now classify our own output as Linearized.
        let (status, _stats) =
            crate::pdf::streamline::is_linearized(&output).expect("inspect output");
        assert_eq!(
            status,
            crate::pdf::streamline::LinearizationStatus::Linearized,
            "Atelier Linearize step produced a Fast Web View PDF"
        );
    }

    #[test]
    fn empty_recipe_copies_input_to_output() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 1);
        let output = dir.path().join("out.pdf");
        let r = Recipe {
            name: "Empty".into(),
            version: 1,
            steps: vec![],
        };
        let report = run_recipe(&input, &output, &r, &|_| {}).unwrap();
        assert_eq!(report.steps_completed, 0);
        assert!(output.exists());
    }

    #[test]
    fn missing_input_returns_err_not_panic() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("nope.pdf");
        let output = dir.path().join("out.pdf");
        let r = Recipe {
            name: "Bad".into(),
            version: 1,
            steps: vec![Step::Watermark {
                text: "x".into(),
                opacity: 0.3,
            }],
        };
        let res = run_recipe(&input, &output, &r, &|_| {});
        assert!(res.is_err());
        assert!(!output.exists());
    }

    #[test]
    fn step_failure_emits_failed_event() {
        // Empty watermark text triggers PdfError::Other from the watermark module.
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 1);
        let output = dir.path().join("out.pdf");
        let r = Recipe {
            name: "Bad step".into(),
            version: 1,
            steps: vec![Step::Watermark {
                text: String::new(),
                opacity: 0.3,
            }],
        };
        let events: Arc<Mutex<Vec<Progress>>> = Arc::new(Mutex::new(vec![]));
        let ev2 = events.clone();
        let res = run_recipe(&input, &output, &r, &move |p| {
            ev2.lock().unwrap().push(p);
        });
        assert!(res.is_err());
        let e = events.lock().unwrap();
        assert!(matches!(e.last(), Some(Progress::Failed { .. })));
    }

    #[test]
    fn recipe_runs_convert_to_docx_terminal_step() {
        // Killer paralegal flow proof: a recipe ending in ConvertToDocx
        // produces a real `.docx` file (ZIP archive starting with PK\x03\x04).
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 2);
        let output = dir.path().join("out.docx");

        let recipe = Recipe {
            name: "PDF → Word".into(),
            version: 1,
            steps: vec![Step::ConvertToDocx {
                detect_tables: true,
                detect_lists: true,
                heading_size_ratio: 1.25,
            }],
        };
        let report = run_recipe(&input, &output, &recipe, &|_| {}).expect("recipe ran");
        assert_eq!(report.steps_completed, 1);
        assert!(output.exists(), "DOCX output written");
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.len() > 4, "DOCX has content");
        assert_eq!(
            &bytes[0..4],
            b"PK\x03\x04",
            "DOCX is a valid ZIP archive (paralegal can open in Word)"
        );
    }

    #[test]
    fn recipe_runs_compactor_then_convert_to_docx_chained() {
        // Realistic chain: shrink the PDF first, then convert to Word.
        // Mirrors how a paralegal uses Atelier: pre-process + final hand-off.
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 1);
        let output = dir.path().join("out.docx");
        let recipe = Recipe {
            name: "Compact + Word".into(),
            version: 1,
            steps: vec![
                Step::Compactor,
                Step::ConvertToDocx {
                    detect_tables: true,
                    detect_lists: true,
                    heading_size_ratio: 1.25,
                },
            ],
        };
        let report = run_recipe(&input, &output, &recipe, &|_| {}).expect("recipe ran");
        assert_eq!(report.steps_completed, 2);
        assert!(output.exists());
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
    }
}
