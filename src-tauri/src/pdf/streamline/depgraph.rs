//! Object-graph dependency walk.
//!
//! Given a `lopdf::Document` and the **page number** to optimize for (1-based),
//! return the set of indirect-object IDs that must be present in the file
//! *before* the hint stream for a streaming reader to render that page.
//!
//! This is the "first-page reachable set" in PDF 1.4 §F.4.2. We walk:
//! catalog → pages tree → target page → its resources (fonts, XObjects,
//! ColorSpaces, ExtGState, Patterns, Shadings) → content streams →
//! recursively any indirect refs they contain.
//!
//! We bound recursion at 64 levels to defend against pathological cycles
//! (PDF objects can self-reference in principle).

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

/// Maximum depth when walking the object graph. PDFs rarely nest above ~12
/// (catalog → pages → page → resources → font → font_descriptor → ...).
const MAX_DEPTH: u32 = 64;

/// Compute the reachable set of object IDs from `page_num` (1-based).
///
/// The set always includes the catalog, the page-tree root, the page object
/// itself, and anything they transitively reference (subject to MAX_DEPTH).
pub fn first_page_reachable(doc: &Document, page_num: u32) -> HashSet<ObjectId> {
    let mut seen: HashSet<ObjectId> = HashSet::new();

    if let Some(catalog_id) = find_catalog_id(doc) {
        walk(doc, catalog_id, &mut seen, 0);
    }

    // Also explicitly mark the target page object so callers can find
    // it quickly even if MAX_DEPTH cut off a branch.
    if let Some(page_id) = nth_page_id(doc, page_num) {
        seen.insert(page_id);
        walk(doc, page_id, &mut seen, 0);
    }

    seen
}

/// Return the 1-based nth page object id, if it exists.
pub fn nth_page_id(doc: &Document, page_num: u32) -> Option<ObjectId> {
    if page_num == 0 {
        return None;
    }
    doc.page_iter().nth((page_num - 1) as usize)
}

fn find_catalog_id(doc: &Document) -> Option<ObjectId> {
    if let Ok(root) = doc.trailer.get(b"Root") {
        if let Ok(rid) = root.as_reference() {
            return Some(rid);
        }
    }
    // Fallback: scan for a dict with /Type /Catalog.
    doc.objects.iter().find_map(|(id, obj)| {
        if let Object::Dictionary(d) = obj {
            if d.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Catalog") {
                return Some(*id);
            }
        }
        None
    })
}

fn walk(doc: &Document, id: ObjectId, seen: &mut HashSet<ObjectId>, depth: u32) {
    if depth > MAX_DEPTH || !seen.insert(id) {
        return;
    }
    let Ok(obj) = doc.get_object(id) else { return };
    walk_obj(doc, obj, seen, depth + 1);
}

fn walk_obj(doc: &Document, obj: &Object, seen: &mut HashSet<ObjectId>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    match obj {
        Object::Reference(rid) => walk(doc, *rid, seen, depth),
        Object::Array(arr) => {
            for o in arr {
                walk_obj(doc, o, seen, depth);
            }
        }
        Object::Dictionary(d) => {
            for (_k, v) in d.iter() {
                walk_obj(doc, v, seen, depth);
            }
        }
        Object::Stream(s) => {
            for (_k, v) in s.dict.iter() {
                walk_obj(doc, v, seen, depth);
            }
            // Stream raw bytes never contain indirect refs (they're encoded).
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use tempfile::tempdir;

    #[test]
    fn reachable_set_includes_catalog_pages_and_page_1() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("walk.pdf");
        make_n_page_pdf(&p, 5);
        let doc = Document::load(&p).unwrap();

        let set = first_page_reachable(&doc, 1);
        assert!(!set.is_empty(), "reachable set must be non-empty");

        let page1 = nth_page_id(&doc, 1).expect("page 1 exists");
        assert!(
            set.contains(&page1),
            "reachable set must include the target page object"
        );

        // The reachable set should be a proper subset of all objects (the
        // remaining 4 pages and their resources should NOT be required for
        // page 1).
        let total = doc.objects.len();
        assert!(
            set.len() < total,
            "reachable set ({}) should be a strict subset of total objects ({})",
            set.len(),
            total
        );
    }

    #[test]
    fn nth_page_id_handles_out_of_range() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("walk2.pdf");
        make_n_page_pdf(&p, 2);
        let doc = Document::load(&p).unwrap();
        assert!(nth_page_id(&doc, 0).is_none());
        assert!(nth_page_id(&doc, 99).is_none());
        assert!(nth_page_id(&doc, 1).is_some());
        assert!(nth_page_id(&doc, 2).is_some());
    }

    #[test]
    fn reachable_set_is_deterministic() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("walk3.pdf");
        make_n_page_pdf(&p, 4);
        let doc = Document::load(&p).unwrap();
        let a = first_page_reachable(&doc, 2);
        let b = first_page_reachable(&doc, 2);
        assert_eq!(a, b);
    }

    #[test]
    fn find_catalog_id_returns_some_for_valid_pdf() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("walk4.pdf");
        make_n_page_pdf(&p, 1);
        let doc = Document::load(&p).unwrap();
        assert!(find_catalog_id(&doc).is_some());
    }
}
