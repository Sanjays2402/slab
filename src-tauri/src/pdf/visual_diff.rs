// Visual (pixel-level) PDF diff for the v2.4.0 "Stack" release.
//
// Builds on the line-level diff in `pdf::diff` and the Poppler `pdftoppm`
// raster pattern from `pdf::flatten`. Renders both PDFs at the same DPI,
// computes a per-pixel luminance-delta mask (with a 3x3 dilate to coalesce
// neighbouring edits), reduces the hot pixels to axis-aligned change boxes,
// base64-encodes the PNG rasters, and ships everything alongside the
// existing line diff in one DTO so the UI does a single round-trip.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::{ImageBuffer, ImageFormat, ImageReader, Luma, RgbaImage};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::pdf::PdfError;

/// Pixel-level change box, in raster coordinates (top-left origin, pixels at
/// the rendered DPI). Both sides share the same coordinate frame because
/// pages are rendered at the same DPI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Pixel area of the connected component before AABB-wrapping.
    /// Lets the UI sort/filter trivial single-pixel noise boxes.
    pub mass: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualPage {
    /// 1-based page index in the OLD doc (None = page only in new doc).
    pub old_page: Option<u32>,
    /// 1-based page index in the NEW doc (None = page only in old doc).
    pub new_page: Option<u32>,
    /// PNG of the old raster, base64-encoded. None if page is new-only.
    pub old_png_b64: Option<String>,
    /// PNG of the new raster, base64-encoded. None if page is old-only.
    pub new_png_b64: Option<String>,
    /// Rendered width / height at the chosen DPI.
    pub w: u32,
    pub h: u32,
    /// Pixel-coordinate change boxes. Empty = visually identical.
    pub changes: Vec<ChangeBox>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualDiff {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub dpi: u32,
    pub pages: Vec<VisualPage>,
    /// Re-exported line-level diff so the UI doesn't have to round-trip twice.
    pub line_diff: crate::pdf::diff::DocDiff,
}

type GrayImage = ImageBuffer<Luma<u8>, Vec<u8>>;

/// Per-pixel luminance delta. Both images must be the same size; if not,
/// the smaller dimensions win and the rest is ignored (treated as unchanged,
/// since the UI will draw any size mismatch separately).
///
/// `threshold` is the absolute luma-delta cutoff (0..=255). 20 is a good
/// default — survives anti-aliasing wiggles while catching a reflowed word.
pub fn mask_changes(a: &RgbaImage, b: &RgbaImage, threshold: u8) -> GrayImage {
    let w = a.width().min(b.width());
    let h = a.height().min(b.height());
    let mut out = ImageBuffer::from_pixel(w, h, Luma([0u8]));
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y).0;
            let pb = b.get_pixel(x, y).0;
            // Rec.709 luma.
            let la = 0.2126 * pa[0] as f32 + 0.7152 * pa[1] as f32 + 0.0722 * pa[2] as f32;
            let lb = 0.2126 * pb[0] as f32 + 0.7152 * pb[1] as f32 + 0.0722 * pb[2] as f32;
            if (la - lb).abs() as u32 >= threshold as u32 {
                out.put_pixel(x, y, Luma([255]));
            }
        }
    }
    dilate_3x3(&out)
}

/// 3x3 max-filter — fattens isolated change pixels into chunks so the
/// connected-component pass groups neighbouring edits into one box instead
/// of speckled confetti. Single pass, no extra allocation beyond the output.
fn dilate_3x3(src: &GrayImage) -> GrayImage {
    let (w, h) = (src.width(), src.height());
    let mut out = ImageBuffer::from_pixel(w, h, Luma([0u8]));
    for y in 0..h {
        for x in 0..w {
            let mut hot = 0u8;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    hot = hot.max(src.get_pixel(nx as u32, ny as u32).0[0]);
                }
            }
            out.put_pixel(x, y, Luma([hot]));
        }
    }
    out
}

/// Iterative flood-fill connected components on a binary mask, then produces
/// AABB-wrapped `ChangeBox`es. Drops boxes whose mass < `min_mass`.
pub fn aabb_components(mask: &GrayImage, min_mass: u32) -> Vec<ChangeBox> {
    let (w, h) = (mask.width(), mask.height());
    let mut visited = vec![false; (w * h) as usize];
    let mut boxes = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            let seed = (y0 * w + x0) as usize;
            if visited[seed] || mask.get_pixel(x0, y0).0[0] == 0 {
                continue;
            }
            let mut stack = vec![(x0, y0)];
            let (mut min_x, mut min_y, mut max_x, mut max_y, mut mass) = (x0, y0, x0, y0, 0u32);
            while let Some((x, y)) = stack.pop() {
                let i = (y * w + x) as usize;
                if visited[i] || mask.get_pixel(x, y).0[0] == 0 {
                    continue;
                }
                visited[i] = true;
                mass += 1;
                if x < min_x {
                    min_x = x;
                }
                if y < min_y {
                    min_y = y;
                }
                if x > max_x {
                    max_x = x;
                }
                if y > max_y {
                    max_y = y;
                }
                if x + 1 < w {
                    stack.push((x + 1, y));
                }
                if x > 0 {
                    stack.push((x - 1, y));
                }
                if y + 1 < h {
                    stack.push((x, y + 1));
                }
                if y > 0 {
                    stack.push((x, y - 1));
                }
            }
            if mass >= min_mass {
                boxes.push(ChangeBox {
                    x: min_x,
                    y: min_y,
                    w: max_x - min_x + 1,
                    h: max_y - min_y + 1,
                    mass,
                });
            }
        }
    }
    boxes
}

/// Rasterize every page of `pdf` to RGBA at `dpi` using Poppler `pdftoppm`.
/// Returns one `RgbaImage` per page in page order.
///
/// Requires `pdftoppm` on PATH. On absence, returns a descriptive error so
/// the Tauri command surface can translate it into a user-friendly message
/// (parity with `pdf::flatten`).
pub fn render_pdf_pages(pdf: &Path, dpi: u32) -> Result<Vec<RgbaImage>, PdfError> {
    let tmp = tempfile::tempdir().map_err(PdfError::Io)?;
    let prefix = tmp.path().join("page");
    let status = std::process::Command::new("pdftoppm")
        .arg("-r")
        .arg(dpi.to_string())
        .arg("-png")
        .arg(pdf)
        .arg(&prefix)
        .status()
        .map_err(|e| {
            PdfError::Other(format!(
                "pdftoppm not found ({e}). Install poppler: `brew install poppler` (macOS) / \
                 `apt install poppler-utils` (Linux) / vendored on Windows CI."
            ))
        })?;
    if !status.success() {
        return Err(PdfError::Other(format!("pdftoppm exited {status}")));
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(tmp.path())
        .map_err(PdfError::Io)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .collect();
    entries.sort();
    let mut out = Vec::with_capacity(entries.len());
    for p in entries {
        let img = ImageReader::open(&p)
            .map_err(|e| PdfError::Other(format!("open {}: {e}", p.display())))?
            .decode()
            .map_err(|e| PdfError::Other(format!("decode {}: {e}", p.display())))?
            .to_rgba8();
        out.push(img);
    }
    Ok(out)
}

fn rgba_to_png_b64(img: &RgbaImage) -> Result<String, PdfError> {
    let mut buf: Vec<u8> = Vec::with_capacity((img.width() * img.height()) as usize);
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| PdfError::Other(format!("png encode: {e}")))?;
    Ok(B64.encode(&buf))
}

/// Full visual diff. 1:1 page alignment by index for v1.
///
/// - `dpi`: render DPI (36..=300 clamped at command boundary; 150 is the UI default).
/// - `luma_threshold`: 0..=255 (20 default) — pixel-delta cutoff.
/// - `min_mass`: smallest connected-component size to keep (8 default) —
///   drops 1-2-pixel anti-alias jitter.
pub fn visual_diff_pdfs(
    old: &Path,
    new: &Path,
    dpi: u32,
    luma_threshold: u8,
    min_mass: u32,
) -> Result<VisualDiff, PdfError> {
    if !old.exists() {
        return Err(PdfError::InputMissing(old.display().to_string()));
    }
    if !new.exists() {
        return Err(PdfError::InputMissing(new.display().to_string()));
    }
    let old_pages = render_pdf_pages(old, dpi)?;
    let new_pages = render_pdf_pages(new, dpi)?;
    let n = old_pages.len().max(new_pages.len());
    let mut pages = Vec::with_capacity(n);
    for i in 0..n {
        let op = old_pages.get(i);
        let np = new_pages.get(i);
        let (w, h, changes, op_b64, np_b64) = match (op, np) {
            (Some(a), Some(b)) => {
                let mask = mask_changes(a, b, luma_threshold);
                let boxes = aabb_components(&mask, min_mass);
                (
                    a.width().min(b.width()),
                    a.height().min(b.height()),
                    boxes,
                    Some(rgba_to_png_b64(a)?),
                    Some(rgba_to_png_b64(b)?),
                )
            }
            (Some(a), None) => (
                a.width(),
                a.height(),
                vec![ChangeBox {
                    x: 0,
                    y: 0,
                    w: a.width(),
                    h: a.height(),
                    mass: a.width() * a.height(),
                }],
                Some(rgba_to_png_b64(a)?),
                None,
            ),
            (None, Some(b)) => (
                b.width(),
                b.height(),
                vec![ChangeBox {
                    x: 0,
                    y: 0,
                    w: b.width(),
                    h: b.height(),
                    mass: b.width() * b.height(),
                }],
                None,
                Some(rgba_to_png_b64(b)?),
            ),
            (None, None) => unreachable!("at least one side must have page i"),
        };
        pages.push(VisualPage {
            old_page: op.map(|_| (i + 1) as u32),
            new_page: np.map(|_| (i + 1) as u32),
            old_png_b64: op_b64,
            new_png_b64: np_b64,
            w,
            h,
            changes,
        });
    }
    let line_diff = crate::pdf::diff::diff_pdfs(old, new)?;
    Ok(VisualDiff {
        old_path: old.to_path_buf(),
        new_path: new.to_path_buf(),
        dpi,
        pages,
        line_diff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        ImageBuffer::from_pixel(w, h, Rgba(rgba))
    }

    fn pdftoppm_available() -> bool {
        std::process::Command::new("pdftoppm")
            .arg("-v")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn mask_of_identical_images_is_all_zero() {
        let a = solid(32, 32, [255, 255, 255, 255]);
        let b = solid(32, 32, [255, 255, 255, 255]);
        let m = mask_changes(&a, &b, 20);
        assert!(m.pixels().all(|p| p.0[0] == 0));
    }

    #[test]
    fn mask_of_one_changed_pixel_threshold_pass() {
        let a = solid(8, 8, [255, 255, 255, 255]);
        let mut b = a.clone();
        b.put_pixel(3, 4, Rgba([0, 0, 0, 255])); // black on white, max luma delta
        let m = mask_changes(&a, &b, 20);
        // After 3x3 dilate the hot pixel and its neighbours are all 255.
        assert_eq!(m.get_pixel(3, 4).0[0], 255, "changed pixel must be hot");
        assert_eq!(m.get_pixel(0, 0).0[0], 0, "far pixel must be cold");
    }

    #[test]
    fn mask_below_threshold_is_cold() {
        let a = solid(8, 8, [200, 200, 200, 255]);
        let mut b = a.clone();
        b.put_pixel(3, 4, Rgba([195, 195, 195, 255])); // luma delta = 5
        let m = mask_changes(&a, &b, 20);
        assert!(m.pixels().all(|p| p.0[0] == 0));
    }

    #[test]
    fn aabb_components_wraps_one_blob() {
        let mut mask: GrayImage = ImageBuffer::from_pixel(16, 16, Luma([0u8]));
        // Paint a 3x4 hot rectangle starting at (5, 6).
        for y in 6..10 {
            for x in 5..8 {
                mask.put_pixel(x, y, Luma([255]));
            }
        }
        let boxes = aabb_components(&mask, 1);
        assert_eq!(boxes.len(), 1);
        let b = &boxes[0];
        assert_eq!((b.x, b.y, b.w, b.h), (5, 6, 3, 4));
        assert_eq!(b.mass, 12);
    }

    #[test]
    fn aabb_components_drops_below_min_mass() {
        let mut mask: GrayImage = ImageBuffer::from_pixel(16, 16, Luma([0u8]));
        mask.put_pixel(2, 2, Luma([255])); // single pixel, mass=1
        let boxes = aabb_components(&mask, 5);
        assert!(boxes.is_empty(), "min_mass=5 should drop a mass-1 blob");
    }

    #[test]
    fn aabb_components_separates_two_blobs() {
        let mut mask: GrayImage = ImageBuffer::from_pixel(32, 16, Luma([0u8]));
        // Left blob at (2,2)-(4,4), right blob at (20,2)-(22,4) — well separated.
        for y in 2..5 {
            for x in 2..5 {
                mask.put_pixel(x, y, Luma([255]));
            }
            for x in 20..23 {
                mask.put_pixel(x, y, Luma([255]));
            }
        }
        let boxes = aabb_components(&mask, 1);
        assert_eq!(boxes.len(), 2);
    }

    #[test]
    fn dto_round_trips_through_serde() {
        let vd = VisualDiff {
            old_path: PathBuf::from("/tmp/a.pdf"),
            new_path: PathBuf::from("/tmp/b.pdf"),
            dpi: 150,
            pages: vec![VisualPage {
                old_page: Some(1),
                new_page: Some(1),
                old_png_b64: Some("aGVsbG8=".into()),
                new_png_b64: Some("d29ybGQ=".into()),
                w: 612,
                h: 792,
                changes: vec![ChangeBox {
                    x: 10,
                    y: 20,
                    w: 30,
                    h: 40,
                    mass: 600,
                }],
            }],
            line_diff: crate::pdf::diff::DocDiff {
                old_path: PathBuf::from("/tmp/a.pdf"),
                new_path: PathBuf::from("/tmp/b.pdf"),
                old_page_count: 1,
                new_page_count: 1,
                pages: vec![],
                total: crate::pdf::diff::DiffSummary::default(),
            },
        };
        let json = serde_json::to_string(&vd).unwrap();
        let back: VisualDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dpi, 150);
        assert_eq!(back.pages.len(), 1);
        assert_eq!(back.pages[0].changes[0].mass, 600);
    }

    #[test]
    fn visual_diff_pdfs_end_to_end_on_two_minimal_pdfs() {
        if !pdftoppm_available() {
            eprintln!("skipping: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("old.pdf");
        let new_ = tmp.path().join("new.pdf");
        crate::pdf::test_fixtures::make_n_page_pdf(&old, 1);
        crate::pdf::test_fixtures::make_n_page_pdf(&new_, 2);
        let vd = visual_diff_pdfs(&old, &new_, 72, 20, 8).expect("visual diff");
        assert_eq!(vd.dpi, 72);
        assert_eq!(vd.pages.len(), 2);
        assert!(vd.pages[0].old_png_b64.is_some());
        assert!(vd.pages[0].new_png_b64.is_some());
        // Page 2 is new-only — old_png_b64 is None.
        assert!(vd.pages[1].old_png_b64.is_none());
        assert!(vd.pages[1].new_png_b64.is_some());
    }
}
