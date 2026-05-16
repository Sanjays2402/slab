// Redact regions on chosen pages.
//
// True redaction in PDF is hard: you have to find content stream tokens that
// fall inside the rectangle and delete them, then black-bar the visible area.
// For v1 we ship the visible-black-bar half — opaque filled rectangles that
// fully cover the rectangle so the rendered output looks redacted in every
// viewer. (Future work: pair this with a text-extraction pass that strips
// matching glyphs from the underlying content stream.)
//
// We append a graphics-state stream that:
//   1. Sets fill color to black (or user-chosen)
//   2. Draws filled rectangles on top of existing content
//
// Each rectangle is given in 0..100 percentages of MediaBox so the UI doesn't
// have to think in points.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct RedactRect {
    pub page: u32, // 1-based
    pub left_pct: f32,
    pub bottom_pct: f32,
    pub right_pct: f32,
    pub top_pct: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedactOpts {
    pub rects: Vec<RedactRect>,
    /// 0.0 = black, 1.0 = white. Defaults to 0.
    pub gray: f32,
}

/// Returns the number of rectangles actually painted.
pub fn redact(input: &Path, output: &Path, opts: RedactOpts) -> Result<u32, PdfError> {
    if opts.rects.is_empty() {
        return Err(PdfError::Other("No redaction rectangles supplied.".into()));
    }
    let mut doc = Document::load(input)?;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();

    // Group rects by page number for efficiency.
    let mut applied = 0u32;
    let gray = opts.gray.clamp(0.0, 1.0);

    let mut updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();

    for rect in &opts.rects {
        let Some((_, page_id)) = page_ids.iter().find(|(n, _)| *n == rect.page) else {
            continue;
        };
        let Some((w, h)) = media_box(&doc, *page_id) else {
            continue;
        };
        let l = (w * rect.left_pct.clamp(0.0, 100.0) / 100.0).min(w);
        let b = (h * rect.bottom_pct.clamp(0.0, 100.0) / 100.0).min(h);
        let r = (w * rect.right_pct.clamp(0.0, 100.0) / 100.0).min(w);
        let t = (h * rect.top_pct.clamp(0.0, 100.0) / 100.0).min(h);
        let (x1, x2) = if l < r { (l, r) } else { (r, l) };
        let (y1, y2) = if b < t { (b, t) } else { (t, b) };
        if (x2 - x1) < 1.0 || (y2 - y1) < 1.0 {
            continue;
        }

        // q g rect-fill Q
        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new("rg", vec![gray.into(), gray.into(), gray.into()]),
            Operation::new(
                "re",
                vec![
                    Object::Real(x1),
                    Object::Real(y1),
                    Object::Real(x2 - x1),
                    Object::Real(y2 - y1),
                ],
            ),
            Operation::new("f", vec![]),
            Operation::new("Q", vec![]),
        ];
        let content = Content { operations: ops };
        let stream_id = doc.add_object(Stream::new(
            dictionary! {},
            content
                .encode()
                .map_err(|e| PdfError::Other(format!("Failed to encode redact stream: {e}")))?,
        ));
        updates.push((*page_id, stream_id));
        applied += 1;
    }

    for (page_id, stream_id) in updates {
        append_stream(&mut doc, page_id, stream_id)?;
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

fn append_stream(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    new_stream_id: lopdf::ObjectId,
) -> Result<(), PdfError> {
    let page = doc.get_object_mut(page_id)?;
    if let Object::Dictionary(dict) = page {
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn sample_pdf() -> Vec<u8> {
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
    fn redact_paints_rectangle() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();

        let n = redact(
            &input,
            &output,
            RedactOpts {
                rects: vec![RedactRect {
                    page: 1,
                    left_pct: 10.0,
                    bottom_pct: 10.0,
                    right_pct: 50.0,
                    top_pct: 30.0,
                }],
                gray: 0.0,
            },
        )
        .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn empty_rects_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        let r = redact(
            &input,
            &output,
            RedactOpts {
                rects: vec![],
                gray: 0.0,
            },
        );
        assert!(r.is_err());
    }
}
