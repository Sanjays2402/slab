//! Polyglot input bridge.
//!
//! Converts non-PDF input formats (.docx, .xlsx, .pptx, .html, .epub,
//! images, audio, …) into PDFs by shelling out to Microsoft's
//! `markitdown` CLI to produce Markdown, then handing the Markdown to
//! `crate::pdf::md2pdf::render`.
//!
//! See `.cron-state/research-markitdown.md` for the design rationale.

use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PolyglotOpts {
    /// Forwarded to md2pdf — "A4" | "Letter" | "Legal". Defaults to A4.
    #[serde(default)]
    pub page_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyglotReport {
    /// Lower-case extension of the source file (e.g. `"docx"`).
    pub source_kind: String,
    /// Pages in the produced PDF.
    pub pages: u32,
    /// Bytes of Markdown extracted by markitdown (informational).
    pub markdown_bytes: u32,
}

pub fn polyglot_to_pdf(
    _input: &Path,
    _output: &Path,
    _opts: PolyglotOpts,
) -> Result<PolyglotReport, PdfError> {
    Err(PdfError::Other("polyglot: not yet implemented".into()))
}

/// Return a canonical short kind for accepted source files.
///
/// PDF is deliberately rejected — round-tripping PDF→MD→PDF would
/// silently degrade the document.
pub fn supported_extension(input: &Path) -> Option<&'static str> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "docx" => Some("docx"),
        "pptx" => Some("pptx"),
        "xlsx" => Some("xlsx"),
        "xls" => Some("xls"),
        "html" | "htm" => Some("html"),
        "epub" => Some("epub"),
        "csv" => Some("csv"),
        "json" => Some("json"),
        "xml" => Some("xml"),
        "rtf" => Some("rtf"),
        "odt" => Some("odt"),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "webp" => Some("image"),
        "wav" | "mp3" | "m4a" | "flac" | "ogg" => Some("audio"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn supported_kinds() {
        for (path, want) in [
            ("note.docx", Some("docx")),
            ("DECK.PPTX", Some("pptx")),
            ("data.xlsx", Some("xlsx")),
            ("page.html", Some("html")),
            ("page.htm", Some("html")),
            ("book.epub", Some("epub")),
            ("table.csv", Some("csv")),
            ("payload.json", Some("json")),
            ("feed.xml", Some("xml")),
            ("note.rtf", Some("rtf")),
            ("doc.odt", Some("odt")),
            ("scan.png", Some("image")),
            ("photo.JPG", Some("image")),
            ("clip.wav", Some("audio")),
            ("clip.mp3", Some("audio")),
        ] {
            assert_eq!(supported_extension(&PathBuf::from(path)), want, "{path}");
        }
    }

    #[test]
    fn pdf_input_is_rejected() {
        assert_eq!(supported_extension(&PathBuf::from("doc.pdf")), None);
    }

    #[test]
    fn unknown_extension_is_rejected() {
        assert_eq!(supported_extension(&PathBuf::from("file.xyz")), None);
        assert_eq!(supported_extension(&PathBuf::from("no_extension")), None);
    }
}
