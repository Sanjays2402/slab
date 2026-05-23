//! Public end-to-end driver for true (destructive) redaction.
//!
//! Orchestrates the pipeline:
//!   1. Load PDF.
//!   2. For each page: convert `RedactRect`s to user-space points.
//!   3. Excise text-show operators whose bbox intersects (text_stream).
//!   4. Scrub annotations whose /Rect intersects (annotations).
//!   5. Sanitize document-level metadata (sanitize).
//!   6. Paint visible black bars via super::redact (visual cover).
//!   7. Save to output path with a full xref rewrite (no incremental history).

use crate::pdf::redact::{redact as paint_visible_bars, RedactOpts, RedactRect};
use crate::pdf::redact_true::annotations::scrub_annotations_on_page;
use crate::pdf::redact_true::sanitize::{sanitize_document, SanitizeReport};
use crate::pdf::redact_true::text_stream::{excise_text_on_page, rect_to_points};
use crate::pdf::PdfError;
use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrueRedactReport {
    pub rects_painted: u32,
    pub text_runs_excised: u32,
    pub annotations_removed: u32,
    pub sanitize: SanitizeFlatReport,
}

/// Flat, JSON-serializable mirror of `SanitizeReport` (frontend-friendly).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SanitizeFlatReport {
    pub info_fields_cleared: u32,
    pub xmp_metadata_removed: bool,
    pub embedded_files_removed: u32,
    pub javascript_removed: u32,
    pub structure_tree_removed: bool,
}

impl From<SanitizeReport> for SanitizeFlatReport {
    fn from(r: SanitizeReport) -> Self {
        Self {
            info_fields_cleared: r.info_fields_cleared,
            xmp_metadata_removed: r.xmp_metadata_removed,
            embedded_files_removed: r.embedded_files_removed,
            javascript_removed: r.javascript_removed,
            structure_tree_removed: r.structure_tree_removed,
        }
    }
}

/// Run the full true-redaction pipeline.
///
/// `input` is the source PDF, `output` is where the redacted copy is written.
/// `opts.rects` defines the regions (in page-percentage space, like the
/// visible-bar `redact` API) and `opts.gray` is the bar shade (0=black).
pub fn redact_true(
    input: &Path,
    output: &Path,
    opts: RedactOpts,
) -> Result<TrueRedactReport, PdfError> {
    if opts.rects.is_empty() {
        return Err(PdfError::Other("No redaction rectangles supplied.".into()));
    }

    // STAGE A — text + annotation scrub on a working doc.
    let mut doc = Document::load(input)?;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();

    // Group rects by 1-based page number.
    let mut by_page: std::collections::HashMap<u32, Vec<&RedactRect>> =
        std::collections::HashMap::new();
    for r in &opts.rects {
        by_page.entry(r.page).or_default().push(r);
    }

    let mut text_runs_excised = 0u32;
    let mut annotations_removed = 0u32;

    for (page_num, rects) in &by_page {
        let Some((_, page_id)) = page_ids.iter().find(|(n, _)| n == page_num) else {
            continue;
        };
        let mut pts: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(rects.len());
        for r in rects {
            if let Some(p) = rect_to_points(&doc, *page_id, r) {
                pts.push(p);
            }
        }
        if pts.is_empty() {
            continue;
        }
        text_runs_excised += excise_text_on_page(&mut doc, *page_id, &pts)?;
        annotations_removed += scrub_annotations_on_page(&mut doc, *page_id, &pts)?;
    }

    // STAGE B — document-level metadata scrub (also drops incremental history).
    let sanitize_report = sanitize_document(&mut doc)?;

    // Save the scrubbed doc to a tempfile so STAGE C (the painter) can layer
    // visible bars on top of the already-cleaned object graph.
    let mut tmp = tempfile::NamedTempFile::new()?;
    {
        use std::io::Write;
        let mut buf: Vec<u8> = Vec::new();
        doc.save_to(&mut buf)
            .map_err(|e| PdfError::Other(format!("save_to failed: {e}")))?;
        tmp.write_all(&buf)?;
    }
    let tmp_path = tmp.into_temp_path();

    // STAGE C — paint visible black bars over the cleaned doc.
    let rects_painted = paint_visible_bars(tmp_path.as_ref(), output, opts)?;

    Ok(TrueRedactReport {
        rects_painted,
        text_runs_excised,
        annotations_removed,
        sanitize: sanitize_report.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::redact::{RedactOpts, RedactRect};
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream, StringFormat};

    fn write_tmp_pdf_with_text(text: &str, tx: f32, ty: f32) -> tempfile::TempPath {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
                ),
                Operation::new("Td", vec![Object::Real(tx), Object::Real(ty)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        text.as_bytes().to_vec(),
                        StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => stream_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let info = doc.add_object(dictionary! {
            "Title" => Object::string_literal("LEAKY-TITLE"),
            "Author" => Object::string_literal("Alice"),
        });
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", cat);
        doc.trailer.set("Info", info);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.into_temp_path();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        std::fs::write(&path, buf).unwrap();
        path
    }

    #[test]
    fn redact_true_excises_text_and_paints_bar() {
        let input = write_tmp_pdf_with_text("SECRET-DATA", 100.0, 700.0);
        let out_dir = tempfile::tempdir().unwrap();
        let output = out_dir.path().join("out.pdf");

        let report = redact_true(
            input.as_ref(),
            &output,
            RedactOpts {
                rects: vec![RedactRect {
                    page: 1,
                    // 100/595 ≈ 16.8% — 200/595 ≈ 33.6%; 700/842 ≈ 83.1% — 720/842 ≈ 85.5%
                    left_pct: 15.0,
                    bottom_pct: 82.0,
                    right_pct: 34.0,
                    top_pct: 86.0,
                }],
                gray: 0.0,
            },
        )
        .unwrap();

        assert_eq!(report.rects_painted, 1, "visible bar painted");
        assert!(report.text_runs_excised >= 1, "text excised");
        assert!(report.sanitize.info_fields_cleared >= 1, "info scrubbed");

        // Output must not contain the original literal bytes anywhere.
        let bytes = std::fs::read(&output).unwrap();
        assert!(
            !bytes.windows(11).any(|w| w == b"SECRET-DATA"),
            "redacted text bytes still present in output PDF"
        );
        assert!(
            !bytes.windows(11).any(|w| w == b"LEAKY-TITLE"),
            "info dict title still present in output PDF"
        );
    }

    #[test]
    fn redact_true_empty_rects_errors() {
        let input = write_tmp_pdf_with_text("HI", 0.0, 0.0);
        let out_dir = tempfile::tempdir().unwrap();
        let output = out_dir.path().join("out.pdf");
        let r = redact_true(
            input.as_ref(),
            &output,
            RedactOpts {
                rects: vec![],
                gray: 0.0,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn redact_true_leaves_outside_text_intact() {
        let input = write_tmp_pdf_with_text("KEEP-ME-VISIBLE", 50.0, 50.0);
        let out_dir = tempfile::tempdir().unwrap();
        let output = out_dir.path().join("out.pdf");

        let report = redact_true(
            input.as_ref(),
            &output,
            RedactOpts {
                rects: vec![RedactRect {
                    page: 1,
                    left_pct: 80.0,
                    bottom_pct: 80.0,
                    right_pct: 95.0,
                    top_pct: 95.0,
                }],
                gray: 0.0,
            },
        )
        .unwrap();
        assert_eq!(report.text_runs_excised, 0);

        // Note: Info dict scrub happens regardless of region, so we just check
        // the page text isn't lost.
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.windows(15).any(|w| w == b"KEEP-ME-VISIBLE"));
    }
}
