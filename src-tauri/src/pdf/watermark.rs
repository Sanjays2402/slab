// Text watermark across all (or selected) pages.
//
// Implementation: append a content stream to each target page that draws
// the watermark text centered, with low opacity, rotated 45°. We don't
// touch the existing content streams — the watermark is layered on top
// using q/Q to isolate graphics state.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct WatermarkOpts<'a> {
    pub text: &'a str,
    pub opacity: f32, // 0.0..=1.0
    pub font_size: f32,
    /// Rotation in degrees, CCW. -45.0 is a common diagonal watermark.
    pub rotation_deg: f32,
    /// Gray level 0.0 (black) ..= 1.0 (white).
    pub gray: f32,
}

impl<'a> Default for WatermarkOpts<'a> {
    fn default() -> Self {
        WatermarkOpts {
            text: "DRAFT",
            opacity: 0.25,
            font_size: 72.0,
            rotation_deg: 45.0,
            gray: 0.6,
        }
    }
}

pub fn watermark(
    input: &Path,
    output: &Path,
    opts: WatermarkOpts<'_>,
    pages: &[u32],
) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if opts.text.is_empty() {
        return Err(PdfError::Other("watermark text is empty".into()));
    }
    if !(0.0..=1.0).contains(&opts.opacity) {
        return Err(PdfError::Other("opacity must be in 0.0..=1.0".into()));
    }

    let mut doc = Document::load(input)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    let targets: BTreeSet<u32> = if pages.is_empty() {
        (1..=total).collect()
    } else {
        for &p in pages {
            if p == 0 || p > total {
                return Err(PdfError::Other(format!(
                    "page {} out of range (1..={})",
                    p, total
                )));
            }
        }
        pages.iter().copied().collect()
    };

    // Add a global font we can reference from every watermark stream.
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    // ExtGState dict for opacity.
    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => opts.opacity as f64,
        "CA" => opts.opacity as f64,
    });

    let mut applied = 0u32;
    let mut page_updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();

    for (page_num, page_id) in &page_map {
        if !targets.contains(page_num) {
            continue;
        }

        // Get page width/height from MediaBox.
        let (w, h) = page_size(&doc, *page_id).unwrap_or((612.0, 792.0));
        let cx = w / 2.0;
        let cy = h / 2.0;
        let theta = opts.rotation_deg.to_radians();
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        let ops = vec![
            Operation::new("q", vec![]),
            // Apply graphics state (opacity).
            Operation::new("gs", vec!["SlabGS".into()]),
            // Set gray fill color.
            Operation::new("g", vec![Object::Real(opts.gray)]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["SlabF".into(), opts.font_size.into()]),
            // Rotation matrix around (cx, cy)
            Operation::new(
                "Tm",
                vec![
                    Object::Real(cos_t),
                    Object::Real(sin_t),
                    Object::Real(-sin_t),
                    Object::Real(cos_t),
                    Object::Real(cx),
                    Object::Real(cy),
                ],
            ),
            // Rough left-shift so the text is centered (-len*size*0.3 ≈ half advance for Helvetica)
            Operation::new(
                "Td",
                vec![
                    Object::Real(-(opts.text.len() as f32) * opts.font_size * 0.25),
                    Object::Real(-opts.font_size * 0.3),
                ],
            ),
            Operation::new("Tj", vec![Object::string_literal(opts.text)]),
            Operation::new("ET", vec![]),
            Operation::new("Q", vec![]),
        ];
        let content = Content { operations: ops };
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        page_updates.push((*page_id, stream_id));
        applied += 1;
    }

    for (page_id, stream_id) in page_updates {
        append_content_stream(&mut doc, page_id, stream_id, font_id, gs_id)?;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(applied)
}

fn page_size(doc: &Document, page_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let page = doc.get_object(page_id).ok()?;
    let dict = page.as_dict().ok()?;
    let mb = dict.get(b"MediaBox").ok()?;
    let arr = mb.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let nums: Vec<f32> = arr
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

fn append_content_stream(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    new_stream_id: lopdf::ObjectId,
    font_id: lopdf::ObjectId,
    gs_id: lopdf::ObjectId,
) -> Result<(), PdfError> {
    let page = doc.get_object_mut(page_id)?;
    if let Object::Dictionary(dict) = page {
        // Append the new content stream.
        let new_contents = match dict.get(b"Contents") {
            Ok(Object::Reference(r)) => {
                vec![Object::Reference(*r), Object::Reference(new_stream_id)]
            }
            Ok(Object::Array(arr)) => {
                let mut v = arr.clone();
                v.push(Object::Reference(new_stream_id));
                v
            }
            _ => vec![Object::Reference(new_stream_id)],
        };
        dict.set("Contents", new_contents);

        // Make sure Resources has /Font /SlabF and /ExtGState /SlabGS.
        let resources_value = dict.get(b"Resources").ok().cloned();
        let resources_dict = match resources_value {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(r)) => {
                // Inline by following the ref.
                let obj = doc.get_object(r)?.as_dict()?;
                obj.clone()
            }
            _ => lopdf::Dictionary::new(),
        };
        let mut resources = resources_dict;

        let mut font = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        font.set("SlabF", Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(font));

        let mut gs = match resources.get(b"ExtGState") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        gs.set("SlabGS", Object::Reference(gs_id));
        resources.set("ExtGState", Object::Dictionary(gs));

        // Re-borrow mutably to set Resources back.
        let page = doc.get_object_mut(page_id)?;
        if let Object::Dictionary(dict) = page {
            dict.set("Resources", Object::Dictionary(resources));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn watermark_all_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);
        let opts = WatermarkOpts {
            text: "CONFIDENTIAL",
            ..Default::default()
        };
        let n = watermark(&src, &dst, opts, &[]).unwrap();
        assert_eq!(n, 3);
        // Output must still be a valid 3-page PDF.
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn watermark_rejects_empty_text() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let opts = WatermarkOpts {
            text: "",
            ..Default::default()
        };
        assert!(watermark(&src, &dst, opts, &[]).is_err());
    }

    #[test]
    fn watermark_rejects_bad_opacity() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let opts = WatermarkOpts {
            opacity: 2.0,
            ..Default::default()
        };
        assert!(watermark(&src, &dst, opts, &[]).is_err());
    }
}
