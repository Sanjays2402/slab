// Clone a set of pages in place — useful for "duplicate slide", "make a 2-up
// handout from existing pages", or stamping a cover sheet on every chapter
// break.
//
// The kernel is a thin wrapper around `extract_pages_to`: we build the desired
// page order (each duplicated page appears twice, immediately after its
// original) and let the extract kernel do the heavy lifting.

use crate::pdf::split::{extract_pages_to, page_count};
use crate::pdf::PdfError;
use std::collections::BTreeSet;
use std::path::Path;

/// Duplicate the given pages, inserting copies immediately after each source
/// page. Pages are 1-indexed. Each entry in `pages` is duplicated once
/// (duplicates in the slice are deduped). Returns the new total page count.
///
/// Example: input has 3 pages, `pages = [2]` → output has 4 pages in order
/// `[1, 2, 2, 3]`.
pub fn duplicate_pages(input: &Path, pages: &[u32], output: &Path) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if pages.is_empty() {
        return Err(PdfError::Other("no pages specified for duplication".into()));
    }
    let total = page_count(input)?;
    let dup_set: BTreeSet<u32> = pages.iter().copied().collect();
    for &p in &dup_set {
        if p == 0 || p > total {
            return Err(PdfError::Other(format!(
                "page {p} out of range (1..={total})"
            )));
        }
    }
    let mut order: Vec<u32> = Vec::with_capacity(total as usize + dup_set.len());
    for p in 1..=total {
        order.push(p);
        if dup_set.contains(&p) {
            order.push(p);
        }
    }
    extract_pages_to(input, &order, output)?;
    Ok(order.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::split::page_count;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn duplicate_one_page_of_3() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let n = duplicate_pages(&src, &[2], &dst).unwrap();
        assert_eq!(n, 4);
        assert_eq!(page_count(&dst).unwrap(), 4);
    }

    #[test]
    fn duplicate_multiple_pages_at_once() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 5);

        let n = duplicate_pages(&src, &[1, 3, 5], &dst).unwrap();
        assert_eq!(n, 8);
        assert_eq!(page_count(&dst).unwrap(), 8);
    }

    #[test]
    fn duplicate_dedupes_repeated_input() {
        // Caller passes [2, 2, 2] — still duplicates page 2 exactly once.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let n = duplicate_pages(&src, &[2, 2, 2], &dst).unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn duplicate_rejects_out_of_range() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let err = duplicate_pages(&src, &[99], &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn duplicate_rejects_zero_index() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let err = duplicate_pages(&src, &[0], &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
    }

    #[test]
    fn duplicate_rejects_empty_slice() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let err = duplicate_pages(&src, &[], &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("no pages"));
    }

    #[test]
    fn duplicate_missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");
        let err = duplicate_pages(&bogus, &[1], &dst).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
