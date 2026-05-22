// Reverse the page order of a PDF.
//
// Thin wrapper over `extract_pages_to` — we just hand it `[N, N-1, …, 1]`
// and let the existing split kernel produce a clean new doc with all
// page-level resources preserved.
//
// Why ship it as its own op when the user could already do this via
// `split-ranges`? Because typing "reverse" is 10x clearer than
// "ranges=10,9,8,…,1" for a 200-page deposition, and the surface
// shows up in the landing grid as its own card.

use crate::pdf::split::{extract_pages_to, page_count};
use crate::pdf::PdfError;
use std::path::Path;

pub fn reverse_pages(input: &Path, output: &Path) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let total = page_count(input)?;
    if total == 0 {
        return Err(PdfError::Other("input has no pages".into()));
    }
    let order: Vec<u32> = (1..=total).rev().collect();
    extract_pages_to(input, &order, output)?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn reverses_5_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("rev.pdf");
        make_n_page_pdf(&src, 5);
        let n = reverse_pages(&src, &dst).unwrap();
        assert_eq!(n, 5);
        assert_eq!(page_count(&dst).unwrap(), 5);
    }

    #[test]
    fn single_page_is_noop_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("rev.pdf");
        make_n_page_pdf(&src, 1);
        let n = reverse_pages(&src, &dst).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");
        let err = reverse_pages(&bogus, &dst).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
