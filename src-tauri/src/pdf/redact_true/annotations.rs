//! Annotation scrubbing for true redaction.
//!
//! After text-stream excision, the recipient could still recover redacted
//! content via:
//!   * Link / highlight / text-comment annotations whose `/Rect` falls inside
//!     a redact area (the annotation's `/Contents` string is plaintext).
//!   * Widget annotations (form fields) whose value carries the secret.
//!   * FreeText / Stamp annotations that draw text inside the redact box.
//!
//! This module walks `/Annots` on each page and removes any annotation whose
//! bounding rectangle intersects one of the supplied redact rects (in user-
//! space points).

use crate::pdf::PdfError;
use lopdf::{Document, Object, ObjectId};

/// Walk the page's `/Annots` array and drop any annotation whose `/Rect`
/// intersects one of `rects_pts`. Returns the number of annotations removed.
pub fn scrub_annotations_on_page(
    doc: &mut Document,
    page_id: ObjectId,
    rects_pts: &[(f32, f32, f32, f32)],
) -> Result<u32, PdfError> {
    if rects_pts.is_empty() {
        return Ok(0);
    }

    // Pull the annot id list (may be inline array or reference to array).
    let annot_ids: Vec<ObjectId> = {
        let Ok(Object::Dictionary(dict)) = doc.get_object(page_id) else {
            return Ok(0);
        };
        let arr = match dict.get(b"Annots") {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(r)) => {
                if let Ok(Object::Array(a)) = doc.get_object(*r) {
                    a.clone()
                } else {
                    return Ok(0);
                }
            }
            _ => return Ok(0),
        };
        arr.into_iter()
            .filter_map(|o| {
                if let Object::Reference(r) = o {
                    Some(r)
                } else {
                    None
                }
            })
            .collect()
    };

    let mut keep: Vec<Object> = Vec::with_capacity(annot_ids.len());
    let mut removed = 0u32;

    for aid in &annot_ids {
        let rect_opt = annot_rect(doc, *aid);
        let intersects = match rect_opt {
            Some((l, b, r, t)) => rects_pts
                .iter()
                .any(|(rl, rb, rr, rt)| !(r < *rl || l > *rr || t < *rb || b > *rt)),
            None => false,
        };
        if intersects {
            removed += 1;
        } else {
            keep.push(Object::Reference(*aid));
        }
    }

    if removed > 0 {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
            if keep.is_empty() {
                dict.remove(b"Annots");
            } else {
                dict.set("Annots", keep);
            }
        }
    }
    Ok(removed)
}

fn annot_rect(doc: &Document, annot_id: ObjectId) -> Option<(f32, f32, f32, f32)> {
    let dict = doc.get_object(annot_id).ok()?.as_dict().ok()?;
    let arr = dict.get(b"Rect").ok()?.as_array().ok()?;
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
    let l = nums[0].min(nums[2]);
    let r = nums[0].max(nums[2]);
    let b = nums[1].min(nums[3]);
    let t = nums[1].max(nums[3]);
    Some((l, b, r, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn page_with_annots(rects: &[(f32, f32, f32, f32, &str)]) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let contents = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));

        let mut annot_refs: Vec<Object> = Vec::new();
        for (l, b, r, t, text) in rects {
            let aid = doc.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Text",
                "Rect" => vec![
                    Object::Real(*l), Object::Real(*b),
                    Object::Real(*r), Object::Real(*t),
                ],
                "Contents" => Object::string_literal(*text),
            });
            annot_refs.push(Object::Reference(aid));
        }

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => contents,
            "Annots" => annot_refs,
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

    fn annot_count(doc: &Document, page_id: ObjectId) -> usize {
        let dict = doc.get_object(page_id).unwrap().as_dict().unwrap();
        match dict.get(b"Annots") {
            Ok(Object::Array(a)) => a.len(),
            _ => 0,
        }
    }

    #[test]
    fn removes_annot_inside_rect() {
        let (mut doc, pid) = page_with_annots(&[
            (100.0, 100.0, 200.0, 130.0, "SECRET-NOTE"),
            (300.0, 500.0, 400.0, 530.0, "KEEP-NOTE"),
        ]);
        assert_eq!(annot_count(&doc, pid), 2);
        let n = scrub_annotations_on_page(&mut doc, pid, &[(90.0, 90.0, 250.0, 150.0)]).unwrap();
        assert_eq!(n, 1);
        assert_eq!(annot_count(&doc, pid), 1);
    }

    #[test]
    fn keeps_annots_outside_rect() {
        let (mut doc, pid) = page_with_annots(&[(100.0, 100.0, 200.0, 130.0, "x")]);
        let n = scrub_annotations_on_page(&mut doc, pid, &[(400.0, 400.0, 500.0, 500.0)]).unwrap();
        assert_eq!(n, 0);
        assert_eq!(annot_count(&doc, pid), 1);
    }

    #[test]
    fn removes_all_clears_annots_key() {
        let (mut doc, pid) = page_with_annots(&[(0.0, 0.0, 595.0, 842.0, "x")]);
        let n = scrub_annotations_on_page(&mut doc, pid, &[(0.0, 0.0, 595.0, 842.0)]).unwrap();
        assert_eq!(n, 1);
        let dict = doc.get_object(pid).unwrap().as_dict().unwrap();
        assert!(dict.get(b"Annots").is_err());
    }
}
