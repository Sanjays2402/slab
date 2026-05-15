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
}
