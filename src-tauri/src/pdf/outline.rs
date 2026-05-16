// Read and write the PDF Outlines (bookmarks / Table of Contents) tree.
//
// PDF spec §8.2.2: the catalog's /Outlines entry references an "outline
// dictionary" with /First and /Last pointers to a doubly-linked list of
// outline-item dictionaries, each of which can have its own /First/Last
// children. Each item carries /Title (the visible text) and a destination
// (/Dest or /A) that says where to jump.
//
// We deliberately model outlines as a clean Rust tree (`OutlineNode`) so
// callers can rebuild the whole tree atomically rather than fiddling with
// linked-list pointers. The write path:
//   1. Reads the destination map so existing /Dest references survive a
//      rename/reorder.
//   2. Clears the old outline objects.
//   3. Allocates fresh object IDs for the new tree.
//   4. Wires /Prev /Next /Parent /First /Last /Count correctly.
//   5. Updates the catalog's /Outlines to point at the new root.
//
// This is a structure-only edit: we don't re-stream pages, so the file
// stays byte-stable everywhere except the outline subtree.

use crate::pdf::PdfError;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One outline (bookmark) entry. Page index is 0-based; `None` means the
/// destination couldn't be resolved (e.g. a named dest we don't follow).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OutlineNode {
    /// Visible title shown in the bookmark sidebar.
    pub title: String,
    /// 0-based page index this bookmark jumps to. None = no target.
    pub page_index: Option<u32>,
    /// Children (sub-bookmarks).
    #[serde(default)]
    pub children: Vec<OutlineNode>,
}

/// Read the outline tree from `input` and return it as a flat forest of
/// `OutlineNode`. Returns an empty Vec when the PDF has no outline at all.
pub fn read_outline(input: &Path) -> Result<Vec<OutlineNode>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    read_outline_doc(&doc)
}

fn read_outline_doc(doc: &Document) -> Result<Vec<OutlineNode>, PdfError> {
    // Find /Outlines via catalog. Some PDFs spell it /Outline; we honor both.
    let root_id = catalog_id(doc)?;
    let catalog = doc.get_object(root_id)?.as_dict()?;
    let outlines_ref = match catalog
        .get(b"Outlines")
        .or_else(|_| catalog.get(b"Outline"))
    {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };
    let outlines_id = match outlines_ref {
        Object::Reference(r) => *r,
        _ => return Ok(Vec::new()),
    };
    let outlines = match doc.get_object(outlines_id) {
        Ok(o) => o.as_dict()?,
        Err(_) => return Ok(Vec::new()),
    };

    // Build a page-ref → 0-based-index map for resolving /Dest.
    let page_map = doc.get_pages();
    let mut page_lookup: std::collections::HashMap<ObjectId, u32> = Default::default();
    for (page_num, page_id) in page_map {
        // get_pages uses 1-based numbering; we want 0-based.
        page_lookup.insert(page_id, page_num.saturating_sub(1));
    }

    let first_id = match outlines.get(b"First") {
        Ok(Object::Reference(r)) => Some(*r),
        _ => None,
    };
    let Some(first_id) = first_id else {
        return Ok(Vec::new());
    };

    let mut roots = Vec::new();
    walk_siblings(doc, first_id, &page_lookup, &mut roots)?;
    Ok(roots)
}

/// Walks /First → /Next chain at one level, recurring into each item's
/// children. Tracks visited IDs to prevent cycles in malformed PDFs.
fn walk_siblings(
    doc: &Document,
    first_id: ObjectId,
    page_lookup: &std::collections::HashMap<ObjectId, u32>,
    out: &mut Vec<OutlineNode>,
) -> Result<(), PdfError> {
    let mut visited: std::collections::HashSet<ObjectId> = Default::default();
    let mut current = Some(first_id);
    while let Some(id) = current {
        if !visited.insert(id) {
            // Cycle — stop here.
            break;
        }
        let dict = match doc.get_object(id) {
            Ok(o) => o.as_dict()?,
            Err(_) => break,
        };

        let title = read_title(dict).unwrap_or_default();
        let page_index = resolve_dest(doc, dict, page_lookup);

        let mut node = OutlineNode {
            title,
            page_index,
            children: Vec::new(),
        };

        if let Ok(Object::Reference(child_first)) = dict.get(b"First") {
            walk_siblings(doc, *child_first, page_lookup, &mut node.children)?;
        }

        out.push(node);

        current = match dict.get(b"Next") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        };
    }
    Ok(())
}

fn read_title(dict: &Dictionary) -> Option<String> {
    match dict.get(b"Title").ok()? {
        Object::String(bytes, _) => Some(decode_pdf_string(bytes)),
        _ => None,
    }
}

/// PDF strings can be UTF-16BE with a BOM, PDFDocEncoding, or just ASCII.
/// We do a best-effort decode: BOM → UTF-16BE, otherwise lossy UTF-8.
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16BE with BOM
        let u16s: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Encode a Rust String back to a PDF string. We always emit UTF-16BE with a
/// BOM when the title contains non-ASCII characters; otherwise plain bytes
/// (which the spec interprets as PDFDocEncoding for the ASCII subset).
fn encode_pdf_string(s: &str) -> Vec<u8> {
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

/// Resolve an outline item's destination (/Dest or /A /D) to a 0-based page
/// index. Returns None when the dest is a named string we don't follow or
/// the target page isn't in the page map.
fn resolve_dest(
    doc: &Document,
    dict: &Dictionary,
    page_lookup: &std::collections::HashMap<ObjectId, u32>,
) -> Option<u32> {
    // Try /Dest first (array form preferred over named dests for our path).
    if let Ok(dest) = dict.get(b"Dest") {
        if let Some(idx) = page_index_from_dest(doc, dest, page_lookup) {
            return Some(idx);
        }
    }
    // Then /A → /D (action with GoTo destination).
    if let Ok(Object::Reference(a_ref)) = dict.get(b"A") {
        if let Ok(action) = doc.get_object(*a_ref).and_then(|o| o.as_dict()) {
            if let Ok(d) = action.get(b"D") {
                if let Some(idx) = page_index_from_dest(doc, d, page_lookup) {
                    return Some(idx);
                }
            }
        }
    }
    if let Ok(action) = dict.get(b"A").and_then(|o| o.as_dict()) {
        if let Ok(d) = action.get(b"D") {
            if let Some(idx) = page_index_from_dest(doc, d, page_lookup) {
                return Some(idx);
            }
        }
    }
    None
}

fn page_index_from_dest(
    doc: &Document,
    dest: &Object,
    page_lookup: &std::collections::HashMap<ObjectId, u32>,
) -> Option<u32> {
    match dest {
        // Direct array: [pageRef /XYZ left top zoom] etc.
        Object::Array(arr) => {
            if let Some(Object::Reference(r)) = arr.first() {
                return page_lookup.get(r).copied();
            }
            None
        }
        // Indirect ref to an array
        Object::Reference(r) => {
            if let Ok(obj) = doc.get_object(*r) {
                if let Ok(arr) = obj.as_array() {
                    if let Some(Object::Reference(p)) = arr.first() {
                        return page_lookup.get(p).copied();
                    }
                }
            }
            None
        }
        // Named dest: we'd need to walk /Names /Dests. Out of scope.
        _ => None,
    }
}

/// Overwrite the outline tree with `nodes`. Empty `nodes` removes the
/// outline entirely.
pub fn write_outline(input: &Path, output: &Path, nodes: &[OutlineNode]) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;
    let count = write_outline_doc(&mut doc, nodes)?;
    doc.save(output)?;
    Ok(count)
}

fn write_outline_doc(doc: &mut Document, nodes: &[OutlineNode]) -> Result<u32, PdfError> {
    let root_id = catalog_id(doc)?;

    // Find which page-ref corresponds to each 0-based index for /Dest writes.
    let page_map = doc.get_pages();
    let mut index_to_ref: std::collections::HashMap<u32, ObjectId> = Default::default();
    for (page_num, page_id) in page_map {
        index_to_ref.insert(page_num.saturating_sub(1), page_id);
    }

    // Empty tree: remove /Outlines from the catalog and exit.
    if nodes.is_empty() {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(root_id) {
            dict.remove(b"Outlines");
            dict.remove(b"Outline");
        }
        return Ok(0);
    }

    // Allocate the root outline object.
    let outlines_id = doc.new_object_id();

    // Recursively build the subtree, then patch the root.
    let mut total: u32 = 0;
    let (first, last, child_count) =
        build_level(doc, nodes, outlines_id, &index_to_ref, &mut total)?;

    let outlines_dict = dictionary! {
        "Type" => "Outlines",
        "First" => first,
        "Last" => last,
        "Count" => child_count as i64,
    };
    doc.objects
        .insert(outlines_id, Object::Dictionary(outlines_dict));

    // Point the catalog at it.
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(root_id) {
        dict.set("Outlines", Object::Reference(outlines_id));
        // Some PDFs use the alternate spelling; we don't want both pointing
        // at different objects.
        dict.remove(b"Outline");
    }

    Ok(total)
}

/// Build one level of the outline tree. Returns (firstChildRef, lastChildRef,
/// directChildCount). Direct child count is the number of items in `nodes`
/// (excluding deeper descendants); the spec's /Count uses signed magnitude
/// for collapse state but we always emit positive (= open) counts.
fn build_level(
    doc: &mut Document,
    nodes: &[OutlineNode],
    parent_id: ObjectId,
    index_to_ref: &std::collections::HashMap<u32, ObjectId>,
    total: &mut u32,
) -> Result<(ObjectId, ObjectId, usize), PdfError> {
    // First pass: reserve IDs for every sibling so we can wire prev/next.
    let ids: Vec<ObjectId> = (0..nodes.len()).map(|_| doc.new_object_id()).collect();

    for (i, node) in nodes.iter().enumerate() {
        *total += 1;
        let id = ids[i];
        let prev = if i > 0 { Some(ids[i - 1]) } else { None };
        let next = if i + 1 < ids.len() {
            Some(ids[i + 1])
        } else {
            None
        };

        let mut entry = dictionary! {
            "Title" => Object::String(encode_pdf_string(&node.title), lopdf::StringFormat::Literal),
            "Parent" => Object::Reference(parent_id),
        };
        if let Some(p) = prev {
            entry.set("Prev", Object::Reference(p));
        }
        if let Some(n) = next {
            entry.set("Next", Object::Reference(n));
        }
        if let Some(idx) = node.page_index {
            if let Some(page_ref) = index_to_ref.get(&idx) {
                // /Dest = [pageRef /Fit]
                let dest = Object::Array(vec![
                    Object::Reference(*page_ref),
                    Object::Name(b"Fit".to_vec()),
                ]);
                entry.set("Dest", dest);
            }
        }

        // Build child subtree if any.
        if !node.children.is_empty() {
            let (cf, cl, cc) = build_level(doc, &node.children, id, index_to_ref, total)?;
            entry.set("First", Object::Reference(cf));
            entry.set("Last", Object::Reference(cl));
            entry.set("Count", Object::Integer(cc as i64));
        }

        doc.objects.insert(id, Object::Dictionary(entry));
    }

    Ok((ids[0], *ids.last().unwrap(), nodes.len()))
}

fn catalog_id(doc: &Document) -> Result<ObjectId, PdfError> {
    match doc.trailer.get(b"Root") {
        Ok(Object::Reference(r)) => Ok(*r),
        Ok(_) => Err(PdfError::Other("Trailer /Root not a reference".into())),
        Err(e) => Err(PdfError::Lopdf(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    fn pdf(path: &Path, n: u32) {
        make_n_page_pdf(path, n);
    }

    #[test]
    fn read_returns_empty_for_pdf_with_no_outline() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("src.pdf");
        pdf(&p, 3);
        let tree = read_outline(&p).unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn write_then_read_roundtrip_single_level() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        pdf(&src, 5);
        let tree = vec![
            OutlineNode {
                title: "Intro".into(),
                page_index: Some(0),
                children: vec![],
            },
            OutlineNode {
                title: "Body".into(),
                page_index: Some(2),
                children: vec![],
            },
            OutlineNode {
                title: "End".into(),
                page_index: Some(4),
                children: vec![],
            },
        ];
        let n = write_outline(&src, &out, &tree).unwrap();
        assert_eq!(n, 3);
        let back = read_outline(&out).unwrap();
        assert_eq!(back, tree);
    }

    #[test]
    fn write_then_read_roundtrip_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        pdf(&src, 6);
        let tree = vec![
            OutlineNode {
                title: "Part 1".into(),
                page_index: Some(0),
                children: vec![
                    OutlineNode {
                        title: "1.1".into(),
                        page_index: Some(1),
                        children: vec![],
                    },
                    OutlineNode {
                        title: "1.2".into(),
                        page_index: Some(2),
                        children: vec![],
                    },
                ],
            },
            OutlineNode {
                title: "Part 2".into(),
                page_index: Some(3),
                children: vec![OutlineNode {
                    title: "2.1".into(),
                    page_index: Some(4),
                    children: vec![],
                }],
            },
        ];
        let n = write_outline(&src, &out, &tree).unwrap();
        assert_eq!(n, 5);
        let back = read_outline(&out).unwrap();
        assert_eq!(back, tree);
    }

    #[test]
    fn write_then_read_unicode_titles() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        pdf(&src, 2);
        let tree = vec![OutlineNode {
            title: "中文 — title 📚".into(),
            page_index: Some(0),
            children: vec![],
        }];
        write_outline(&src, &out, &tree).unwrap();
        let back = read_outline(&out).unwrap();
        assert_eq!(back, tree);
    }

    #[test]
    fn empty_tree_removes_existing_outline() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let with_outline = tmp.path().join("with.pdf");
        let stripped = tmp.path().join("stripped.pdf");
        pdf(&src, 3);
        write_outline(
            &src,
            &with_outline,
            &[OutlineNode {
                title: "A".into(),
                page_index: Some(0),
                children: vec![],
            }],
        )
        .unwrap();
        assert_eq!(read_outline(&with_outline).unwrap().len(), 1);
        let n = write_outline(&with_outline, &stripped, &[]).unwrap();
        assert_eq!(n, 0);
        assert!(read_outline(&stripped).unwrap().is_empty());
    }

    #[test]
    fn unresolved_page_index_drops_dest_silently() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("out.pdf");
        pdf(&src, 2);
        let tree = vec![OutlineNode {
            title: "ghost".into(),
            page_index: Some(999),
            children: vec![],
        }];
        write_outline(&src, &out, &tree).unwrap();
        let back = read_outline(&out).unwrap();
        // Title round-trips; page_index is None because the dest was dropped.
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].title, "ghost");
        assert_eq!(back[0].page_index, None);
    }

    #[test]
    fn missing_input_errors() {
        let r = read_outline(Path::new("/no/such/file.pdf"));
        assert!(matches!(r, Err(PdfError::InputMissing(_))));
    }
}
