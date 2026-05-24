// Reflow — offline PDF → Word (.docx) conversion.
//
// Pipeline: extract TextRuns from PDF → cluster into Blocks → emit OOXML.
// Each layer lives in its own submodule and is independently testable.
//
// Task 5+6 wire the end-to-end path: `convert_to_docx` reads a PDF, reflows
// it through `extract -> layout -> docx`, and writes a valid `.docx` to disk.

pub mod docx;
pub mod errors;
pub mod extract;
pub mod layout;
pub mod tables;
pub mod types;

pub use errors::ReflowError;
pub use types::{Block, ListKind, ReflowOptions, ReflowReport, TextRun};

use std::path::Path;
use std::time::Instant;

/// Convert a PDF at `input` to a `.docx` at `output`.
///
/// Steps:
///   1. Load the PDF with `lopdf::Document::load`.
///   2. Extract positioned `TextRun`s from every page.
///   3. Reconstruct paragraph / heading / list / table `Block`s.
///   4. Emit an OOXML `.docx` and write the byte blob to `output`.
///
/// Returns a `ReflowReport` summarising how many of each block type shipped.
pub fn convert_to_docx(
    input: &Path,
    output: &Path,
    opts: &ReflowOptions,
) -> Result<ReflowReport, ReflowError> {
    if !input.exists() {
        return Err(ReflowError::InputMissing(input.display().to_string()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(ReflowError::OutputNotWritable(output.display().to_string()));
        }
    }

    let started = Instant::now();
    let doc = lopdf::Document::load(input)?;
    let runs = extract::extract_text_runs(&doc)?;
    let page_count = doc.get_pages().len() as u32;
    let blocks = layout::reconstruct_blocks(&runs, opts);

    // Tally before emission so we can include counts in the report even when
    // the writer happens to coalesce/expand certain blocks (it doesn't today,
    // but the report should reflect what the *layout* layer produced).
    let mut paragraphs = 0u32;
    let mut headings = 0u32;
    let mut list_items = 0u32;
    let mut table_rows = 0u32;
    for b in &blocks {
        match b {
            Block::Body { .. } => paragraphs += 1,
            Block::Heading { .. } => headings += 1,
            Block::ListItem { .. } => list_items += 1,
            Block::TableRow { .. } => table_rows += 1,
        }
    }

    let bytes = docx::write_docx(&blocks)?;
    std::fs::write(output, &bytes)?;

    Ok(ReflowReport {
        pages: page_count,
        paragraphs,
        headings,
        list_items,
        table_rows,
        bytes_written: bytes.len() as u64,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn convert_to_docx_errors_on_missing_input() {
        let opts = ReflowOptions::default();
        let result = convert_to_docx(
            Path::new("/nonexistent/path/does-not-exist.pdf"),
            Path::new("/tmp/should-not-be-created.docx"),
            &opts,
        );
        assert!(matches!(result, Err(ReflowError::InputMissing(_))));
    }

    #[test]
    fn convert_to_docx_errors_on_garbage_pdf() {
        // Existing input file but invalid PDF — lopdf returns Err.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"%PDF-1.4\n%%EOF\n").unwrap();
        let out = tempfile::NamedTempFile::new().unwrap();
        let result = convert_to_docx(tmp.path(), out.path(), &ReflowOptions::default());
        assert!(matches!(result, Err(ReflowError::Pdf(_))));
    }

    #[test]
    fn options_default_matches_doc() {
        let opts = ReflowOptions::default();
        assert!(opts.detect_tables);
        assert!(opts.detect_lists);
        assert!((opts.heading_size_ratio - 1.25).abs() < 1e-6);
        assert!(!opts.preserve_page_breaks);
        assert_eq!(opts.locale, "en");
    }

    #[test]
    fn report_empty_is_zeroed() {
        let r = ReflowReport::empty();
        assert_eq!(r.pages, 0);
        assert_eq!(r.paragraphs, 0);
        assert_eq!(r.headings, 0);
        assert_eq!(r.list_items, 0);
        assert_eq!(r.table_rows, 0);
        assert_eq!(r.bytes_written, 0);
        assert_eq!(r.duration_ms, 0);
    }

    #[test]
    fn text_run_eq_uses_all_fields() {
        let a = TextRun {
            page: 1,
            x: 100.0,
            y: 800.0,
            text: "hi".into(),
            font_name: "Helvetica".into(),
            font_size: 12.0,
            bold: false,
            italic: false,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn convert_to_docx_end_to_end_real_pdf() {
        // Build a real 2-page PDF using the shared test fixture helper,
        // run it through the full pipeline, and assert the resulting `.docx`
        // is a valid ZIP whose `document.xml` contains the source text.
        use crate::pdf::test_fixtures::make_n_page_pdf;
        use std::io::Read;

        let pdf = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(pdf.path(), 2);

        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join("reflowed.docx");

        let report = convert_to_docx(pdf.path(), &out_path, &ReflowOptions::default())
            .expect("convert_to_docx should succeed on a valid 2-page PDF");

        assert_eq!(report.pages, 2, "report.pages = {}", report.pages);
        assert!(report.bytes_written > 0);
        assert!(
            out_path.exists() && std::fs::metadata(&out_path).unwrap().len() > 0,
            ".docx file must exist and be non-empty"
        );

        // Validate that the output is a real OOXML package containing the
        // source text from the PDF (the fixture writes "Slab page 1" / "Slab page 2").
        let bytes = std::fs::read(&out_path).unwrap();
        assert_eq!(
            &bytes[0..4],
            b"PK\x03\x04",
            "output must be a ZIP (PK header)"
        );
        let mut zr = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut doc_xml = String::new();
        zr.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut doc_xml)
            .unwrap();
        assert!(
            doc_xml.contains("Slab page 1") || doc_xml.contains("Slab page 2"),
            "expected at least one page label in document.xml, got: {}",
            doc_xml
        );
        // Required parts must all be present.
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/styles.xml",
            "word/numbering.xml",
            "word/document.xml",
        ] {
            assert!(
                zr.by_name(required).is_ok(),
                "missing required OOXML part: {}",
                required
            );
        }
    }

    #[test]
    fn convert_to_docx_report_pages_matches_pdf_page_count() {
        use crate::pdf::test_fixtures::make_n_page_pdf;
        let pdf = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(pdf.path(), 5);
        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join("five.docx");
        let report = convert_to_docx(pdf.path(), &out_path, &ReflowOptions::default()).unwrap();
        assert_eq!(report.pages, 5);
    }

    #[test]
    fn block_constructors_compile() {
        let _ = Block::Body { text: "p".into() };
        let _ = Block::Heading {
            level: 1,
            text: "H1".into(),
        };
        let _ = Block::ListItem {
            kind: ListKind::Bullet,
            text: "item".into(),
            indent: 0,
        };
        let _ = Block::TableRow {
            cells: vec!["a".into(), "b".into()],
        };
    }
}
