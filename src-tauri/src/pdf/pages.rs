// Single-PDF page operations: rotate, delete, reorder.
//
// All operations are non-destructive — the source file is untouched and the
// result is written to `output`. Pages are 1-indexed throughout.

use crate::pdf::split::extract_pages_to;
use crate::pdf::PdfError;
use lopdf::{Document, Object};
use std::collections::BTreeSet;
use std::path::Path;

/// Rotation in degrees. PDF spec only allows multiples of 90.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    Cw90,
    Cw180,
    Cw270,
}

impl Rotation {
    pub fn degrees(self) -> i64 {
        match self {
            Rotation::Cw90 => 90,
            Rotation::Cw180 => 180,
            Rotation::Cw270 => 270,
        }
    }

    pub fn from_int(d: i64) -> Result<Self, PdfError> {
        match d.rem_euclid(360) {
            90 => Ok(Rotation::Cw90),
            180 => Ok(Rotation::Cw180),
            270 => Ok(Rotation::Cw270),
            _ => Err(PdfError::Other(format!(
                "rotation must be 90, 180, or 270 (got {})",
                d
            ))),
        }
    }
}

/// Rotate the given pages by `rot`. `pages` is 1-indexed; passing an empty
/// slice rotates every page.
pub fn rotate_pages(
    input: &Path,
    pages: &[u32],
    rot: Rotation,
    output: &Path,
) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
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

    let mut rotated = 0u32;
    for (page_num, page_id) in page_map {
        if !targets.contains(&page_num) {
            continue;
        }
        let existing = read_rotate(&doc, page_id);
        let new_rot = (existing + rot.degrees()) % 360;
        write_rotate(&mut doc, page_id, new_rot)?;
        rotated += 1;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(rotated)
}

/// Delete the given pages. `pages` is 1-indexed; refuses to delete every page.
pub fn delete_pages(input: &Path, pages: &[u32], output: &Path) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if pages.is_empty() {
        return Err(PdfError::Other("no pages specified for deletion".into()));
    }
    let total = crate::pdf::split::page_count(input)?;
    let drop_set: BTreeSet<u32> = pages.iter().copied().collect();
    if drop_set.len() == total as usize {
        return Err(PdfError::Other("refusing to delete every page".into()));
    }
    let keep: Vec<u32> = (1..=total).filter(|p| !drop_set.contains(p)).collect();
    extract_pages_to(input, &keep, output)?;
    Ok(drop_set.len() as u32)
}

/// Reorder pages. `order` is a permutation containing each page (1-indexed)
/// exactly once.
pub fn reorder_pages(input: &Path, order: &[u32], output: &Path) -> Result<(), PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let total = crate::pdf::split::page_count(input)?;
    if order.len() != total as usize {
        return Err(PdfError::Other(format!(
            "reorder needs {} pages, got {}",
            total,
            order.len()
        )));
    }
    let mut seen = BTreeSet::new();
    for &p in order {
        if p == 0 || p > total {
            return Err(PdfError::Other(format!(
                "page {} out of range (1..={})",
                p, total
            )));
        }
        if !seen.insert(p) {
            return Err(PdfError::Other(format!("page {} listed twice", p)));
        }
    }
    extract_pages_to(input, order, output)
}

/// Bake rotation into page geometry — permanent rotation, not a viewer hint.
///
/// For each target page:
/// 1. Resolves the inherited `/MediaBox` (walking up `/Parent` chain if needed).
/// 2. Strips any `/Rotate` entry already present.
/// 3. Swaps the `/MediaBox` (and `/CropBox` if present) for 90°/270° rotations;
///    leaves the box unchanged for 180°.
/// 4. Prepends a `q <matrix> cm` to every content stream and appends ` Q` to
///    rotate the actual drawing operators in PDF page-space.
///
/// The result re-opens identically rotated in every PDF reader, with no
/// `/Rotate` hint present. Closes issue #26 acceptance criterion #3.
pub fn rotate_pages_permanent(
    input: &Path,
    pages: &[u32],
    rot: Rotation,
    output: &Path,
) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
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

    let mut count = 0u32;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = page_map.into_iter().collect();
    for (page_num, page_id) in page_ids {
        if !targets.contains(&page_num) {
            continue;
        }
        bake_rotation_on_page(&mut doc, page_id, rot)?;
        count += 1;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Serialize and atomically write.
    let mut buf: Vec<u8> = Vec::new();
    doc.save_to(&mut buf)?;
    crate::pdf::atomic_save(output, &buf)?;
    Ok(count)
}

fn bake_rotation_on_page(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    rot: Rotation,
) -> Result<(), PdfError> {
    // Read inherited MediaBox + any existing /Rotate hint on this page.
    let (w, h) = read_mediabox_inheriting(doc, page_id)?;
    let prior = read_rotate(doc, page_id);
    let effective = ((prior + rot.degrees()).rem_euclid(360)) as u32;

    // Compute new dimensions + the content-stream prefix matrix.
    // PDF default user-space: origin bottom-left, +x right, +y up, units = pts.
    // We pre-multiply (`q <m> cm`) so the existing drawing operators are
    // rotated about the origin and then translated back into the new viewport.
    let (nw, nh, matrix) = match effective {
        0 => (w, h, None),
        90 => (h, w, Some(format!("0 -1 1 0 0 {}", w))),
        180 => (w, h, Some(format!("-1 0 0 -1 {} {}", w, h))),
        270 => (h, w, Some(format!("0 1 -1 0 {} 0", h))),
        _ => unreachable!("rem_euclid(360) keeps quadrant aligned"),
    };

    let contents_obj = {
        let page = doc.get_object(page_id)?.as_dict()?;
        page.get(b"Contents").ok().cloned()
    };

    {
        let page = doc.get_object_mut(page_id)?.as_dict_mut()?;
        let new_box = vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(nw),
            Object::Real(nh),
        ];
        page.set("MediaBox", new_box.clone());
        if page.has(b"CropBox") {
            page.set("CropBox", new_box);
        }
        page.remove(b"Rotate");
    }

    if let (Some(c), Some(matrix)) = (contents_obj, matrix) {
        let stream_ids: Vec<lopdf::ObjectId> = match c {
            Object::Reference(id) => vec![id],
            Object::Array(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
            _ => Vec::new(),
        };
        for sid in stream_ids {
            let s = doc.get_object_mut(sid)?.as_stream_mut()?;
            // Decompress if needed so the prepended operators land in plain text.
            let _ = s.decompress();
            let mut new_content = format!("q {} cm\n", matrix).into_bytes();
            new_content.extend_from_slice(&s.content);
            new_content.extend_from_slice(b"\nQ");
            s.set_content(new_content);
        }
    }
    Ok(())
}

fn read_mediabox_inheriting(
    doc: &Document,
    page_id: lopdf::ObjectId,
) -> Result<(f32, f32), PdfError> {
    fn obj_f32(o: &Object) -> Option<f32> {
        match o {
            Object::Real(r) => Some(*r),
            Object::Integer(i) => Some(*i as f32),
            _ => None,
        }
    }
    let mut node_id = page_id;
    for _ in 0..16 {
        let dict = doc.get_object(node_id)?.as_dict()?;
        if let Ok(arr) = dict.get(b"MediaBox").and_then(|o| o.as_array()) {
            if arr.len() >= 4 {
                let llx = obj_f32(&arr[0]).unwrap_or(0.0);
                let lly = obj_f32(&arr[1]).unwrap_or(0.0);
                let urx = obj_f32(&arr[2]).unwrap_or(0.0);
                let ury = obj_f32(&arr[3]).unwrap_or(0.0);
                return Ok((urx - llx, ury - lly));
            }
        }
        match dict.get(b"Parent").and_then(|o| o.as_reference()) {
            Ok(p) => node_id = p,
            Err(_) => break,
        }
    }
    Err(PdfError::Other(
        "no /MediaBox found walking page-tree".into(),
    ))
}

fn read_rotate(doc: &Document, page_id: lopdf::ObjectId) -> i64 {
    if let Ok(page) = doc.get_object(page_id) {
        if let Ok(dict) = page.as_dict() {
            if let Ok(r) = dict.get(b"Rotate").and_then(|o| o.as_i64()) {
                return ((r % 360) + 360) % 360;
            }
        }
    }
    0
}

fn write_rotate(doc: &mut Document, page_id: lopdf::ObjectId, rot: i64) -> Result<(), PdfError> {
    let obj = doc.get_object_mut(page_id)?;
    if let Object::Dictionary(dict) = obj {
        if rot == 0 {
            dict.remove(b"Rotate");
        } else {
            dict.set("Rotate", rot);
        }
        Ok(())
    } else {
        Err(PdfError::Other("page object is not a dictionary".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::split::page_count;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn rotate_all_pages_by_90() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);
        let n = rotate_pages(&src, &[], Rotation::Cw90, &dst).unwrap();
        assert_eq!(n, 3);

        let doc = Document::load(&dst).unwrap();
        for (_, id) in doc.get_pages() {
            assert_eq!(read_rotate(&doc, id), 90);
        }
    }

    #[test]
    fn rotate_subset_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 4);
        rotate_pages(&src, &[2, 4], Rotation::Cw180, &dst).unwrap();
        let doc = Document::load(&dst).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        assert_eq!(read_rotate(&doc, pages[0].1), 0);
        assert_eq!(read_rotate(&doc, pages[1].1), 180);
        assert_eq!(read_rotate(&doc, pages[2].1), 0);
        assert_eq!(read_rotate(&doc, pages[3].1), 180);
    }

    #[test]
    fn delete_pages_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 5);
        let n = delete_pages(&src, &[2, 4], &dst).unwrap();
        assert_eq!(n, 2);
        assert_eq!(page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn delete_refuses_all() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);
        assert!(delete_pages(&src, &[1, 2], &dst).is_err());
    }

    #[test]
    fn reorder_reverses() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 4);
        reorder_pages(&src, &[4, 3, 2, 1], &dst).unwrap();
        assert_eq!(page_count(&dst).unwrap(), 4);
    }

    #[test]
    fn reorder_rejects_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);
        assert!(reorder_pages(&src, &[1, 1, 2], &dst).is_err());
    }

    #[test]
    fn rotation_from_int_normalizes() {
        assert_eq!(Rotation::from_int(450).unwrap(), Rotation::Cw90);
        assert_eq!(Rotation::from_int(-90).unwrap(), Rotation::Cw270);
        assert!(Rotation::from_int(45).is_err());
    }

    #[test]
    fn permanent_rotation_strips_rotate_entry_and_swaps_mediabox() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let n = rotate_pages_permanent(&src, &[1], Rotation::Cw90, &dst).unwrap();
        assert_eq!(n, 1);

        let doc = Document::load(&dst).unwrap();
        for (_pn, pid) in doc.get_pages() {
            let page = doc.get_object(pid).unwrap().as_dict().unwrap();
            assert!(
                page.get(b"Rotate").is_err(),
                "page {pid:?} still has /Rotate after permanent rotation"
            );
        }

        // Page 1 should now be landscape (792 x 612).
        let pages = doc.get_pages();
        let first = *pages.get(&1).unwrap();
        let p = doc.get_object(first).unwrap().as_dict().unwrap();
        let mb = p.get(b"MediaBox").unwrap().as_array().unwrap();
        let read = |o: &Object| -> f32 {
            match o {
                Object::Real(r) => *r,
                Object::Integer(i) => *i as f32,
                _ => 0.0,
            }
        };
        let w = read(&mb[2]) - read(&mb[0]);
        let h = read(&mb[3]) - read(&mb[1]);
        assert!(
            (w - 792.0).abs() < 0.5 && (h - 612.0).abs() < 0.5,
            "MediaBox not swapped after 90° rotation: {w}x{h}"
        );
    }

    #[test]
    fn permanent_rotation_180_keeps_mediabox() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);

        rotate_pages_permanent(&src, &[1, 2], Rotation::Cw180, &dst).unwrap();
        let doc = Document::load(&dst).unwrap();
        for (_pn, pid) in doc.get_pages() {
            let p = doc.get_object(pid).unwrap().as_dict().unwrap();
            let mb = p.get(b"MediaBox").unwrap().as_array().unwrap();
            let read = |o: &Object| -> f32 {
                match o {
                    Object::Real(r) => *r,
                    Object::Integer(i) => *i as f32,
                    _ => 0.0,
                }
            };
            let w = read(&mb[2]) - read(&mb[0]);
            let h = read(&mb[3]) - read(&mb[1]);
            assert!(
                (w - 612.0).abs() < 0.5 && (h - 792.0).abs() < 0.5,
                "MediaBox should be unchanged for 180°"
            );
        }
    }

    #[test]
    fn permanent_rotation_composes_with_existing_rotate_hint() {
        // Apply soft 90° then permanent 90° → effective 180°, MediaBox stays portrait.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let mid = tmp.path().join("mid.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        rotate_pages(&src, &[1], Rotation::Cw90, &mid).unwrap();
        rotate_pages_permanent(&mid, &[1], Rotation::Cw90, &dst).unwrap();

        let doc = Document::load(&dst).unwrap();
        let (_pn, pid) = doc.get_pages().into_iter().next().unwrap();
        let p = doc.get_object(pid).unwrap().as_dict().unwrap();
        assert!(p.get(b"Rotate").is_err());
        let mb = p.get(b"MediaBox").unwrap().as_array().unwrap();
        let read = |o: &Object| -> f32 {
            match o {
                Object::Real(r) => *r,
                Object::Integer(i) => *i as f32,
                _ => 0.0,
            }
        };
        let w = read(&mb[2]) - read(&mb[0]);
        let h = read(&mb[3]) - read(&mb[1]);
        // Effective 180° → stays 612x792.
        assert!(
            (w - 612.0).abs() < 0.5 && (h - 792.0).abs() < 0.5,
            "expected 612x792, got {w}x{h}"
        );
    }

    #[test]
    fn permanent_rotation_rejects_out_of_range() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);
        let err = rotate_pages_permanent(&src, &[5], Rotation::Cw90, &dst).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("out of range"), "got: {msg}");
    }
}
