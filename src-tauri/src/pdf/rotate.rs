// Rotate pages — adjust the /Rotate entry on chosen page dictionaries.
//
// PDF viewers render a page rotated clockwise by its /Rotate value, which
// must be a multiple of 90. We add the requested delta to any existing
// rotation (mod 360) instead of overwriting it, so rotating twice by 90° is
// the same as a single 180° rotation and pages that already carry a
// rotation keep behaving.

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use std::path::Path;

/// Rotate the chosen page numbers (1-based) clockwise by `degrees`.
/// `degrees` must be a multiple of 90 (negative values rotate
/// counter-clockwise); an empty `pages` list rotates every page.
/// Returns the number of pages modified.
pub fn rotate(input: &Path, output: &Path, degrees: i32, pages: &[u32]) -> Result<u32, PdfError> {
    if degrees % 90 != 0 {
        return Err(PdfError::Other(format!(
            "Rotation must be a multiple of 90°, got {degrees}°."
        )));
    }
    let delta = degrees.rem_euclid(360) as i64;

    let mut doc = Document::load(input)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    if total == 0 {
        return Err(PdfError::Other("The PDF has no pages.".into()));
    }
    if let Some(&beyond) = pages.iter().find(|&&n| n < 1 || n > total) {
        return Err(PdfError::Other(format!(
            "Page {beyond} is out of range — the document has {total} page(s)."
        )));
    }

    let mut applied = 0u32;
    for (n, page_id) in page_map {
        if !pages.is_empty() && !pages.contains(&n) {
            continue;
        }
        let page = doc.get_object_mut(page_id)?;
        if let Object::Dictionary(d) = page {
            let current = d
                .get(b"Rotate")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            let next = (current + delta).rem_euclid(360);
            if next == 0 {
                d.remove(b"Rotate");
            } else {
                d.set("Rotate", Object::Integer(next));
            }
            applied += 1;
        }
    }

    if applied == 0 {
        return Err(PdfError::Other("No pages were rotated.".into()));
    }

    doc.compress();
    doc.save(output)?;
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn sample_pdf() -> Vec<u8> {
        // A trivial 3-page PDF. Built once from lopdf.
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..3 {
            let contents = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Contents" => contents,
            });
            kids.push(Object::Reference(page_id));
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => 3,
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

    fn rotations_of(path: &Path) -> Vec<Option<i64>> {
        let doc = Document::load(path).unwrap();
        let mut ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
        ids.sort_by_key(|(n, _)| *n);
        ids.into_iter()
            .map(|(_, id)| {
                doc.get_object(id)
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .get(b"Rotate")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
            })
            .collect()
    }

    fn run(degrees: i32, pages: &[u32]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        rotate(&input, &output, degrees, pages).unwrap();
        (dir, output)
    }

    #[test]
    fn rotates_only_chosen_pages() {
        let (_dir, out) = run(90, &[2]);
        assert_eq!(rotations_of(&out), vec![None, Some(90), None]);
    }

    #[test]
    fn empty_pages_rotates_everything() {
        let (_dir, out) = run(180, &[]);
        assert_eq!(
            rotations_of(&out),
            vec![Some(180), Some(180), Some(180)]
        );
    }

    #[test]
    fn rotation_accumulates_and_wraps() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let mid = dir.path().join("mid.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();

        rotate(&input, &mid, 90, &[]).unwrap();
        rotate(&mid, &output, 270, &[]).unwrap();
        // 90 + 270 = 360 -> no /Rotate entry at all.
        assert_eq!(rotations_of(&output), vec![None, None, None]);

        rotate(&input, &mid, 270, &[1]).unwrap();
        rotate(&mid, &output, 180, &[1]).unwrap();
        // 270 + 180 = 450 -> 90.
        assert_eq!(rotations_of(&output)[0], Some(90));
    }

    #[test]
    fn negative_degrees_rotate_counter_clockwise() {
        let (_dir, out) = run(-90, &[]);
        assert_eq!(
            rotations_of(&out),
            vec![Some(270), Some(270), Some(270)]
        );
    }

    #[test]
    fn non_multiple_of_90_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        assert!(rotate(&input, &output, 45, &[]).is_err());
    }

    #[test]
    fn out_of_range_page_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        let err = rotate(&input, &output, 90, &[4]).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }
}
