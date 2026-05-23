//! Excise text show operators that fall inside redact rectangles.
//!
//! For each page, we decode the content stream(s), walk glyph bounding boxes
//! via `glyph_bbox::collect_text_boxes`, and rewrite the stream so any text
//! operator whose bbox is fully inside (or sufficiently overlapping) a
//! redact rect becomes a no-op. This is the load-bearing half of true
//! redaction — the rendered black bar painted afterwards by
//! `super::redact::redact` only covers what the eye sees; this pass removes
//! the recoverable text payload.

use crate::pdf::redact::RedactRect;
use crate::pdf::redact_true::glyph_bbox::collect_text_boxes;
use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId};

/// Rewrite the content stream(s) of `page_id` so that any text-show operator
/// whose glyph bbox intersects one of `rects_pts` (already in user-space
/// points for this page) is replaced by a no-op. Returns the count of glyph
/// runs excised.
pub fn excise_text_on_page(
    doc: &mut Document,
    page_id: ObjectId,
    rects_pts: &[(f32, f32, f32, f32)],
) -> Result<u32, PdfError> {
    if rects_pts.is_empty() {
        return Ok(0);
    }

    let stream_ids = page_content_stream_ids(doc, page_id);
    let mut total = 0u32;

    for sid in stream_ids {
        let raw = match doc.get_object(sid) {
            Ok(Object::Stream(s)) => s.clone(),
            _ => continue,
        };
        let decoded = match raw.decompressed_content() {
            Ok(b) => b,
            Err(_) => raw.content.clone(),
        };
        let content = match Content::decode(&decoded) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let boxes = collect_text_boxes(&content.operations, 12.0);
        if boxes.is_empty() {
            continue;
        }

        // Find op indices to excise.
        let mut excise: Vec<usize> = Vec::new();
        for b in &boxes {
            for (l, bo, r, t) in rects_pts {
                if b.intersects(*l, *bo, *r, *t) {
                    excise.push(b.op_index);
                    break;
                }
            }
        }
        if excise.is_empty() {
            continue;
        }

        let new_ops: Vec<Operation> = content
            .operations
            .iter()
            .enumerate()
            .map(|(i, op)| {
                if excise.contains(&i) {
                    // Replace with a harmless no-op that preserves any text-
                    // matrix advance: emit an empty string TJ array. This
                    // keeps surrounding ops valid (still inside BT/ET) but
                    // emits no glyphs and triggers no font glyph lookups.
                    Operation::new("TJ", vec![Object::Array(vec![])])
                } else {
                    op.clone()
                }
            })
            .collect();

        let new_content = Content {
            operations: new_ops,
        };
        let new_bytes = new_content.encode().map_err(|e| {
            PdfError::Other(format!("Failed to encode excised content stream: {e}"))
        })?;

        if let Ok(Object::Stream(s)) = doc.get_object_mut(sid) {
            s.set_content(new_bytes);
            // Drop any /Filter so the new bytes (uncompressed) round-trip cleanly.
            s.dict.remove(b"Filter");
            s.dict.remove(b"DecodeParms");
        }

        total += excise.len() as u32;
    }

    Ok(total)
}

/// Return the list of content-stream object IDs for a page (handles both the
/// `/Contents N R` and `/Contents [N R M R …]` forms).
fn page_content_stream_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let Ok(Object::Dictionary(dict)) = doc.get_object(page_id) else {
        return Vec::new();
    };
    match dict.get(b"Contents") {
        Ok(Object::Reference(r)) => vec![*r],
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|o| {
                if let Object::Reference(r) = o {
                    Some(*r)
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Convert a `RedactRect` (page-percentage space) to user-space points using
/// the page's MediaBox. Returns `(l,b,r,t)` with l<=r and b<=t.
pub fn rect_to_points(
    doc: &Document,
    page_id: ObjectId,
    rect: &RedactRect,
) -> Option<(f32, f32, f32, f32)> {
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
    let w = (nums[2] - nums[0]).abs();
    let h = (nums[3] - nums[1]).abs();
    let l = w * rect.left_pct.clamp(0.0, 100.0) / 100.0;
    let b = h * rect.bottom_pct.clamp(0.0, 100.0) / 100.0;
    let r = w * rect.right_pct.clamp(0.0, 100.0) / 100.0;
    let t = h * rect.top_pct.clamp(0.0, 100.0) / 100.0;
    let (x1, x2) = if l <= r { (l, r) } else { (r, l) };
    let (y1, y2) = if b <= t { (b, t) } else { (t, b) };
    Some((x1, y1, x2, y2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream, StringFormat};

    fn build_page_with_text(text: &str, tx: f32, ty: f32) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
                ),
                Operation::new("Td", vec![Object::Real(tx), Object::Real(ty)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        text.as_bytes().to_vec(),
                        StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => stream_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", cat);
        (doc, page_id)
    }

    fn page_text_payload(doc: &Document, page_id: ObjectId) -> Vec<u8> {
        let sids = page_content_stream_ids(doc, page_id);
        let mut out = Vec::new();
        for sid in sids {
            if let Ok(Object::Stream(s)) = doc.get_object(sid) {
                let bytes = s
                    .decompressed_content()
                    .unwrap_or_else(|_| s.content.clone());
                out.extend(bytes);
            }
        }
        out
    }

    #[test]
    fn excises_tj_when_inside_rect() {
        let (mut doc, page_id) = build_page_with_text("SECRET", 100.0, 700.0);
        // Rect that covers the text origin and ~70pt of glyph advance.
        let rects = vec![(90.0, 690.0, 200.0, 720.0)];
        let n = excise_text_on_page(&mut doc, page_id, &rects).unwrap();
        assert_eq!(n, 1, "expected one glyph run excised");
        let body = page_text_payload(&doc, page_id);
        // The literal bytes "SECRET" must no longer appear in the stream.
        assert!(
            !body.windows(6).any(|w| w == b"SECRET"),
            "redacted text still present in stream"
        );
    }

    #[test]
    fn leaves_text_outside_rect_alone() {
        let (mut doc, page_id) = build_page_with_text("KEEP", 400.0, 100.0);
        let rects = vec![(0.0, 700.0, 100.0, 800.0)]; // top-left corner, nowhere near our text
        let n = excise_text_on_page(&mut doc, page_id, &rects).unwrap();
        assert_eq!(n, 0);
        let body = page_text_payload(&doc, page_id);
        assert!(body.windows(4).any(|w| w == b"KEEP"));
    }

    #[test]
    fn rect_to_points_uses_media_box() {
        let (doc, page_id) = build_page_with_text("X", 0.0, 0.0);
        let rect = RedactRect {
            page: 1,
            left_pct: 0.0,
            bottom_pct: 0.0,
            right_pct: 50.0,
            top_pct: 50.0,
        };
        let (l, b, r, t) = rect_to_points(&doc, page_id, &rect).unwrap();
        assert!((l - 0.0).abs() < 0.1);
        assert!((b - 0.0).abs() < 0.1);
        assert!((r - 595.0 * 0.5).abs() < 0.1);
        assert!((t - 842.0 * 0.5).abs() < 0.1);
    }

    #[test]
    fn rect_to_points_normalizes_inverted_corners() {
        let (doc, page_id) = build_page_with_text("X", 0.0, 0.0);
        let rect = RedactRect {
            page: 1,
            left_pct: 80.0,
            bottom_pct: 90.0,
            right_pct: 10.0,
            top_pct: 5.0,
        };
        let (l, b, r, t) = rect_to_points(&doc, page_id, &rect).unwrap();
        assert!(l < r);
        assert!(b < t);
    }

    #[test]
    fn excise_returns_zero_for_no_rects() {
        let (mut doc, page_id) = build_page_with_text("ANYTHING", 0.0, 0.0);
        assert_eq!(excise_text_on_page(&mut doc, page_id, &[]).unwrap(), 0);
    }
}
