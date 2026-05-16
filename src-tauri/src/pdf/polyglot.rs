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
