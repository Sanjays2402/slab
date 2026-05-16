//! OCR pipeline: rasterize PDF pages, run Tesseract, stitch the searchable
//! pages back into one PDF with an invisible text layer.
//!
//! ## Strategy
//!
//! There are two viable approaches here:
//!
//! 1. **Add an invisible text layer on top of existing pages.** This
//!    preserves vector content (fonts, paths) and only adds searchability
//!    where there was none. It's the right thing for PDFs that mix scanned
//!    images with vector graphics.
//!
//! 2. **Rasterize → OCR → stitch.** Loses vector content but is robust,
//!    universal, and what almost every "OCR a PDF" tool actually does for
//!    scanned input.
//!
//! For v0.8 we ship #2 because:
//!   * The primary use case is scanned PDFs (which are pure raster anyway).
//!   * Stitching an invisible text layer back onto arbitrary page content
//!     in `lopdf` requires laying down a transparent text-mode group with
//!     the exact OCR bounding boxes — that's a multi-week build, not a
//!     feature drop.
//!   * Tesseract's `pdf` config already produces a self-contained
//!     searchable PDF (raster on a Type-3 invisible-text overlay) — we just
//!     ask it to do all the pages in one shot.
//!
//! ## External binaries
//!
//! * `pdftoppm` (poppler) — page rasterization.
//! * `tesseract` (>= 4.x) — OCR + PDF output.
//!
//! Both are widely available on macOS via Homebrew and on Linux via apt.
//! We check for their presence up front and return a structured error if
//! they aren't installed.

use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// User-tunable OCR knobs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OcrOpts {
    /// Tesseract language code (e.g. `"eng"`, `"deu"`, `"eng+fra"`).
    /// Defaults to `"eng"` when missing.
    #[serde(default = "default_lang")]
    pub lang: String,
    /// Rasterization DPI. 300 is the sweet spot for printed text — lower
    /// loses accuracy, higher just bloats the output for diminishing
    /// returns. Defaults to 300.
    #[serde(default = "default_dpi")]
    pub dpi: u32,
}

fn default_lang() -> String {
    "eng".into()
}

fn default_dpi() -> u32 {
    300
}

impl Default for OcrOpts {
    fn default() -> Self {
        Self {
            lang: default_lang(),
            dpi: default_dpi(),
        }
    }
}

/// Result of running OCR over a PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrReport {
    /// Number of pages successfully OCR'd.
    pub pages: u32,
    /// Detected language (echo of what we asked Tesseract for).
    pub lang: String,
    /// Rasterization DPI used.
    pub dpi: u32,
}

/// Render `input` to images, OCR them, and write the searchable PDF to
/// `output`. The output is a NEW PDF; we don't modify `input` in place.
///
/// Returns the number of pages OCR'd, plus the settings used.
pub fn ocr(input: &Path, output: &Path, opts: &OcrOpts) -> Result<OcrReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }

    // Up-front tool check so the error message is friendly. (Otherwise the
    // user gets a cryptic ENOENT halfway through the pipeline.)
    require_binary("pdftoppm")?;
    require_binary("tesseract")?;

    let workdir =
        tempfile::tempdir().map_err(|e| PdfError::Other(format!("create temp dir: {e}")))?;
    let tmp = workdir.path();

    // ----- 1. Rasterize input PDF to PNGs at the requested DPI. -----
    let raster_prefix = tmp.join("page");
    let status = Command::new("pdftoppm")
        .arg("-r")
        .arg(opts.dpi.to_string())
        .arg("-png")
        .arg(input)
        .arg(&raster_prefix)
        .status()
        .map_err(|e| PdfError::Other(format!("run pdftoppm: {e}")))?;
    if !status.success() {
        return Err(PdfError::Other(format!(
            "pdftoppm exited {}",
            status.code().unwrap_or(-1)
        )));
    }

    // pdftoppm produces `page-1.png`, `page-2.png`, ... — sometimes with
    // zero-padding for documents with > 9 pages. We sort by extracted page
    // number to keep the OCR output ordered.
    let mut pngs: Vec<PathBuf> = std::fs::read_dir(tmp)
        .map_err(|e| PdfError::Other(format!("read temp dir: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "png"))
        .collect();
    pngs.sort_by_key(|p| page_number_from_path(p));

    if pngs.is_empty() {
        return Err(PdfError::Other(
            "pdftoppm produced no PNGs — was the input a valid PDF?".into(),
        ));
    }

    // ----- 2. Write imagelist.txt for tesseract's multi-page input. -----
    let list_path = tmp.join("pages.txt");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&list_path)
            .map_err(|e| PdfError::Other(format!("write imagelist: {e}")))?;
        for p in &pngs {
            writeln!(f, "{}", p.display())
                .map_err(|e| PdfError::Other(format!("write imagelist: {e}")))?;
        }
    }

    // ----- 3. Run tesseract. The "pdf" config emits a searchable PDF. ----
    // We pass an output prefix without extension; tesseract appends `.pdf`.
    let out_prefix = tmp.join("ocr_out");
    let status = Command::new("tesseract")
        .arg(&list_path)
        .arg(&out_prefix)
        .arg("-l")
        .arg(&opts.lang)
        .arg("pdf")
        .status()
        .map_err(|e| PdfError::Other(format!("run tesseract: {e}")))?;
    if !status.success() {
        return Err(PdfError::Other(format!(
            "tesseract exited {}",
            status.code().unwrap_or(-1)
        )));
    }

    let produced = out_prefix.with_extension("pdf");
    if !produced.exists() {
        return Err(PdfError::Other(
            "tesseract did not produce the expected .pdf output".into(),
        ));
    }

    // Ensure the parent of `output` exists. Avoids surprises when the user
    // points us at a brand-new directory tree.
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PdfError::Other(format!("create output dir: {e}")))?;
    }

    // Move the temp output into place. Use copy + remove rather than
    // `rename` because the temp dir may be on a different filesystem.
    std::fs::copy(&produced, output)
        .map_err(|e| PdfError::Other(format!("copy OCR output: {e}")))?;

    Ok(OcrReport {
        pages: pngs.len() as u32,
        lang: opts.lang.clone(),
        dpi: opts.dpi,
    })
}

/// Extract the trailing integer from a pdftoppm-style filename
/// (`page-12.png` → 12). Falls back to `0` so unrelated files sort first.
fn page_number_from_path(p: &Path) -> u32 {
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let digits: String = stem
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().unwrap_or(0)
}

/// Return Ok if `name` is on `$PATH`, else a friendly PdfError.
fn require_binary(name: &str) -> Result<(), PdfError> {
    let probe = Command::new(name).arg("--version").output();
    match probe {
        Ok(_) => Ok(()),
        Err(e) => Err(PdfError::Other(format!(
            "{name} not found on PATH ({e}). On macOS: `brew install \
             {brew_target}`. On Debian/Ubuntu: `sudo apt install \
             {apt_target}`.",
            brew_target = match name {
                "pdftoppm" => "poppler",
                "tesseract" => "tesseract",
                other => other,
            },
            apt_target = match name {
                "pdftoppm" => "poppler-utils",
                "tesseract" => "tesseract-ocr",
                other => other,
            },
        ))),
    }
}

// ---- tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    use lopdf::Document;
    use lopdf::Object;
    use lopdf::Stream;
    use tempfile::tempdir;

    /// Build a one-page PDF with a single line of crisp black text — small
    /// enough for the tests to be quick. We use a built-in Helvetica font
    /// so Tesseract has clean shapes to read.
    fn build_text_pdf(out: &Path, text: &str) {
        let mut doc = Document::with_version("1.5");

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = format!(
            "BT /F1 36 Tf 72 720 Td ({}) Tj ET",
            text.replace('(', "\\(").replace(')', "\\)")
        );
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => resources_id,
            "Contents" => content_id,
        });
        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.save(out).unwrap();
    }

    /// Helper: is tesseract installed on this machine? CI macos-15 has
    /// tesseract via Homebrew; bare dev boxes may not. Tests skip with
    /// `eprintln!` rather than panic so we don't block PRs on missing
    /// system deps.
    fn tesseract_available() -> bool {
        Command::new("tesseract").arg("--version").output().is_ok()
            && Command::new("pdftoppm").arg("--version").output().is_ok()
    }

    #[test]
    fn missing_input_errors() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let err = ocr(&dir.path().join("nope.pdf"), &out, &OcrOpts::default()).unwrap_err();
        match err {
            PdfError::InputMissing(_) => {}
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn opts_default_lang_and_dpi() {
        let opts = OcrOpts::default();
        assert_eq!(opts.lang, "eng");
        assert_eq!(opts.dpi, 300);
    }

    #[test]
    fn page_number_parses_trailing_digits() {
        let p1 = PathBuf::from("/tmp/page-1.png");
        let p12 = PathBuf::from("/tmp/page-12.png");
        let p123 = PathBuf::from("/tmp/page-123.png");
        assert_eq!(page_number_from_path(&p1), 1);
        assert_eq!(page_number_from_path(&p12), 12);
        assert_eq!(page_number_from_path(&p123), 123);

        // Sort order keeps numeric ordering — not lexicographic.
        let mut v = vec![p123.clone(), p1.clone(), p12.clone()];
        v.sort_by_key(|p| page_number_from_path(p));
        assert_eq!(v, vec![p1, p12, p123]);
    }

    #[test]
    fn page_number_falls_back_to_zero() {
        let p = PathBuf::from("/tmp/no-digits.png");
        assert_eq!(page_number_from_path(&p), 0);
    }

    #[test]
    fn ocr_one_page_yields_searchable_text() {
        if !tesseract_available() {
            eprintln!("skip: tesseract or pdftoppm not on PATH");
            return;
        }
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.pdf");
        let out = dir.path().join("out.pdf");
        build_text_pdf(&src, "OCR HELLO WORLD");

        let report = ocr(
            &src,
            &out,
            &OcrOpts {
                lang: "eng".into(),
                // 200 DPI is enough for a single-line title; keeps the
                // test fast.
                dpi: 200,
            },
        )
        .unwrap();
        assert_eq!(report.pages, 1);
        assert_eq!(report.lang, "eng");
        assert_eq!(report.dpi, 200);
        assert!(out.exists());

        // Pull the text content out of the output PDF: tesseract embeds
        // the OCR string as actual page text, so `extract_text` should
        // return something close to what we drew.
        let doc = Document::load(&out).unwrap();
        let pages: Vec<_> = doc.get_pages().keys().copied().collect();
        let text = doc.extract_text(&pages).unwrap_or_default();
        let normalized = text.to_lowercase().replace(['\n', '\r'], " ");
        assert!(
            normalized.contains("ocr")
                || normalized.contains("hello")
                || normalized.contains("world"),
            "expected OCR text to surface, got: {text:?}"
        );
    }
}
