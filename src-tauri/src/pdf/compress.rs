// Compress a PDF.
//
// lopdf's `compress()` re-encodes uncompressed streams with Flate, which
// produces measurable savings on PDFs exported from "save without compression"
// pipelines. We round-trip the document and call compress() before save.

use crate::pdf::PdfError;
use lopdf::Document;
use std::path::Path;

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CompressReport {
    pub original_bytes: u64,
    pub new_bytes: u64,
    pub ratio: f32,
}

pub fn compress(input: &Path, output: &Path) -> Result<CompressReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let original_bytes = std::fs::metadata(input)?.len();
    let mut doc = Document::load(input)?;
    doc.compress();
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.save(output)?;
    let new_bytes = std::fs::metadata(output)?.len();
    let ratio = if original_bytes > 0 {
        new_bytes as f32 / original_bytes as f32
    } else {
        1.0
    };
    Ok(CompressReport {
        original_bytes,
        new_bytes,
        ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn compress_produces_valid_pdf() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);
        let report = compress(&src, &dst).unwrap();
        assert!(report.new_bytes > 0);
        // round-trip
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
    }
}
