// Reflow — offline PDF → Word (.docx) conversion.
//
// Pipeline: extract TextRuns from PDF → cluster into Blocks → emit OOXML.
// Each layer lives in its own submodule and is independently testable.
//
// v3.14.0 ships the scaffold + Task 1 stub. Tasks 2..10 add the real
// implementation per `docs/plans/2026-05-23-v3.14.0-reflow-pdf-to-word.md`.

pub mod errors;
pub mod extract;
pub mod layout;
pub mod tables;
pub mod types;

pub use errors::ReflowError;
pub use types::{Block, ListKind, ReflowOptions, ReflowReport, TextRun};

use std::path::Path;

/// Convert a PDF at `input` to a `.docx` at `output`.
///
/// v3.14.0 Task 1 ships the surface; the pipeline returns
/// `Err(ReflowError::NotYetImplemented)` for now. Tasks 2–6 will fill it in.
pub fn convert_to_docx(
    input: &Path,
    output: &Path,
    _opts: &ReflowOptions,
) -> Result<ReflowReport, ReflowError> {
    if !input.exists() {
        return Err(ReflowError::InputMissing(input.display().to_string()));
    }
    // Cheap up-front writability check so callers fail fast.
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(ReflowError::OutputNotWritable(output.display().to_string()));
        }
    }
    Err(ReflowError::NotYetImplemented)
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
    fn convert_to_docx_returns_not_implemented_for_now() {
        // Existing input file — scaffold returns NotYetImplemented (Task 1).
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"%PDF-1.4\n%%EOF\n").unwrap();
        let out = tempfile::NamedTempFile::new().unwrap();
        let result = convert_to_docx(tmp.path(), out.path(), &ReflowOptions::default());
        assert!(matches!(result, Err(ReflowError::NotYetImplemented)));
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
