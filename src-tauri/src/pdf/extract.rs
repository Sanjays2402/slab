// Extract page text from a PDF.
//
// Uses lopdf's built-in text extraction. The result is a Vec<String>, one
// entry per page, in document order.

use crate::pdf::PdfError;
use lopdf::Document;
use std::path::Path;

pub fn extract_text(input: &Path) -> Result<Vec<String>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    let total = doc.get_pages().len() as u32;
    let mut out = Vec::with_capacity(total as usize);
    for page in 1..=total {
        let text = doc.extract_text(&[page]).unwrap_or_default();
        out.push(text);
    }
    Ok(out)
}

/// Extract text and concatenate to a single string with form-feed separators
/// between pages. Useful for "save as .txt".
pub fn extract_text_concat(input: &Path) -> Result<String, PdfError> {
    let pages = extract_text(input)?;
    Ok(pages.join("\n\x0c\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn extract_text_returns_one_string_per_page() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("src.pdf");
        make_n_page_pdf(&p, 3);
        let pages = extract_text(&p).unwrap();
        assert_eq!(pages.len(), 3);
        // Each fixture page contains "Slab page <N>".
        for (i, page_text) in pages.iter().enumerate() {
            assert!(
                page_text.contains(&format!("page {}", i + 1)),
                "page {} text {:?} missing label",
                i + 1,
                page_text
            );
        }
    }

    #[test]
    fn extract_text_concat_joins_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("src.pdf");
        make_n_page_pdf(&p, 2);
        let all = extract_text_concat(&p).unwrap();
        assert!(all.contains("page 1"));
        assert!(all.contains("page 2"));
        assert!(all.contains('\x0c'));
    }
}
