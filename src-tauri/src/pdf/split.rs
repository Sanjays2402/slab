// Split a PDF.
//
// Two modes:
//   - Ranges: explicit page lists (e.g. [(1,3), (5,5), (7,10)]) -> N output files.
//   - Every: split into chunks of N pages each -> ceil(total/N) output files.
//
// Pages are 1-indexed in the public API to match every PDF UX in existence.

use crate::pdf::PdfError;
use lopdf::{Document, Object, ObjectId};
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
        let end = cur.saturating_add(chunk_size - 1).min(total);
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
    flatten_page_tree(&mut doc)?;
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

/// Attributes a page may inherit from an ancestor /Pages node (PDF 32000 7.7.3.4).
const INHERITABLE: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Make every page a direct child of the root /Pages node.
///
/// The extract kernel rewrites only the root /Kids list, so a document with
/// intermediate page-tree nodes would lose pages (a 3-page PDF came back as 2).
/// This copies inherited attributes down onto each leaf first, so pages render
/// the same. Intermediate nodes are left as unreferenced objects.
fn flatten_page_tree(doc: &mut Document) -> Result<(), PdfError> {
    let root_id = doc
        .catalog()?
        .get(b"Pages")
        .and_then(|o| o.as_reference())
        .map_err(|_| PdfError::Other("catalog missing /Pages".into()))?;
    let mut leaves: Vec<(ObjectId, Vec<(&'static [u8], Object)>)> = Vec::new();
    collect_leaves(doc, root_id, &[], &mut leaves, 0)?;
    if leaves.is_empty() {
        return Err(PdfError::Other("document has no pages".into()));
    }
    for (id, inherited) in &leaves {
        let dict = match doc.get_object_mut(*id)? {
            Object::Dictionary(d) => d,
            _ => return Err(PdfError::Other("page is not a dictionary".into())),
        };
        for (key, value) in inherited {
            if !dict.has(key) {
                dict.set(key.to_vec(), value.clone());
            }
        }
        dict.set("Parent", Object::Reference(root_id));
    }
    let count = leaves.len() as i64;
    let kids: Vec<Object> = leaves
        .iter()
        .map(|(id, _)| Object::Reference(*id))
        .collect();
    if let Object::Dictionary(root) = doc.get_object_mut(root_id)? {
        root.set("Kids", kids);
        root.set("Count", count);
    }
    Ok(())
}

fn collect_leaves(
    doc: &Document,
    node: ObjectId,
    inherited: &[(&'static [u8], Object)],
    out: &mut Vec<(ObjectId, Vec<(&'static [u8], Object)>)>,
    depth: u32,
) -> Result<(), PdfError> {
    if depth > 64 {
        return Err(PdfError::Other("page tree is nested too deeply".into()));
    }
    let dict = doc.get_dictionary(node)?;
    let mut inh: Vec<(&'static [u8], Object)> = inherited.to_vec();
    for key in INHERITABLE {
        if let Ok(value) = dict.get(key) {
            inh.retain(|(k, _)| *k != key);
            inh.push((key, value.clone()));
        }
    }
    match dict.get(b"Kids").and_then(|k| k.as_array()) {
        Ok(kids) => {
            for kid in kids {
                let kid_id = kid
                    .as_reference()
                    .map_err(|_| PdfError::Other("page tree child is not a reference".into()))?;
                collect_leaves(doc, kid_id, &inh, out, depth + 1)?;
            }
        }
        Err(_) => out.push((node, inh)),
    }
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

    // Build the new Kids array in the desired order. A page that appears more
    // than once needs its own page object for each extra occurrence: a page
    // object referenced twice from /Kids is malformed and some readers reject
    // the file. The copy keeps the same /Parent and shares content streams,
    // which PDF allows.
    let mut used = std::collections::HashSet::new();
    let mut kids: Vec<Object> = Vec::with_capacity(desired.len());
    for orig in desired {
        let Some(&id) = original_to_new_id.get(orig) else {
            return Err(PdfError::Other("lost pages during reorder".into()));
        };
        let target = if used.insert(id) {
            id
        } else {
            let copy = doc.get_object(id)?.clone();
            doc.add_object(copy)
        };
        kids.push(Object::Reference(target));
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
    fn balanced_split_preserves_pages_and_source() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("ten.pdf");
        make_n_page_pdf(&src, 10);
        let original = std::fs::read(&src).unwrap();
        let ranges = [
            PageRange::new(1, 4).unwrap(),
            PageRange::new(5, 7).unwrap(),
            PageRange::new(8, 10).unwrap(),
        ];
        let outs = split_by_ranges(&src, &ranges, &tmp.path().join("parts")).unwrap();
        let counts: Vec<u32> = outs.iter().map(|p| page_count(p).unwrap()).collect();
        assert_eq!(counts, vec![4, 3, 3]);
        for (i, out) in outs.iter().enumerate() {
            let actual = Document::load(out)
                .unwrap()
                .extract_text(&(1..=counts[i]).collect::<Vec<_>>())
                .unwrap();
            let expected = Document::load(&src)
                .unwrap()
                .extract_text(&(ranges[i].start..=ranges[i].end).collect::<Vec<_>>())
                .unwrap();
            assert_eq!(actual, expected);
        }
        assert_eq!(std::fs::read(&src).unwrap(), original);
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

    /// A 3-page PDF whose root /Pages has one intermediate node holding pages 2-3.
    fn make_nested_tree_pdf(path: &Path) {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 3 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Type /Pages /Parent 2 0 R /MediaBox [0 0 300 400] /Kids [6 0 R 7 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 5 0 R /Contents 4 0 R >>",
            "<< /Type /Page /Parent 5 0 R /Contents 4 0 R >>",
        ];
        let mut o = String::from("%PDF-1.4\n");
        let mut offs = Vec::new();
        for (i, x) in objs.iter().enumerate() {
            offs.push(o.len());
            o.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, x));
        }
        let x = o.len();
        o.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            objs.len() + 1
        ));
        for q in &offs {
            o.push_str(&format!("{:010} 00000 n \n", q));
        }
        o.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            objs.len() + 1,
            x
        ));
        std::fs::write(path, o).unwrap();
    }

    #[test]
    fn reorder_nested_page_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("nested.pdf");
        let dst = tmp.path().join("out.pdf");
        make_nested_tree_pdf(&src);
        assert_eq!(page_count(&src).unwrap(), 3);
        extract_pages_to(&src, &[3, 1, 2], &dst).unwrap();
        assert_eq!(page_count(&dst).unwrap(), 3);
        // Pages 2 and 3 inherited their MediaBox from the intermediate node;
        // the copy must keep it or they would render at the wrong size.
        let doc = Document::load(&dst).unwrap();
        for (_, id) in doc.get_pages() {
            let page = doc.get_dictionary(id).unwrap();
            assert!(
                page.get(b"MediaBox").is_ok(),
                "page lost its inherited MediaBox"
            );
        }
    }

    #[test]
    fn merge_keeps_pages_from_nested_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        let out = tmp.path().join("merged.pdf");
        make_nested_tree_pdf(&a);
        make_nested_tree_pdf(&b);
        crate::pdf::merge::merge_pdfs(&[a, b], out.clone()).unwrap();
        assert_eq!(page_count(&out).unwrap(), 6);
    }

    #[test]
    fn extract_with_duplicate_page_yields_valid_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("three.pdf");
        let dst = tmp.path().join("dup.pdf");
        make_n_page_pdf(&src, 3);
        extract_pages_to(&src, &[2, 2, 1], &dst).unwrap();
        let doc = lopdf::Document::load(&dst).unwrap();
        let pages = doc.get_pages();
        let mut ids: Vec<_> = pages.values().copied().collect();
        ids.sort();
        ids.dedup();
        assert_eq!(pages.len(), ids.len(), "a page object is referenced twice");
        // Each copy carries the same content, so all three outputs read back.
        let texts: Vec<_> = pages
            .values()
            .map(|id| doc.get_page_content(*id).unwrap())
            .collect();
        assert_eq!(texts[0], texts[1]);
    }

    #[test]
    fn huge_chunk_size_does_not_overflow() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("three.pdf");
        make_n_page_pdf(&src, 3);
        let out = tmp.path().join("out");
        let files = split_every(&src, u32::MAX, &out).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(page_count(&files[0]).unwrap(), 3);
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
