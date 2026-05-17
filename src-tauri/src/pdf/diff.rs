// PDF diff & compare — `pdf::diff`.
//
// Aligns two PDFs page-by-page (1:1 by index for v1; v2 will support
// content-similarity alignment) and produces a structured line-level diff
// using the `similar` crate's text-diff engine.
//
// All public types implement `serde::{Serialize, Deserialize}` so Tauri can
// hand them straight to the Svelte panel. Pure-Rust, no native deps, no
// unsafe.

use crate::pdf::extract::extract_text;
use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::path::{Path, PathBuf};

/// One kind of edit produced by the line-diff engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffOp {
    /// Line is present in both old and new with identical content.
    Equal,
    /// Line is only in the new document.
    Insert,
    /// Line is only in the old document.
    Delete,
}

/// One line-level edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineDiff {
    pub op: DiffOp,
    /// 1-based index into the old page's lines, if the op consumes one.
    pub old_line: Option<u32>,
    /// 1-based index into the new page's lines, if the op consumes one.
    pub new_line: Option<u32>,
    /// Verbatim line content (no trailing newline).
    pub text: String,
}

/// Page-level diff aggregate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub added: u32,
    pub removed: u32,
    /// Lines that appear in both but are flagged as a near-match (consecutive
    /// `Delete`+`Insert` pair, treated as one logical change in the count).
    pub changed: u32,
}

/// One page in the doc-level diff.
///
/// If the two PDFs have different page counts, the trailing pages are emitted
/// as one-sided entries: `old_page = Some` and `new_page = None` (or vice
/// versa). The `lines` vec captures the whole missing side as Delete/Insert
/// ops respectively so the UI can show them too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageDiff {
    pub old_page: Option<u32>,
    pub new_page: Option<u32>,
    pub lines: Vec<LineDiff>,
    pub summary: DiffSummary,
}

/// Top-level doc diff DTO returned by the Tauri command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocDiff {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub old_page_count: u32,
    pub new_page_count: u32,
    pub pages: Vec<PageDiff>,
    pub total: DiffSummary,
}

/// Diff two PDFs page-by-page (1:1 by page index).
///
/// For each aligned page, the page's extracted text is split on newlines and
/// run through `TextDiff` to produce a line-level edit script. Trailing pages
/// in the longer doc are emitted as one-sided `PageDiff` entries.
pub fn diff_pdfs(old: &Path, new: &Path) -> Result<DocDiff, PdfError> {
    if !old.exists() {
        return Err(PdfError::InputMissing(old.display().to_string()));
    }
    if !new.exists() {
        return Err(PdfError::InputMissing(new.display().to_string()));
    }

    let old_pages = extract_text(old)?;
    let new_pages = extract_text(new)?;
    let old_count = old_pages.len() as u32;
    let new_count = new_pages.len() as u32;
    let max = old_count.max(new_count);

    let mut pages: Vec<PageDiff> = Vec::with_capacity(max as usize);
    let mut total = DiffSummary::default();

    for i in 0..max {
        let old_text = old_pages.get(i as usize).map(String::as_str);
        let new_text = new_pages.get(i as usize).map(String::as_str);
        let page_diff = match (old_text, new_text) {
            (Some(o), Some(n)) => diff_page(Some(i + 1), Some(i + 1), o, n),
            (Some(o), None) => diff_page(Some(i + 1), None, o, ""),
            (None, Some(n)) => diff_page(None, Some(i + 1), "", n),
            // Cannot happen because `max` is the larger count.
            (None, None) => continue,
        };
        total.added = total.added.saturating_add(page_diff.summary.added);
        total.removed = total.removed.saturating_add(page_diff.summary.removed);
        total.changed = total.changed.saturating_add(page_diff.summary.changed);
        pages.push(page_diff);
    }

    Ok(DocDiff {
        old_path: old.to_path_buf(),
        new_path: new.to_path_buf(),
        old_page_count: old_count,
        new_page_count: new_count,
        pages,
        total,
    })
}

/// Build a single `PageDiff` from the two raw page-text strings.
fn diff_page(
    old_page: Option<u32>,
    new_page: Option<u32>,
    old_text: &str,
    new_text: &str,
) -> PageDiff {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut lines: Vec<LineDiff> = Vec::new();
    let mut summary = DiffSummary::default();

    let mut old_lineno: u32 = 0;
    let mut new_lineno: u32 = 0;
    // Track the previous op for the "Replace" (consecutive Delete+Insert)
    // heuristic that bumps `changed` instead of inflating add+remove.
    let mut last_was_delete = false;

    for change in diff.iter_all_changes() {
        // `similar` keeps trailing newlines on each line; strip exactly one
        // so the UI doesn't render visible blanks.
        let text = trim_trailing_newline(change.value());
        match change.tag() {
            ChangeTag::Equal => {
                old_lineno += 1;
                new_lineno += 1;
                lines.push(LineDiff {
                    op: DiffOp::Equal,
                    old_line: Some(old_lineno),
                    new_line: Some(new_lineno),
                    text,
                });
                last_was_delete = false;
            }
            ChangeTag::Delete => {
                old_lineno += 1;
                summary.removed = summary.removed.saturating_add(1);
                lines.push(LineDiff {
                    op: DiffOp::Delete,
                    old_line: Some(old_lineno),
                    new_line: None,
                    text,
                });
                last_was_delete = true;
            }
            ChangeTag::Insert => {
                new_lineno += 1;
                summary.added = summary.added.saturating_add(1);
                lines.push(LineDiff {
                    op: DiffOp::Insert,
                    old_line: None,
                    new_line: Some(new_lineno),
                    text,
                });
                if last_was_delete {
                    // One logical "replace" — bump `changed` (we keep the
                    // raw added/removed counts as-is so the UI can show both
                    // representations).
                    summary.changed = summary.changed.saturating_add(1);
                }
                last_was_delete = false;
            }
        }
    }

    PageDiff {
        old_page,
        new_page,
        lines,
        summary,
    }
}

fn trim_trailing_newline(s: &str) -> String {
    let mut t = s.to_string();
    if t.ends_with('\n') {
        t.pop();
        if t.ends_with('\r') {
            t.pop();
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn identical_pdfs_yield_only_equal_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        make_n_page_pdf(&a, 3);
        make_n_page_pdf(&b, 3);

        let d = diff_pdfs(&a, &b).unwrap();
        assert_eq!(d.old_page_count, 3);
        assert_eq!(d.new_page_count, 3);
        assert_eq!(d.pages.len(), 3);
        assert_eq!(d.total.added, 0);
        assert_eq!(d.total.removed, 0);
        assert_eq!(d.total.changed, 0);
        for p in &d.pages {
            for l in &p.lines {
                assert_eq!(l.op, DiffOp::Equal);
            }
        }
    }

    #[test]
    fn extra_page_in_new_pdf_yields_unmatched_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        make_n_page_pdf(&a, 2);
        make_n_page_pdf(&b, 3);

        let d = diff_pdfs(&a, &b).unwrap();
        assert_eq!(d.old_page_count, 2);
        assert_eq!(d.new_page_count, 3);
        assert_eq!(d.pages.len(), 3);

        // Last page should be new-only.
        let last = d.pages.last().unwrap();
        assert_eq!(last.old_page, None);
        assert_eq!(last.new_page, Some(3));
        assert!(
            last.summary.added >= 1,
            "new-only page must contribute adds"
        );
        assert_eq!(last.summary.removed, 0);
    }

    #[test]
    fn missing_page_in_new_pdf_yields_old_only_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        make_n_page_pdf(&a, 3);
        make_n_page_pdf(&b, 2);

        let d = diff_pdfs(&a, &b).unwrap();
        assert_eq!(d.pages.len(), 3);

        let last = d.pages.last().unwrap();
        assert_eq!(last.old_page, Some(3));
        assert_eq!(last.new_page, None);
        assert!(last.summary.removed >= 1);
        assert_eq!(last.summary.added, 0);
    }

    #[test]
    fn missing_input_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        make_n_page_pdf(&a, 1);
        let missing = tmp.path().join("nope.pdf");

        let err = diff_pdfs(&a, &missing).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));

        let err2 = diff_pdfs(&missing, &a).unwrap_err();
        assert!(matches!(err2, PdfError::InputMissing(_)));
    }

    // Direct unit tests of `diff_page` for fine-grained behaviour. Going
    // through `diff_pdfs` would require us to mint bespoke PDFs with custom
    // multi-line text per page, which is overkill for what we're verifying.

    #[test]
    fn diff_page_single_line_change_marks_changed() {
        let p = diff_page(Some(1), Some(1), "hello world\n", "hello there\n");
        // Expect one Delete + one Insert + maybe an Equal at the end (none here).
        assert_eq!(p.summary.removed, 1);
        assert_eq!(p.summary.added, 1);
        // Consecutive delete+insert collapses to one `changed`.
        assert_eq!(p.summary.changed, 1);
    }

    #[test]
    fn diff_page_pure_insertion_does_not_bump_changed() {
        let p = diff_page(Some(1), Some(1), "a\nb\n", "a\nb\nc\n");
        assert_eq!(p.summary.added, 1);
        assert_eq!(p.summary.removed, 0);
        assert_eq!(p.summary.changed, 0);
    }

    #[test]
    fn diff_page_pure_deletion_does_not_bump_changed() {
        let p = diff_page(Some(1), Some(1), "a\nb\nc\n", "a\nb\n");
        assert_eq!(p.summary.added, 0);
        assert_eq!(p.summary.removed, 1);
        assert_eq!(p.summary.changed, 0);
    }

    #[test]
    fn diff_page_preserves_line_numbers() {
        let p = diff_page(Some(1), Some(1), "x\ny\nz\n", "x\nY\nz\n");
        let equal_first = p.lines.iter().find(|l| l.op == DiffOp::Equal).unwrap();
        assert_eq!(equal_first.old_line, Some(1));
        assert_eq!(equal_first.new_line, Some(1));

        let delete = p.lines.iter().find(|l| l.op == DiffOp::Delete).unwrap();
        assert_eq!(delete.old_line, Some(2));
        assert_eq!(delete.new_line, None);
        assert_eq!(delete.text, "y");

        let insert = p.lines.iter().find(|l| l.op == DiffOp::Insert).unwrap();
        assert_eq!(insert.old_line, None);
        assert_eq!(insert.new_line, Some(2));
        assert_eq!(insert.text, "Y");

        // Trailing equal line keeps its own incremented indices.
        let last_equal = p.lines.iter().rfind(|l| l.op == DiffOp::Equal).unwrap();
        assert_eq!(last_equal.old_line, Some(3));
        assert_eq!(last_equal.new_line, Some(3));
        assert_eq!(last_equal.text, "z");
    }

    #[test]
    fn diff_page_empty_inputs_no_panic() {
        let p = diff_page(Some(1), Some(1), "", "");
        assert_eq!(p.lines.len(), 0);
        assert_eq!(p.summary.added, 0);
        assert_eq!(p.summary.removed, 0);
    }

    #[test]
    fn diff_page_handles_crlf() {
        let p = diff_page(Some(1), Some(1), "a\r\nb\r\n", "a\r\nB\r\n");
        // Each line should be stripped of \r\n entirely.
        let delete = p.lines.iter().find(|l| l.op == DiffOp::Delete).unwrap();
        assert_eq!(delete.text, "b");
        let insert = p.lines.iter().find(|l| l.op == DiffOp::Insert).unwrap();
        assert_eq!(insert.text, "B");
    }
}
