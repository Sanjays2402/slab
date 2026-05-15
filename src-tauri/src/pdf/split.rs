// Split a PDF.
//
// Two modes:
//   - Ranges: explicit page lists (e.g. [(1,3), (5,5), (7,10)]) -> N output files.
//   - Every: split into chunks of N pages each -> ceil(total/N) output files.
//
// Pages are 1-indexed in the public API to match every PDF UX in existence.

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use std::path::{Path, PathBuf};

/// A single page range, 1-indexed inclusive on both ends.
#[derive(Debug, Clone, Copy)]
pub struct PageRange {
    pub start: u32,
    pub end: u32,
}

impl PageRange {
    pub fn new(start: u32, end: u32) -> Result<Self, PdfError> {
        if start == 0 || end == 0 {
            return Err(PdfError::Other("page numbers are 1-indexed (got 0)".into()));
        }
        if start > end {
            return Err(PdfError::Other(format!(
                "invalid range: start ({}) > end ({})",
                start, end
            )));
        }
        Ok(PageRange { start, end })
    }
}

/// Split `input` by explicit ranges. Each range becomes its own PDF saved
/// at `out_dir / "<stem>-<idx>-<start>-<end>.pdf"`.
pub fn split_by_ranges(
    input: &Path,
    ranges: &[PageRange],
    out_dir: &Path,
) -> Result<Vec<PathBuf>, PdfError> {
    validate_input(input)?;
    if ranges.is_empty() {
        return Err(PdfError::Other("no ranges provided".into()));
    }
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir)?;
    }

    let total = page_count(input)?;
    for r in ranges {
        if r.end > total {
            return Err(PdfError::Other(format!(
                "range end {} exceeds total pages {}",
                r.end, total
            )));
        }
    }

    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "split".to_string());

    let mut outputs = Vec::with_capacity(ranges.len());
    for (idx, r) in ranges.iter().enumerate() {
        let pages: Vec<u32> = (r.start..=r.end).collect();
        let out = out_dir.join(format!("{}-{}-{}-{}.pdf", stem, idx + 1, r.start, r.end));
        extract_pages_to(input, &pages, &out)?;
        outputs.push(out);
    }
    Ok(outputs)
}

/// Split `input` into chunks of `chunk_size` pages each.
pub fn split_every(
    input: &Path,
    chunk_size: u32,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, PdfError> {
    validate_input(input)?;
    if chunk_size == 0 {
        return Err(PdfError::Other("chunk size must be >= 1".into()));
    }
    let total = page_count(input)?;
    let mut ranges = Vec::new();
    let mut cur = 1u32;
    while cur <= total {
        let end = (cur + chunk_size - 1).min(total);
        ranges.push(PageRange { start: cur, end });
        cur = end + 1;
    }
    split_by_ranges(input, &ranges, out_dir)
}

/// Extract a specific set of pages (1-indexed, in caller-provided order)
/// from `input` and save as a new PDF at `output`.
///
/// This is the core kernel used by split, page-delete, page-reorder, and extract-pages.
pub fn extract_pages_to(input: &Path, pages: &[u32], output: &Path) -> Result<(), PdfError> {
    validate_input(input)?;
    if pages.is_empty() {
        return Err(PdfError::Other("no pages selected".into()));
    }
    let mut doc = Document::load(input)?;
    let total_pages = doc.get_pages().len() as u32;
    for &p in pages {
        if p == 0 || p > total_pages {
            return Err(PdfError::Other(format!(
                "page {} out of range (1..={})",
                p, total_pages
            )));
        }
    }

    // Build the set of page numbers to KEEP (1-indexed) and let lopdf's
    // built-in delete_pages do the work for the inverse.
    let keep: std::collections::BTreeSet<u32> = pages.iter().copied().collect();
    let drop: Vec<u32> = (1..=total_pages).filter(|p| !keep.contains(p)).collect();
    if !drop.is_empty() {
        doc.delete_pages(&drop);
    }

    // After delete_pages, the page order is the original order minus the dropped
    // ones. If the caller wanted a non-sorted order (e.g. reverse), we have to
    // reorder. We do that by rewriting the Pages /Kids array.
    let sorted_keep: Vec<u32> = {
        let mut v: Vec<u32> = keep.into_iter().collect();
        v.sort_unstable();
        v
    };
    if sorted_keep != pages {
        reorder_pages_inplace(&mut doc, pages, &sorted_keep)?;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(())
}

/// Rewrite the Pages /Kids array so that the surviving pages appear in
/// `desired` order. `current_sorted` is the order they exist in after the
/// delete (which is original-document order).
fn reorder_pages_inplace(
    doc: &mut Document,
    desired: &[u32],
    current_sorted: &[u32],
) -> Result<(), PdfError> {
    let page_ids = doc.get_pages(); // BTreeMap<u32, ObjectId> over CURRENT page numbers (1..=N)
    if page_ids.len() != current_sorted.len() {
        return Err(PdfError::Other("page-count mismatch during reorder".into()));
    }

    // Map each "original page number" -> its new ObjectId after deletion.
    // After delete_pages, lopdf renumbers pages 1..N in the surviving order,
    // which equals current_sorted.
    let mut original_to_new_id = std::collections::HashMap::new();
    for (i, original) in current_sorted.iter().enumerate() {
        let new_num = (i + 1) as u32;
        if let Some(&id) = page_ids.get(&new_num) {
            original_to_new_id.insert(*original, id);
        }
    }

    // Build the new Kids array in the desired order.
    let kids: Vec<Object> = desired
        .iter()
        .filter_map(|orig| {
            original_to_new_id
                .get(orig)
                .map(|id| Object::Reference(*id))
        })
        .collect();
    if kids.len() != desired.len() {
        return Err(PdfError::Other("lost pages during reorder".into()));
    }

    // Find the Pages root via the catalog.
    let catalog = doc.catalog()?;
    let pages_ref = catalog
        .get(b"Pages")
        .map_err(|_| PdfError::Other("catalog missing /Pages".into()))?
        .as_reference()
        .map_err(|_| PdfError::Other("/Pages is not a reference".into()))?;
    let pages_obj = doc.get_object_mut(pages_ref)?;
    if let Object::Dictionary(dict) = pages_obj {
        let count = kids.len() as i64;
        dict.set("Kids", kids);
        dict.set("Count", count);
    }
    Ok(())
}

pub fn page_count(input: &Path) -> Result<u32, PdfError> {
    validate_input(input)?;
    let doc = Document::load(input)?;
    Ok(doc.get_pages().len() as u32)
}

fn validate_input(input: &Path) -> Result<(), PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn page_count_works() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("3p.pdf");
        make_n_page_pdf(&p, 3);
        assert_eq!(page_count(&p).unwrap(), 3);
    }

    #[test]
    fn split_every_two() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("five.pdf");
        make_n_page_pdf(&src, 5);
        let outs = split_every(&src, 2, tmp.path()).unwrap();
        assert_eq!(outs.len(), 3);
        assert_eq!(page_count(&outs[0]).unwrap(), 2);
        assert_eq!(page_count(&outs[1]).unwrap(), 2);
        assert_eq!(page_count(&outs[2]).unwrap(), 1);
    }

    #[test]
    fn split_by_ranges_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("ten.pdf");
        make_n_page_pdf(&src, 10);
        let ranges = [
            PageRange::new(1, 3).unwrap(),
            PageRange::new(5, 5).unwrap(),
            PageRange::new(7, 10).unwrap(),
        ];
        let outs = split_by_ranges(&src, &ranges, tmp.path()).unwrap();
        assert_eq!(outs.len(), 3);
        assert_eq!(page_count(&outs[0]).unwrap(), 3);
        assert_eq!(page_count(&outs[1]).unwrap(), 1);
        assert_eq!(page_count(&outs[2]).unwrap(), 4);
    }

    #[test]
    fn extract_pages_reorders() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("five.pdf");
        let out = tmp.path().join("reordered.pdf");
        make_n_page_pdf(&src, 5);
        extract_pages_to(&src, &[5, 3, 1], &out).unwrap();
        assert_eq!(page_count(&out).unwrap(), 3);
    }

    #[test]
    fn rejects_out_of_range() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("two.pdf");
        make_n_page_pdf(&src, 2);
        let r = [PageRange::new(1, 10).unwrap()];
        let res = split_by_ranges(&src, &r, tmp.path());
        assert!(res.is_err());
    }

    #[test]
    fn rejects_zero_chunk() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("one.pdf");
        make_n_page_pdf(&src, 1);
        let res = split_every(&src, 0, tmp.path());
        assert!(res.is_err());
    }
}
