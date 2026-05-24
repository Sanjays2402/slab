//! Markdown — offline PDF → Markdown (.md) + PDF → semantic HTML (.html).
//!
//! Re-uses the reflow pipeline's `extract` + `layout` passes, swapping in
//! Markdown / HTML emitters in place of the OOXML DOCX writer.

pub mod errors;
pub mod html;
pub mod md;
pub mod types;

pub use errors::MarkdownError;
pub use types::{HtmlOptions, HtmlReport, MarkdownFlavour, MarkdownOptions, MarkdownReport};

use crate::pdf::reflow::{self, Block};
use std::path::Path;
use std::time::Instant;

/// Convert a PDF at `input` to a Markdown file at `output`.
pub fn convert_to_markdown(
    input: &Path,
    output: &Path,
    opts: &MarkdownOptions,
) -> Result<MarkdownReport, MarkdownError> {
    if !input.exists() {
        return Err(MarkdownError::InputMissing(input.display().to_string()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(MarkdownError::OutputNotWritable(
                output.display().to_string(),
            ));
        }
    }

    let started = Instant::now();
    let doc = lopdf::Document::load(input)?;
    let runs = reflow::extract::extract_text_runs(&doc)?;
    let page_count = doc.get_pages().len() as u32;

    // Bridge MarkdownOptions to ReflowOptions (shares the layout knobs).
    let ropts = reflow::types::ReflowOptions {
        detect_tables: opts.detect_tables,
        detect_lists: opts.detect_lists,
        heading_size_ratio: opts.heading_size_ratio,
        preserve_page_breaks: opts.preserve_page_breaks,
        locale: "en".to_string(),
    };
    let blocks = reflow::layout::reconstruct_blocks(&runs, &ropts);

    let mut report = MarkdownReport::empty();
    report.pages = page_count;
    let mut in_table = false;
    for b in &blocks {
        match b {
            Block::Body { .. } => {
                report.paragraphs += 1;
                in_table = false;
            }
            Block::Heading { .. } => {
                report.headings += 1;
                in_table = false;
            }
            Block::ListItem { .. } => {
                report.list_items += 1;
                in_table = false;
            }
            Block::TableRow { .. } => {
                if !in_table {
                    report.tables += 1;
                    in_table = true;
                }
            }
        }
    }

    let text = md::emit_markdown(&blocks, opts);
    let bytes = text.as_bytes();
    std::fs::write(output, bytes)?;

    report.bytes_written = bytes.len() as u64;
    report.duration_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

/// Convert a PDF at `input` to a semantic HTML file at `output`.
pub fn convert_to_html(
    input: &Path,
    output: &Path,
    opts: &HtmlOptions,
) -> Result<HtmlReport, MarkdownError> {
    if !input.exists() {
        return Err(MarkdownError::InputMissing(input.display().to_string()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(MarkdownError::OutputNotWritable(
                output.display().to_string(),
            ));
        }
    }

    let started = Instant::now();
    let doc = lopdf::Document::load(input)?;
    let runs = reflow::extract::extract_text_runs(&doc)?;
    let page_count = doc.get_pages().len() as u32;

    let ropts = reflow::types::ReflowOptions {
        detect_tables: opts.detect_tables,
        detect_lists: opts.detect_lists,
        heading_size_ratio: opts.heading_size_ratio,
        preserve_page_breaks: false,
        locale: "en".to_string(),
    };
    let blocks = reflow::layout::reconstruct_blocks(&runs, &ropts);

    let mut report = HtmlReport::empty();
    report.pages = page_count;
    let mut in_table = false;
    for b in &blocks {
        match b {
            Block::Body { .. } => {
                report.paragraphs += 1;
                in_table = false;
            }
            Block::Heading { .. } => {
                report.headings += 1;
                in_table = false;
            }
            Block::ListItem { .. } => {
                report.list_items += 1;
                in_table = false;
            }
            Block::TableRow { .. } => {
                if !in_table {
                    report.tables += 1;
                    in_table = true;
                }
            }
        }
    }

    let text = html::emit_html(&blocks, opts);
    let bytes = text.as_bytes();
    std::fs::write(output, bytes)?;

    report.bytes_written = bytes.len() as u64;
    report.duration_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::types::{HtmlOptions, MarkdownFlavour, MarkdownOptions};
    use super::*;
    use std::path::Path;

    #[test]
    fn markdown_options_default_sane() {
        let o = MarkdownOptions::default();
        assert!(o.detect_tables);
        assert!(o.detect_lists);
        assert!(!o.preserve_page_breaks);
        assert!((o.heading_size_ratio - 1.25).abs() < 1e-6);
        assert_eq!(o.flavour, MarkdownFlavour::Gfm);
    }

    #[test]
    fn html_options_default_sane() {
        let o = HtmlOptions::default();
        assert!(o.semantic_tags);
        assert!(o.embed_css);
        assert!(o.detect_tables);
    }

    #[test]
    fn reports_empty_zeroed() {
        let m = MarkdownReport::empty();
        assert_eq!(m.pages, 0);
        assert_eq!(m.bytes_written, 0);
        let h = HtmlReport::empty();
        assert_eq!(h.pages, 0);
        assert_eq!(h.bytes_written, 0);
    }

    #[test]
    fn convert_to_markdown_errors_on_missing_input() {
        let r = convert_to_markdown(
            Path::new("/nonexistent/markdown-input.pdf"),
            Path::new("/tmp/should-not-exist.md"),
            &MarkdownOptions::default(),
        );
        assert!(matches!(r, Err(MarkdownError::InputMissing(_))));
    }

    #[test]
    fn convert_to_html_errors_on_missing_input() {
        let r = convert_to_html(
            Path::new("/nonexistent/html-input.pdf"),
            Path::new("/tmp/should-not-exist.html"),
            &HtmlOptions::default(),
        );
        assert!(matches!(r, Err(MarkdownError::InputMissing(_))));
    }

    #[test]
    fn convert_to_markdown_end_to_end_real_pdf() {
        use crate::pdf::test_fixtures::make_n_page_pdf;

        let pdf = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(pdf.path(), 2);

        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join("out.md");

        let report = convert_to_markdown(pdf.path(), &out_path, &MarkdownOptions::default())
            .expect("convert_to_markdown should succeed on a valid 2-page PDF");

        assert_eq!(report.pages, 2);
        assert!(report.bytes_written > 0);
        let body = std::fs::read_to_string(&out_path).unwrap();
        assert!(
            body.contains("Slab page 1") || body.contains("Slab page 2"),
            "expected at least one page label in Markdown output, got: {}",
            body
        );
    }

    #[test]
    fn convert_to_html_end_to_end_real_pdf() {
        use crate::pdf::test_fixtures::make_n_page_pdf;

        let pdf = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(pdf.path(), 2);

        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join("out.html");

        let report = convert_to_html(pdf.path(), &out_path, &HtmlOptions::default())
            .expect("convert_to_html should succeed on a valid 2-page PDF");

        assert_eq!(report.pages, 2);
        assert!(report.bytes_written > 0);
        let body = std::fs::read_to_string(&out_path).unwrap();
        assert!(body.starts_with("<!DOCTYPE html>"));
        assert!(body.contains("<article"));
        assert!(
            body.contains("Slab page 1") || body.contains("Slab page 2"),
            "expected page label in HTML output, got: {}",
            body
        );
    }
}
