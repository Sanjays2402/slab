// Crop pages — set the CropBox (and optionally MediaBox) on chosen pages.
//
// We don't re-rasterize anything. PDFs use a CropBox dict entry that says
// "only show this rectangle when rendering". Setting it on a page is one
// dictionary write, so this is the cheapest of the new ops.
//
// Coordinates are in PDF user space (points, origin bottom-left). The frontend
// passes percentages (0..100) of the page's current MediaBox so the user can
// crop without knowing point math; we resolve to absolute coords here.

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct CropOpts {
    /// Left edge as a 0..100 percentage of MediaBox width.
    pub left_pct: f32,
    /// Bottom edge as a 0..100 percentage of MediaBox height.
    pub bottom_pct: f32,
    /// Right edge as a 0..100 percentage of MediaBox width.
    pub right_pct: f32,
    /// Top edge as a 0..100 percentage of MediaBox height.
    pub top_pct: f32,
    /// If true, also shrink MediaBox so the crop "sticks" through every viewer.
    pub also_resize_media: bool,
}

/// Apply crop to the chosen page numbers (1-based). Empty list = every page.
/// Returns the count of pages that were actually modified.
pub fn crop(input: &Path, output: &Path, opts: CropOpts, pages: &[u32]) -> Result<u32, PdfError> {
    let mut doc = Document::load(input)?;
    let page_ids: Vec<_> = doc.get_pages().into_iter().collect();
    let total = page_ids.len() as u32;

    let wanted: Vec<lopdf::ObjectId> = page_ids
        .into_iter()
        .filter_map(|(n, id)| {
            if pages.is_empty() || pages.contains(&n) {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    let mut applied = 0u32;
    for page_id in wanted {
        let Some((w, h)) = media_box(&doc, page_id) else {
            continue;
        };
        let l = w * (opts.left_pct.clamp(0.0, 100.0) / 100.0);
        let b = h * (opts.bottom_pct.clamp(0.0, 100.0) / 100.0);
        let r = w * (opts.right_pct.clamp(0.0, 100.0) / 100.0);
        let t = h * (opts.top_pct.clamp(0.0, 100.0) / 100.0);

        // Validate ordering — frontend may swap if user drags backwards.
        let (x1, x2) = if l < r { (l, r) } else { (r, l) };
        let (y1, y2) = if b < t { (b, t) } else { (t, b) };
        if (x2 - x1) < 1.0 || (y2 - y1) < 1.0 {
            continue;
        }

        let rect = Object::Array(vec![
            Object::Real(x1),
            Object::Real(y1),
            Object::Real(x2),
            Object::Real(y2),
        ]);

        let page = doc.get_object_mut(page_id)?;
        if let Object::Dictionary(d) = page {
            d.set("CropBox", rect.clone());
            if opts.also_resize_media {
                d.set("MediaBox", rect);
            }
            applied += 1;
        }
    }

    if applied == 0 && total > 0 {
        return Err(PdfError::Other(
            "No pages were cropped — check the rectangle.".into(),
        ));
    }

    doc.compress();
    doc.save(output)?;
    Ok(applied)
}

fn media_box(doc: &Document, page_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let dict = doc.get_object(page_id).ok()?.as_dict().ok()?;
    let mb = dict.get(b"MediaBox").ok()?.as_array().ok()?;
    if mb.len() < 4 {
        return None;
    }
    let nums: Vec<f32> = mb
        .iter()
        .filter_map(|o| match o {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        })
        .collect();
    if nums.len() < 4 {
        return None;
    }
    Some(((nums[2] - nums[0]).abs(), (nums[3] - nums[1]).abs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn sample_pdf() -> Vec<u8> {
        // A trivial 1-page A4 PDF (595x842 points). Built once from lopdf.
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let contents = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => contents,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn cropping_shrinks_cropbox() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();

        let n = crop(
            &input,
            &output,
            CropOpts {
                left_pct: 10.0,
                bottom_pct: 10.0,
                right_pct: 90.0,
                top_pct: 90.0,
                also_resize_media: false,
            },
            &[],
        )
        .unwrap();
        assert_eq!(n, 1);

        let doc = Document::load(&output).unwrap();
        let page_id = *doc.get_pages().values().next().unwrap();
        let dict = doc.get_object(page_id).unwrap().as_dict().unwrap();
        let cb = dict.get(b"CropBox").unwrap().as_array().unwrap();
        assert_eq!(cb.len(), 4);
    }

    #[test]
    fn invalid_zero_rect_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        let result = crop(
            &input,
            &output,
            CropOpts {
                left_pct: 50.0,
                bottom_pct: 50.0,
                right_pct: 50.0,
                top_pct: 50.0,
                also_resize_media: false,
            },
            &[],
        );
        assert!(result.is_err());
    }
}
