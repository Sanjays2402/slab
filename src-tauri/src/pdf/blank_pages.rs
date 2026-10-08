// Detect and remove blank pages.
//
// Each page is rasterized at a low DPI (greyscale) with Poppler `pdftoppm`
// and its "ink ratio" — the share of non-white pixels, ignoring a thin edge
// margin so scanner shadows and punch-hole borders don't count — is compared
// to a threshold. Rendering rather than inspecting content streams means a
// page holding only white-on-white text or an empty image still counts as
// blank, and a scan with real content never does.

use crate::pdf::pages::delete_pages;
use crate::pdf::rasterize::run_pdftoppm;
use crate::pdf::split::page_count;
use crate::pdf::PdfError;
use image::GrayImage;
use serde::Serialize;
use std::path::Path;

/// Default threshold: a page with ≤ 0.05% non-white pixels is blank.
pub const DEFAULT_THRESHOLD: f64 = 0.0005;
const SCAN_DPI: u32 = 50;
/// Pixels darker than this (0–255) count as ink.
const INK_LEVEL: u8 = 200;
/// Fraction of each side ignored when measuring.
const EDGE_MARGIN: f64 = 0.03;

#[derive(Debug, Clone, Serialize)]
pub struct PageInk {
    pub page: u32,
    pub ink: f64,
    pub blank: bool,
}

/// Share of pixels darker than `INK_LEVEL` inside the margin-trimmed area.
pub fn ink_ratio(img: &GrayImage) -> f64 {
    let (w, h) = (img.width(), img.height());
    let mx = (w as f64 * EDGE_MARGIN) as u32;
    let my = (h as f64 * EDGE_MARGIN) as u32;
    if w <= 2 * mx || h <= 2 * my {
        return 0.0;
    }
    let mut dark = 0u64;
    for y in my..h - my {
        for x in mx..w - mx {
            if img.get_pixel(x, y).0[0] < INK_LEVEL {
                dark += 1;
            }
        }
    }
    dark as f64 / ((w - 2 * mx) as u64 * (h - 2 * my) as u64) as f64
}

fn check_threshold(threshold: f64) -> Result<(), PdfError> {
    if !(0.0..=0.5).contains(&threshold) {
        return Err(PdfError::Other(
            "threshold must be between 0 and 0.5 (fraction of inked pixels)".into(),
        ));
    }
    Ok(())
}

/// Measure every page. `blank` is `ink <= threshold`.
pub fn scan_blank_pages(input: &Path, threshold: f64) -> Result<Vec<PageInk>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    check_threshold(threshold)?;
    let total = page_count(input)?;
    if total == 0 {
        return Err(PdfError::Other("input has no pages".into()));
    }
    let tmp = tempfile::tempdir()?;
    let prefix = tmp.path().join("p");
    run_pdftoppm(&[
        "-r".into(),
        SCAN_DPI.to_string().into(),
        "-gray".into(),
        "-png".into(),
        input.as_os_str().to_owned(),
        prefix.as_os_str().to_owned(),
    ])?;
    let mut pngs: Vec<_> = std::fs::read_dir(tmp.path())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    // pdftoppm zero-pads page numbers to a common width, so name order == page order.
    pngs.sort();
    if pngs.len() != total as usize {
        return Err(PdfError::Other(format!(
            "pdftoppm produced {} images for {} pages",
            pngs.len(),
            total
        )));
    }
    pngs.iter()
        .enumerate()
        .map(|(i, p)| {
            let img = image::open(p)
                .map_err(|e| PdfError::Other(format!("decode {}: {e}", p.display())))?
                .to_luma8();
            let ink = ink_ratio(&img);
            Ok(PageInk {
                page: i as u32 + 1,
                ink,
                blank: ink <= threshold,
            })
        })
        .collect()
}

/// Write a copy of `input` without its blank pages to `output`. Returns the
/// removed page numbers (1-based). If none are blank, nothing is written.
pub fn remove_blank_pages(
    input: &Path,
    output: &Path,
    threshold: f64,
) -> Result<Vec<u32>, PdfError> {
    let blanks: Vec<u32> = scan_blank_pages(input, threshold)?
        .into_iter()
        .filter(|p| p.blank)
        .map(|p| p.page)
        .collect();
    if blanks.is_empty() {
        return Ok(blanks);
    }
    delete_pages(input, &blanks, output)?;
    Ok(blanks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn white_page_has_no_ink() {
        let img = GrayImage::from_pixel(200, 300, Luma([255]));
        assert_eq!(ink_ratio(&img), 0.0);
    }

    #[test]
    fn edge_shadow_is_ignored() {
        let mut img = GrayImage::from_pixel(200, 300, Luma([255]));
        for y in 0..300 {
            img.put_pixel(0, y, Luma([0]));
            img.put_pixel(199, y, Luma([0]));
        }
        assert_eq!(ink_ratio(&img), 0.0);
    }

    #[test]
    fn content_is_measured() {
        let mut img = GrayImage::from_pixel(200, 300, Luma([255]));
        for y in 100..110 {
            for x in 50..150 {
                img.put_pixel(x, y, Luma([0]));
            }
        }
        let r = ink_ratio(&img);
        assert!(r > 0.01, "ratio {r}");
        assert!(r > DEFAULT_THRESHOLD);
    }

    #[test]
    fn rejects_bad_threshold() {
        assert!(check_threshold(-0.1).is_err());
        assert!(check_threshold(0.9).is_err());
        assert!(check_threshold(0.001).is_ok());
    }
}
