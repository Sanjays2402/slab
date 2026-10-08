// PDF → images: one PNG/JPEG per page via Poppler `pdftoppm`.
//
// Same external dependency as `flatten` and `ocr`, so there is nothing new
// to install. Output files are named `<stem>-<page>.<ext>` with the page
// number zero-padded to the width of the page count so they sort naturally.

use crate::pdf::split::page_count;
use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MIN_DPI: u32 = 36;
pub const MAX_DPI: u32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn parse(s: &str) -> Result<Self, PdfError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            other => Err(PdfError::Other(format!(
                "unsupported image format {other:?} (use png or jpeg)"
            ))),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    fn flag(self) -> &'static str {
        match self {
            Self::Png => "-png",
            Self::Jpeg => "-jpeg",
        }
    }
}

/// Run `pdftoppm` with `args`, mapping a missing binary to a helpful error.
pub(crate) fn run_pdftoppm(args: &[std::ffi::OsString]) -> Result<(), PdfError> {
    let status = std::process::Command::new("pdftoppm")
        .args(args)
        .status()
        .map_err(|e| {
            PdfError::Other(format!(
                "pdftoppm not found ({e}). Install poppler: \
                 `brew install poppler` (macOS) / `apt install poppler-utils` (Linux)."
            ))
        })?;
    if !status.success() {
        return Err(PdfError::Other(format!(
            "pdftoppm exited {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

/// Render `pages` (1-based; empty = every page) of `input` into `out_dir`.
/// Returns the written files in page order.
pub fn pdf_to_images(
    input: &Path,
    out_dir: &Path,
    dpi: u32,
    format: ImageFormat,
    pages: &[u32],
) -> Result<Vec<PathBuf>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if !(MIN_DPI..=MAX_DPI).contains(&dpi) {
        return Err(PdfError::Other(format!(
            "dpi must be between {MIN_DPI} and {MAX_DPI}"
        )));
    }
    let total = page_count(input)?;
    if total == 0 {
        return Err(PdfError::Other("input has no pages".into()));
    }
    let mut wanted: Vec<u32> = if pages.is_empty() {
        (1..=total).collect()
    } else {
        pages.to_vec()
    };
    wanted.sort_unstable();
    wanted.dedup();
    if let Some(bad) = wanted.iter().find(|p| **p < 1 || **p > total) {
        return Err(PdfError::Other(format!(
            "page {bad} is out of range (document has {total})"
        )));
    }
    std::fs::create_dir_all(out_dir)?;

    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());
    let width = total.to_string().len();
    let mut written = Vec::with_capacity(wanted.len());
    for p in wanted {
        let prefix = out_dir.join(format!("{stem}-{p:0width$}"));
        let args: Vec<std::ffi::OsString> = vec![
            "-r".into(),
            dpi.to_string().into(),
            format.flag().into(),
            "-f".into(),
            p.to_string().into(),
            "-l".into(),
            p.to_string().into(),
            "-singlefile".into(),
            input.as_os_str().to_owned(),
            prefix.as_os_str().to_owned(),
        ];
        run_pdftoppm(&args)?;
        let file = prefix.with_file_name(format!(
            "{}.{}",
            prefix.file_name().unwrap().to_string_lossy(),
            format.extension()
        ));
        if !file.exists() {
            return Err(PdfError::Other(format!(
                "pdftoppm did not produce {}",
                file.display()
            )));
        }
        written.push(file);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    fn have_pdftoppm() -> bool {
        std::process::Command::new("pdftoppm")
            .arg("-v")
            .output()
            .is_ok()
    }

    #[test]
    fn parses_formats() {
        assert_eq!(ImageFormat::parse("PNG").unwrap(), ImageFormat::Png);
        assert_eq!(ImageFormat::parse("jpg").unwrap(), ImageFormat::Jpeg);
        assert!(ImageFormat::parse("gif").is_err());
    }

    #[test]
    fn rejects_bad_dpi_and_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.pdf");
        make_n_page_pdf(&src, 3);
        assert!(pdf_to_images(&src, tmp.path(), 10, ImageFormat::Png, &[]).is_err());
        assert!(pdf_to_images(&src, tmp.path(), 1000, ImageFormat::Png, &[]).is_err());
        assert!(pdf_to_images(&src, tmp.path(), 72, ImageFormat::Png, &[9]).is_err());
    }

    #[test]
    fn renders_selected_pages() {
        if !have_pdftoppm() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("doc.pdf");
        make_n_page_pdf(&src, 12);
        let out = tmp.path().join("img");
        let files = pdf_to_images(&src, &out, 50, ImageFormat::Png, &[3, 1]).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["doc-01.png", "doc-03.png"]);
        assert!(files.iter().all(|f| f.exists()));
    }
}
