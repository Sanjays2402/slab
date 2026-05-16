// Append annotations (/Annot dictionaries) to PDF pages.
//
// PDF spec §12.5: every page can carry an /Annots array whose entries are
// annotation dictionaries describing highlights, sticky notes, links, form
// fields, etc. We support the two types people actually want when reading:
//
//   * `Highlight` — a colored swipe over text. Carries a `QuadPoints` array
//      of 8-tuples in PDF user space describing each highlighted line's
//      corner coordinates.
//   * `Text` — a sticky-note icon at a point on the page. Carries `Contents`
//      (the note body), a `Rect` (icon location), and a title/author.
//
// Coordinate system: PDF user space has its origin at the bottom-left of the
// page, with Y increasing upward. The frontend sends coordinates in a
// top-left CSS-pixel space at scale 1.0, so we flip Y against the page
// height before writing.

use crate::pdf::PdfError;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One annotation to add to a PDF. All coordinates are in **PDF user space**
/// (origin at the bottom-left of the page, Y increasing upward, units in
/// PDF points = 1/72 inch). The frontend converts from CSS pixels via
/// `pdfjs`'s `PageViewport.convertToPdfPoint()` before sending.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Annotation {
    /// Multi-quad highlight. Each quad is 8 floats: TL, TR, BL, BR corners
    /// in PDF user space. The outer rect is computed from the bounding box.
    Highlight {
        /// 0-based page index this annotation is anchored to.
        page_index: u32,
        /// Each quad is 8 floats: TL, TR, BL, BR corners (PDF point space).
        quads: Vec<[f32; 8]>,
        /// Optional comment attached to the highlight.
        #[serde(default)]
        contents: String,
        /// Author / title. Defaults to "Slab" if empty.
        #[serde(default)]
        author: String,
        /// Optional RGB color, each component 0..1. Defaults to yellow.
        #[serde(default)]
        color: Option<[f32; 3]>,
    },
    /// Sticky-note icon. `xy` is the icon's anchor point in PDF user space;
    /// we expand it into a 20×20 rect with the anchor at the lower-left.
    Note {
        /// 0-based page index.
        page_index: u32,
        /// Anchor point in PDF user space.
        xy: [f32; 2],
        /// The note's body text.
        #[serde(default)]
        contents: String,
        /// Author / title. Defaults to "Slab" if empty.
        #[serde(default)]
        author: String,
        /// Optional RGB color. Defaults to yellow.
        #[serde(default)]
        color: Option<[f32; 3]>,
    },
}

impl Annotation {
    fn page_index(&self) -> u32 {
        match self {
            Annotation::Highlight { page_index, .. } => *page_index,
            Annotation::Note { page_index, .. } => *page_index,
        }
    }
}

/// Write the input PDF to `output` with `annotations` appended.
/// Returns the number of annotations actually written (may be less than
/// `annotations.len()` if some target invalid pages — those are silently
/// skipped, never error).
pub fn append(input: &Path, output: &Path, annotations: &[Annotation]) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;
    let count = append_doc(&mut doc, annotations)?;
    doc.save(output)?;
    Ok(count)
}

fn append_doc(doc: &mut Document, annotations: &[Annotation]) -> Result<u32, PdfError> {
    let pages = doc.get_pages(); // 1-based num → page object id
    let mut index_to_id: std::collections::HashMap<u32, ObjectId> = Default::default();
    for (num, id) in pages {
        index_to_id.insert(num.saturating_sub(1), id);
    }

    let mut written = 0;
    for annot in annotations {
        let page_id = match index_to_id.get(&annot.page_index()) {
            Some(id) => *id,
            None => continue, // skip bad refs
        };

        let annot_dict = build_annotation_dict(annot);
        let annot_id = doc.add_object(Object::Dictionary(annot_dict));

        // Re-fetch the page mutably to attach the new annotation.
        let page = match doc.get_object_mut(page_id) {
            Ok(Object::Dictionary(d)) => d,
            _ => continue,
        };
        let new_annots = match page.get(b"Annots").cloned() {
            Ok(Object::Array(mut arr)) => {
                arr.push(Object::Reference(annot_id));
                arr
            }
            Ok(Object::Reference(annots_ref)) => {
                // /Annots was indirect — fetch the array, push, write back.
                let existing = doc
                    .get_object(annots_ref)
                    .and_then(|o| o.as_array().cloned())
                    .unwrap_or_default();
                let mut next = existing;
                next.push(Object::Reference(annot_id));
                // Replace by overwriting the referenced object.
                if let Ok(obj) = doc.get_object_mut(annots_ref) {
                    *obj = Object::Array(next.clone());
                }
                next
            }
            _ => vec![Object::Reference(annot_id)],
        };
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
            d.set("Annots", Object::Array(new_annots));
        }
        written += 1;
    }

    Ok(written)
}

/// Best-effort lookup of a page's height (in PDF points). Unused now that
/// the frontend sends PDF-space coordinates directly, but kept private for
/// future use (e.g. clamping rect into the page bounds).
#[allow(dead_code)]
fn page_height(doc: &Document, page_id: ObjectId) -> f32 {
    // Walk the page → /Parent chain looking for a MediaBox.
    let mut current = Some(page_id);
    while let Some(id) = current {
        let dict = match doc.get_object(id) {
            Ok(Object::Dictionary(d)) => d,
            _ => break,
        };
        if let Ok(mb) = dict.get(b"MediaBox") {
            if let Some(h) = media_box_height(doc, mb) {
                return h;
            }
        }
        current = match dict.get(b"Parent") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        };
    }
    792.0
}

#[allow(dead_code)]
fn media_box_height(doc: &Document, obj: &Object) -> Option<f32> {
    let arr = match obj {
        Object::Array(a) => a.clone(),
        Object::Reference(r) => doc.get_object(*r).ok()?.as_array().ok()?.clone(),
        _ => return None,
    };
    if arr.len() != 4 {
        return None;
    }
    let y0 = num(&arr[1])?;
    let y1 = num(&arr[3])?;
    Some((y1 - y0).abs() as f32)
}

#[allow(dead_code)]
fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn build_annotation_dict(annot: &Annotation) -> Dictionary {
    match annot {
        Annotation::Highlight {
            quads,
            contents,
            author,
            color,
            ..
        } => {
            // PDF user space: pass quads through verbatim and compute Rect.
            let mut flat: Vec<f32> = Vec::with_capacity(quads.len() * 8);
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for q in quads {
                for i in 0..4 {
                    let x = q[i * 2];
                    let y = q[i * 2 + 1];
                    flat.push(x);
                    flat.push(y);
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
                }
            }
            if !flat.is_empty() {
                // Inflate Rect very slightly so readers don't clip the highlight.
                min_x -= 1.0;
                min_y -= 1.0;
                max_x += 1.0;
                max_y += 1.0;
            } else {
                min_x = 0.0;
                min_y = 0.0;
                max_x = 0.0;
                max_y = 0.0;
            }

            let rgb = color.unwrap_or([1.0, 0.95, 0.4]);
            let mut dict = dictionary! {
                "Type" => "Annot",
                "Subtype" => "Highlight",
                "Rect" => Object::Array(vec![
                    Object::Real(min_x), Object::Real(min_y),
                    Object::Real(max_x), Object::Real(max_y),
                ]),
                "QuadPoints" => Object::Array(flat.into_iter().map(Object::Real).collect()),
                "C" => Object::Array(rgb.iter().map(|c| Object::Real(*c)).collect()),
                "F" => Object::Integer(4), // print flag
                "CA" => Object::Real(0.5),
            };
            if !contents.is_empty() {
                dict.set(
                    "Contents",
                    Object::String(encode_pdf_text(contents), lopdf::StringFormat::Literal),
                );
            }
            let t = if author.is_empty() { "Slab" } else { author };
            dict.set(
                "T",
                Object::String(encode_pdf_text(t), lopdf::StringFormat::Literal),
            );
            dict.set(
                "M",
                Object::String(now_pdf_date().into_bytes(), lopdf::StringFormat::Literal),
            );
            dict
        }
        Annotation::Note {
            xy,
            contents,
            author,
            color,
            ..
        } => {
            let x = xy[0];
            let y = xy[1];
            // Icon is conventionally 20×20 PDF points; PDF readers paint
            // a fixed icon regardless of Rect size.
            let rect = [x, y, x + 20.0, y + 20.0];

            let rgb = color.unwrap_or([1.0, 0.95, 0.4]);
            let mut dict = dictionary! {
                "Type" => "Annot",
                "Subtype" => "Text",
                "Rect" => Object::Array(rect.iter().map(|c| Object::Real(*c)).collect()),
                "C" => Object::Array(rgb.iter().map(|c| Object::Real(*c)).collect()),
                "F" => Object::Integer(4),
                "Open" => Object::Boolean(false),
                "Name" => "Comment",
            };
            if !contents.is_empty() {
                dict.set(
                    "Contents",
                    Object::String(encode_pdf_text(contents), lopdf::StringFormat::Literal),
                );
            }
            let t = if author.is_empty() { "Slab" } else { author };
            dict.set(
                "T",
                Object::String(encode_pdf_text(t), lopdf::StringFormat::Literal),
            );
            dict.set(
                "M",
                Object::String(now_pdf_date().into_bytes(), lopdf::StringFormat::Literal),
            );
            dict
        }
    }
}

/// Encode a string for use as a PDF text string. ASCII goes through as-is;
/// anything else becomes UTF-16BE with a BOM (PDF spec §7.9.2.2).
fn encode_pdf_text(s: &str) -> Vec<u8> {
    if s.is_ascii() {
        s.as_bytes().to_vec()
    } else {
        let mut buf = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            buf.extend_from_slice(&u.to_be_bytes());
        }
        buf
    }
}

/// Format the current time as a PDF date string: `D:YYYYMMDDHHmmSSZ`.
fn now_pdf_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Plain UTC, no leap-second handling — good enough for an /M timestamp.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day, hour, minute, second) = epoch_to_ymdhms(secs);
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}Z",
        year, month, day, hour, minute, second
    )
}

fn epoch_to_ymdhms(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    // Days since 1970-01-01.
    let days = (secs / 86_400) as i64;
    let rem = (secs % 86_400) as u32;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;

    // Convert days to Gregorian YMD (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use lopdf::Document;

    #[test]
    fn missing_input_errors() {
        let r = append(Path::new("/no/such/file.pdf"), Path::new("/tmp/x"), &[]);
        assert!(matches!(r, Err(PdfError::InputMissing(_))));
    }

    #[test]
    fn append_zero_annotations_is_noop_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);
        let n = append(&src, &out, &[]).unwrap();
        assert_eq!(n, 0);
        let d = Document::load(&out).unwrap();
        assert_eq!(d.get_pages().len(), 2);
    }

    #[test]
    fn append_highlight_writes_annot_dict_to_page() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);
        let n = append(
            &src,
            &out,
            &[Annotation::Highlight {
                page_index: 0,
                quads: vec![[100.0, 100.0, 200.0, 100.0, 100.0, 120.0, 200.0, 120.0]],
                contents: "important".into(),
                author: "Sanjay".into(),
                color: None,
            }],
        )
        .unwrap();
        assert_eq!(n, 1);

        // Verify the annotation made it onto page 0.
        let d = Document::load(&out).unwrap();
        let pages = d.get_pages();
        let page_id = pages.get(&1).copied().expect("page 1");
        let page = d.get_object(page_id).unwrap().as_dict().unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 1);

        let annot_id = match &annots[0] {
            Object::Reference(r) => *r,
            _ => panic!("annot should be indirect"),
        };
        let annot = d.get_object(annot_id).unwrap().as_dict().unwrap();
        let subtype = annot.get(b"Subtype").unwrap();
        assert!(matches!(subtype, Object::Name(n) if n == b"Highlight"));
        // QuadPoints must be 8 floats.
        let quads = annot.get(b"QuadPoints").unwrap().as_array().unwrap();
        assert_eq!(quads.len(), 8);
    }

    #[test]
    fn append_note_writes_text_subtype() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let n = append(
            &src,
            &out,
            &[Annotation::Note {
                page_index: 0,
                xy: [40.0, 60.0],
                contents: "todo: rewrite".into(),
                author: "".into(),
                color: None,
            }],
        )
        .unwrap();
        assert_eq!(n, 1);

        let d = Document::load(&out).unwrap();
        let pages = d.get_pages();
        let page = d.get_object(pages[&1]).unwrap().as_dict().unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        let annot_id = match &annots[0] {
            Object::Reference(r) => *r,
            _ => panic!(),
        };
        let annot = d.get_object(annot_id).unwrap().as_dict().unwrap();
        let subtype = annot.get(b"Subtype").unwrap();
        assert!(matches!(subtype, Object::Name(n) if n == b"Text"));
        // Default author is "Slab" when caller leaves it empty.
        let t = annot.get(b"T").unwrap();
        if let Object::String(bytes, _) = t {
            assert_eq!(bytes, b"Slab");
        } else {
            panic!("/T must be a string");
        }
    }

    #[test]
    fn append_multiple_to_same_page() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let n = append(
            &src,
            &out,
            &[
                Annotation::Note {
                    page_index: 0,
                    xy: [10.0, 10.0],
                    contents: "a".into(),
                    author: "".into(),
                    color: None,
                },
                Annotation::Note {
                    page_index: 0,
                    xy: [30.0, 30.0],
                    contents: "b".into(),
                    author: "".into(),
                    color: None,
                },
                Annotation::Note {
                    page_index: 0,
                    xy: [50.0, 50.0],
                    contents: "c".into(),
                    author: "".into(),
                    color: None,
                },
            ],
        )
        .unwrap();
        assert_eq!(n, 3);
        let d = Document::load(&out).unwrap();
        let pages = d.get_pages();
        let annots = d
            .get_object(pages[&1])
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(annots.len(), 3);
    }

    #[test]
    fn bad_page_index_is_silently_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);
        let n = append(
            &src,
            &out,
            &[
                Annotation::Note {
                    page_index: 999,
                    xy: [1.0, 1.0],
                    contents: "ghost".into(),
                    author: "".into(),
                    color: None,
                },
                Annotation::Note {
                    page_index: 0,
                    xy: [1.0, 1.0],
                    contents: "real".into(),
                    author: "".into(),
                    color: None,
                },
            ],
        )
        .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn appending_preserves_existing_annotations() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let stage1 = tmp.path().join("stage1.pdf");
        let stage2 = tmp.path().join("stage2.pdf");
        make_n_page_pdf(&src, 1);
        append(
            &src,
            &stage1,
            &[Annotation::Note {
                page_index: 0,
                xy: [1.0, 1.0],
                contents: "first".into(),
                author: "".into(),
                color: None,
            }],
        )
        .unwrap();
        append(
            &stage1,
            &stage2,
            &[Annotation::Note {
                page_index: 0,
                xy: [2.0, 2.0],
                contents: "second".into(),
                author: "".into(),
                color: None,
            }],
        )
        .unwrap();
        let d = Document::load(&stage2).unwrap();
        let pages = d.get_pages();
        let annots = d
            .get_object(pages[&1])
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(annots.len(), 2);
    }

    #[test]
    fn unicode_contents_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let body = "中文 — note 📝";
        append(
            &src,
            &out,
            &[Annotation::Note {
                page_index: 0,
                xy: [10.0, 10.0],
                contents: body.into(),
                author: "Sanjay".into(),
                color: None,
            }],
        )
        .unwrap();
        let d = Document::load(&out).unwrap();
        let pages = d.get_pages();
        let annots = d
            .get_object(pages[&1])
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        let annot_id = match &annots[0] {
            Object::Reference(r) => *r,
            _ => panic!(),
        };
        let contents = d
            .get_object(annot_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Contents")
            .unwrap();
        if let Object::String(bytes, _) = contents {
            // Must begin with the UTF-16BE BOM.
            assert_eq!(&bytes[..2], &[0xFE, 0xFF]);
        } else {
            panic!("/Contents not a string");
        }
    }
}
