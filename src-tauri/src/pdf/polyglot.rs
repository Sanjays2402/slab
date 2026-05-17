//! Polyglot input bridge.
//!
//! Converts non-PDF input formats (.docx, .xlsx, .pptx, .html, .epub,
//! images, audio, …) into PDFs by shelling out to Microsoft's
//! `markitdown` CLI to produce Markdown, then handing the Markdown to
//! `crate::pdf::md2pdf::render`.
//!
//! See `.cron-state/research-markitdown.md` for the design rationale.

use crate::pdf::md2pdf::{render as md2pdf_render, Md2PdfOpts};
use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

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
    input: &Path,
    output: &Path,
    opts: PolyglotOpts,
) -> Result<PolyglotReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let kind = supported_extension(input).ok_or_else(|| {
        PdfError::Other(format!(
            "unsupported polyglot input: {} (try .docx, .xlsx, .pptx, \
             .html, .epub, .csv, .json, .xml, .rtf, .odt, image, or audio)",
            input.display()
        ))
    })?;
    require_markitdown()?;

    let out = Command::new("markitdown")
        .arg(input)
        .output()
        .map_err(|e| PdfError::Other(format!("run markitdown: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let trimmed = stderr.trim();
        return Err(PdfError::Other(format!(
            "markitdown failed ({}): {}",
            out.status.code().unwrap_or(-1),
            if trimmed.is_empty() {
                "(no stderr)"
            } else {
                trimmed
            }
        )));
    }
    let markdown = String::from_utf8(out.stdout)
        .map_err(|e| PdfError::Other(format!("markitdown produced non-UTF-8 output: {e}")))?;
    if markdown.trim().is_empty() {
        return Err(PdfError::Other(
            "markitdown returned empty document — input may be unreadable".into(),
        ));
    }
    let markdown_bytes = markdown.len() as u32;

    let page_size = if opts.page_size.is_empty() {
        "A4".to_string()
    } else {
        opts.page_size.clone()
    };
    let pages = md2pdf_render(
        &markdown,
        output,
        Md2PdfOpts {
            markdown: markdown.clone(),
            page_size,
        },
    )?;

    Ok(PolyglotReport {
        source_kind: kind.to_string(),
        pages,
        markdown_bytes,
    })
}

/// Verify the `markitdown` CLI is callable; otherwise return a friendly
/// error pointing at the recommended install command.
///
/// Mirrors `pdf::ocr::require_binary` — we probe `markitdown --help`
/// rather than a real conversion so the preflight stays fast and side-
/// effect free.
fn require_markitdown() -> Result<(), PdfError> {
    match Command::new("markitdown").arg("--help").output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(PdfError::Other(format!(
            "markitdown exited {} during preflight",
            out.status.code().unwrap_or(-1)
        ))),
        Err(e) => Err(PdfError::Other(format!(
            "markitdown not found on PATH ({e}). Install with: \
             `pipx install 'markitdown[all]'` (recommended) or \
             `pip install 'markitdown[all]'`. See \
             https://github.com/microsoft/markitdown",
        ))),
    }
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

    /// Mirrors `pdf::ocr::tesseract_available` — lets us gate tests that
    /// require the real `markitdown` binary on `$PATH` without breaking
    /// dev machines that haven't installed it yet.
    fn markitdown_available() -> bool {
        Command::new("markitdown")
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

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

    /// When `markitdown` is absent the error must be actionable: it
    /// should name the binary, name `PATH`, and suggest the canonical
    /// install command. Without these hints the failure is a black box
    /// to the user.
    #[test]
    fn require_markitdown_missing_error_is_actionable() {
        if markitdown_available() {
            eprintln!("skip: markitdown is installed on this host");
            return;
        }
        let err = require_markitdown().expect_err("expected missing-binary error");
        let msg = format!("{err}");
        assert!(msg.contains("markitdown"), "no binary name: {msg}");
        assert!(msg.contains("PATH"), "no PATH hint: {msg}");
        assert!(msg.contains("pipx install"), "no install hint: {msg}");
    }

    /// When `markitdown` IS installed the preflight must succeed —
    /// otherwise the rest of the polyglot pipeline can never run.
    #[test]
    fn require_markitdown_ok_when_installed() {
        if !markitdown_available() {
            eprintln!("skip: markitdown not on PATH");
            return;
        }
        require_markitdown().expect("preflight should succeed when binary is installed");
    }

    /// Missing input file must surface a typed `InputMissing` error,
    /// not a generic `Other`. Front-end / CLI rely on this enum tag to
    /// produce a friendlier UX than a string-match.
    #[test]
    fn missing_input_errors() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let err = polyglot_to_pdf(&dir.path().join("nope.docx"), &out, PolyglotOpts::default())
            .unwrap_err();
        match err {
            PdfError::InputMissing(_) => {}
            other => panic!("wrong error: {other:?}"),
        }
    }

    /// Unknown extensions must be rejected *before* we even try to
    /// invoke markitdown — that keeps the test cheap (no binary needed)
    /// and gives the user a clearer error than "markitdown crashed".
    #[test]
    fn unsupported_extension_errors() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("file.xyz");
        std::fs::write(&bogus, b"hi").unwrap();
        let out = dir.path().join("out.pdf");
        let err = polyglot_to_pdf(&bogus, &out, PolyglotOpts::default()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unsupported polyglot input"), "got: {msg}");
    }
}
