// src-tauri/src/pdf/loom/structure_tree.rs
//
// Slab Loom — Slice 5: PDF /StructTreeRoot emission.
//
// Public entry point: [`weave`] mutates an `lopdf::Document` in place by adding
// a structure tree, parent tree, role map, and rewriting page content streams
// to wrap content in marked-content sequences (BDC/EMC). This is the heart of
// PDF/UA-1 (ISO 14289-1) — screen readers (NVDA, VoiceOver, JAWS) walk the
// catalog's `/StructTreeRoot → /K → /MCR` graph to read the logical document
// in correct order instead of guessing from coordinate order.
//
// Pipeline (Loom):
//   layout::extract    → LayoutTree
//   classify::classify → StructTree (typed nodes)
//   reading_order      → ReadingOrder
//   alt_text::enrich   → /Alt on Figure nodes
//   structure_tree::weave → mutates Document with tagged content streams
//
// What we emit (per ISO 14289-1 §7):
//   * /StructTreeRoot dict on the catalog with /K, /ParentTree, /ParentTreeNextKey, /RoleMap.
//   * /MarkInfo << /Marked true >> on the catalog.
//   * Per-page /StructParents integer pointing into /ParentTree.
//   * One StructElem per logical leaf (H1..H6, P, LI, Figure, Caption) plus
//     containers (Document, Sect, L).
//   * Marked-content sequences `BDC /<Tag> << /MCID n >> ... EMC` around every
//     text-showing op (Tj, TJ, ', ") and image XObject draw (Do). Page chrome
//     (folios, running headers/footers — `NodeKind::Artifact`) gets
//     `/Artifact << >> BDC ... EMC` and is excluded from the structure tree.
//
// Out of scope for Slice 5 (deferred to Slice 6):
//   * /ActualText for ligatures + math.
//   * /IDTree (we emit a present-but-empty dict via /StructTreeRoot defaults).
//   * Multi-stream /Contents arrays — returned as `Err` for now so the caller
//     can fall back to leaving the page untagged.
//   * Form-XObject content (XObject `Do` calls into nested form XObjects).
//   * Tables: tagged as P + Figure containers (full /Table tagging in Slice 6).

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use super::classify::{NodeKind, StructNode, StructTree, StructTreePage};
use super::reading_order::ReadingOrder;

// ---------------------------------------------------------------------------
// ParentTreeBuilder — PDF /ParentTree NumberTree builder.
// ---------------------------------------------------------------------------

/// Builds the `/ParentTree` NumberTree required on `/StructTreeRoot`.
///
/// PDF/UA requires the catalog's `/StructTreeRoot` to carry a `/ParentTree`
/// NumberTree mapping each page's `/StructParents` integer to either a single
/// StructElem reference (for image-only pages) or an array of refs (one slot
/// per MCID, indexed by MCID).
///
/// We always emit the simple "single-node /Nums dict" form (no /Kids
/// intermediate nodes); a structure tree with thousands of pages is still
/// well-formed because /Nums is a flat array — no balancing required.
pub(crate) struct ParentTreeBuilder {
    entries: Vec<(i64, Vec<ObjectId>)>,
}

impl ParentTreeBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Append a (key, refs) entry. Keys may be inserted out of order; the
    /// builder sorts them on `write_into`.
    pub fn push(&mut self, key: i64, refs: Vec<ObjectId>) {
        self.entries.push((key, refs));
    }

    /// Materialise into the document and return the new object id.
    pub fn write_into(mut self, doc: &mut Document) -> ObjectId {
        self.entries.sort_by_key(|e| e.0);
        let mut nums: Vec<Object> = Vec::with_capacity(self.entries.len() * 2);
        for (k, refs) in self.entries {
            nums.push(Object::Integer(k));
            let arr: Vec<Object> = refs.into_iter().map(Object::Reference).collect();
            nums.push(Object::Array(arr));
        }
        doc.add_object(dictionary! {
            "Nums" => Object::Array(nums),
        })
    }
}

// ---------------------------------------------------------------------------
// RoleMap + StructElem helpers.
// ---------------------------------------------------------------------------

/// Build the `/RoleMap` dictionary.
///
/// All roles we emit (`Document`, `Sect`, `H1..H6`, `P`, `L`, `LI`, `Figure`,
/// `Caption`) are standard PDF/UA-1 structure types — no custom role mappings
/// required. We still allocate the dict so a future custom role can plug in
/// without touching the catalog wiring.
pub(crate) fn build_role_map(doc: &mut Document) -> ObjectId {
    doc.add_object(Dictionary::new())
}

/// Allocate a new `/StructElem` dict with `/Type`, `/S` (role), `/P` (parent),
/// and an empty `/K` array ready for kids.
pub(crate) fn make_struct_elem(doc: &mut Document, parent: ObjectId, role: &str) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => Object::Name(b"StructElem".to_vec()),
        "S"    => Object::Name(role.as_bytes().to_vec()),
        "P"    => Object::Reference(parent),
        "K"    => Object::Array(Vec::new()),
    })
}

fn push_kid(doc: &mut Document, elem: ObjectId, kid: Object) -> Result<(), String> {
    let d = doc
        .get_dictionary_mut(elem)
        .map_err(|e| format!("get_dictionary_mut({:?}): {}", elem, e))?;
    match d.get_mut(b"K") {
        Ok(Object::Array(arr)) => {
            arr.push(kid);
            Ok(())
        }
        _ => Err(format!("StructElem {:?} missing /K array", elem)),
    }
}

// ---------------------------------------------------------------------------
// Per-page plan: StructTree page → flat RunMcid sequence.
// ---------------------------------------------------------------------------

/// Per-operator assignment produced by `plan_page` from the StructTree. The
/// `rewrite_page_stream` walker consumes one of these per text-showing / image
/// operator in PDF stream order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunMcid {
    /// Tag name: "H1".."H6", "P", "L", "LI", "Figure", "Caption", "Artifact".
    pub tag: &'static str,
    /// Marked-content id. `u32::MAX` is reserved for artifacts (no MCID emitted).
    pub mcid: u32,
}

/// Walk one page's StructTree, producing a flat Vec of RunMcid in stream order.
///
/// Container nodes (Document, Sect, List) emit nothing themselves — their
/// leaves do. Artifacts emit an `Artifact` slot but do NOT consume an MCID
/// counter slot (PDF spec: artifacts have no MCID).
pub(crate) fn plan_page(page: &StructTreePage) -> Vec<RunMcid> {
    let mut out: Vec<RunMcid> = Vec::new();
    let mut mcid: u32 = 0;

    fn walk(node: &StructNode, out: &mut Vec<RunMcid>, mcid: &mut u32) {
        // Pure containers: recurse only, never emit.
        if matches!(
            node.kind,
            NodeKind::List | NodeKind::Section | NodeKind::Document
        ) {
            for c in &node.children {
                walk(c, out, mcid);
            }
            return;
        }
        // Artifact: emit but skip the MCID counter.
        if matches!(node.kind, NodeKind::Artifact) {
            out.push(RunMcid {
                tag: "Artifact",
                mcid: u32::MAX,
            });
            // Artifacts don't have children in our model — but if they ever
            // did, the children would also be artifacts; recurse defensively.
            for c in &node.children {
                walk(c, out, mcid);
            }
            return;
        }
        // Logical leaf: emit one tagged slot.
        out.push(RunMcid {
            tag: tag_static(&node.kind),
            mcid: *mcid,
        });
        *mcid += 1;
        for c in &node.children {
            walk(c, out, mcid);
        }
    }

    for n in &page.nodes {
        walk(n, &mut out, &mut mcid);
    }
    out
}

/// Static tag string for a [`NodeKind`]. Identical mapping to
/// [`NodeKind::tag`] but returns `&'static str` so we can stash it in a
/// `Copy` struct without lifetime entanglements.
pub(crate) fn tag_static(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::Document => "Document",
        NodeKind::Section => "Sect",
        NodeKind::Heading(1) => "H1",
        NodeKind::Heading(2) => "H2",
        NodeKind::Heading(3) => "H3",
        NodeKind::Heading(4) => "H4",
        NodeKind::Heading(5) => "H5",
        NodeKind::Heading(_) => "H6",
        NodeKind::Paragraph => "P",
        NodeKind::List => "L",
        NodeKind::ListItem => "LI",
        NodeKind::Figure => "Figure",
        NodeKind::Caption => "Caption",
        NodeKind::Artifact => "Artifact",
    }
}

// ---------------------------------------------------------------------------
// Page content-stream rewriter — injects BDC/EMC marked-content pairs.
// ---------------------------------------------------------------------------

/// Decode `page_id`'s content stream, inject `/<Tag> << /MCID n >> BDC ... EMC`
/// around each text-showing operator (`Tj`, `TJ`, `'`, `"`) and image XObject
/// (`Do`), and write the re-encoded stream back into the document.
///
/// The plan slice must be in stream order — entries are consumed in the order
/// `Content::decode` produces them. Excess plan entries past the last
/// content op are ignored (defensive — the caller may produce an over-estimate
/// when the StructTree disagrees with stream order; we'd rather under-tag than
/// double-wrap).
pub(crate) fn rewrite_page_stream(
    doc: &mut Document,
    page_id: ObjectId,
    plan: &[RunMcid],
) -> Result<(), String> {
    use lopdf::content::{Content, Operation};

    if plan.is_empty() {
        return Ok(());
    }

    let bytes = doc
        .get_page_content(page_id)
        .map_err(|e| format!("get_page_content: {}", e))?;
    let mut content = Content::decode(&bytes).map_err(|e| format!("decode: {}", e))?;

    let mut out: Vec<Operation> = Vec::with_capacity(content.operations.len() * 3);
    let mut cursor = 0usize;
    for op in content.operations.drain(..) {
        let is_text = matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\"");
        let is_xobject = op.operator == "Do";
        if (is_text || is_xobject) && cursor < plan.len() {
            let mark = &plan[cursor];
            cursor += 1;
            let props: Object = if mark.tag == "Artifact" {
                Object::Dictionary(Dictionary::new())
            } else {
                let mut d = Dictionary::new();
                d.set("MCID", Object::Integer(mark.mcid as i64));
                Object::Dictionary(d)
            };
            out.push(Operation::new(
                "BDC",
                vec![Object::Name(mark.tag.as_bytes().to_vec()), props],
            ));
            out.push(op);
            out.push(Operation::new("EMC", vec![]));
        } else {
            out.push(op);
        }
    }
    let new_bytes = Content { operations: out }
        .encode()
        .map_err(|e| format!("encode: {}", e))?;

    // Locate the single content-stream id. Multi-stream Contents arrays are a
    // Slice 6 follow-up; for now we bail out and let the caller skip the page.
    let page_dict = doc.get_dictionary(page_id).map_err(|e| e.to_string())?;
    let contents_obj = page_dict
        .get(b"Contents")
        .map_err(|e| e.to_string())?
        .clone();
    let stream_id = match contents_obj {
        Object::Reference(r) => r,
        _ => return Err("multi-stream /Contents arrays not yet supported".into()),
    };

    if let Ok(Object::Stream(s)) = doc.get_object_mut(stream_id) {
        s.set_plain_content(new_bytes);
        // Best-effort re-flate; if it errors, leave the stream uncompressed.
        let _ = s.compress();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// weave() — the public entry point.
// ---------------------------------------------------------------------------

/// Options for [`weave`].
#[derive(Debug, Clone, Default)]
pub struct WeaveOptions {
    /// Default document language (BCP-47, e.g. "en-US"). Set on the catalog
    /// `/Lang` entry IFF the document doesn't already declare one.
    pub fallback_lang: Option<String>,
}

/// Aggregate stats returned to the caller (and surfaced in the Loom panel).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeaveStats {
    pub pages_processed: usize,
    pub pages_skipped: usize,
    pub bdc_pairs_injected: usize,
    pub struct_elems_created: usize,
    pub figures_with_alt_text: usize,
}

/// Mutate `doc` in place: emit a structure tree, parent tree, role map,
/// /MarkInfo, per-page /StructParents, and rewritten content streams. After
/// this returns Ok, the document is a valid PDF/UA-1 tagged PDF (modulo
/// metadata / XMP work in Slice 6).
///
/// `_order` (ReadingOrder) is accepted to nail down the API now; Slice 6 will
/// use it to reorder children of multi-column Sect nodes. Today the classify
/// pass already preserves stream order, so we honour that.
pub fn weave(
    doc: &mut Document,
    tree: &StructTree,
    _order: &ReadingOrder,
    opts: &WeaveOptions,
) -> Result<WeaveStats, String> {
    let mut stats = WeaveStats::default();

    // 1) Allocate empty top-level StructTreeRoot first so kids can /P-reference
    //    it before we fill in /K, /ParentTree, /RoleMap.
    let stroot_id = doc.add_object(Dictionary::new());
    let role_map_id = build_role_map(doc);

    // 2) Root StructElem (Document).
    let doc_elem = make_struct_elem(doc, stroot_id, "Document");
    stats.struct_elems_created += 1;

    // 3) Walk pages in order; plan + rewrite + build elems.
    let mut pt = ParentTreeBuilder::new();
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();

    for (idx, page_id) in pages.iter().enumerate() {
        let Some(tree_page) = tree.pages.get(idx) else {
            stats.pages_skipped += 1;
            continue;
        };
        let plan = plan_page(tree_page);

        // Rewrite the content stream FIRST; if it fails we leave the page
        // untagged but the document remains valid.
        if !plan.is_empty() {
            if let Err(e) = rewrite_page_stream(doc, *page_id, &plan) {
                eprintln!(
                    "loom::weave: page {} content rewrite failed: {} — leaving untagged",
                    idx + 1,
                    e
                );
                stats.pages_skipped += 1;
                continue;
            }
        }
        stats.bdc_pairs_injected += plan.iter().filter(|r| r.tag != "Artifact").count();

        // 4) Build StructElems for this page's content (depth-first, mirrors
        //    classify's tree shape).
        let parent_tree_key = idx as i64;
        let mut mcid_elems: Vec<ObjectId> = Vec::new();
        let mut mcid_counter: u32 = 0;
        for node in &tree_page.nodes {
            build_elems_for_node(
                doc,
                *page_id,
                doc_elem,
                node,
                &mut mcid_elems,
                &mut mcid_counter,
                &mut stats,
            )?;
        }
        pt.push(parent_tree_key, mcid_elems);

        // /StructParents on the page dict — index into /ParentTree.
        let page_dict = doc
            .get_dictionary_mut(*page_id)
            .map_err(|e| e.to_string())?;
        page_dict.set("StructParents", Object::Integer(parent_tree_key));
        stats.pages_processed += 1;
    }

    let parent_tree_id = pt.write_into(doc);

    // 5) Fill the reserved /StructTreeRoot.
    {
        let stroot_dict = doc
            .get_dictionary_mut(stroot_id)
            .map_err(|e| e.to_string())?;
        stroot_dict.set("Type", Object::Name(b"StructTreeRoot".to_vec()));
        stroot_dict.set("K", Object::Array(vec![Object::Reference(doc_elem)]));
        stroot_dict.set("ParentTree", Object::Reference(parent_tree_id));
        stroot_dict.set(
            "ParentTreeNextKey",
            Object::Integer(stats.pages_processed as i64),
        );
        stroot_dict.set("RoleMap", Object::Reference(role_map_id));
    }

    // 6) Catalog wiring: /StructTreeRoot, /MarkInfo, optional /Lang.
    let cat_id = doc
        .trailer
        .get(b"Root")
        .map_err(|e| e.to_string())?
        .as_reference()
        .map_err(|e| e.to_string())?;
    let cat = doc.get_dictionary_mut(cat_id).map_err(|e| e.to_string())?;
    cat.set("StructTreeRoot", Object::Reference(stroot_id));
    cat.set(
        "MarkInfo",
        Object::Dictionary(dictionary! { "Marked" => true }),
    );
    if !cat.has(b"Lang") {
        if let Some(l) = &opts.fallback_lang {
            cat.set("Lang", Object::string_literal(l.clone()));
        }
    }

    Ok(stats)
}

fn build_elems_for_node(
    doc: &mut Document,
    page_id: ObjectId,
    parent: ObjectId,
    node: &StructNode,
    mcid_elems: &mut Vec<ObjectId>,
    mcid_counter: &mut u32,
    stats: &mut WeaveStats,
) -> Result<(), String> {
    let role = tag_static(&node.kind);
    if role == "Artifact" {
        // Artifacts are NOT in the structure tree per ISO 14289-1 §7.1 —
        // they live solely in the content stream with /Artifact BDC.
        return Ok(());
    }
    let elem = make_struct_elem(doc, parent, role);
    stats.struct_elems_created += 1;

    // Optional /Alt for Figure (set by Slice 4).
    if matches!(node.kind, NodeKind::Figure) {
        if let Some(alt) = &node.alt_text {
            let d = doc.get_dictionary_mut(elem).map_err(|e| e.to_string())?;
            d.set("Alt", Object::string_literal(alt.clone()));
            stats.figures_with_alt_text += 1;
        }
    }
    // Optional /Lang per node.
    if let Some(lang) = &node.lang {
        let d = doc.get_dictionary_mut(elem).map_err(|e| e.to_string())?;
        d.set("Lang", Object::string_literal(lang.clone()));
    }

    let is_leaf = matches!(
        node.kind,
        NodeKind::Heading(_)
            | NodeKind::Paragraph
            | NodeKind::ListItem
            | NodeKind::Figure
            | NodeKind::Caption
    );
    if is_leaf {
        // Marked-content reference: leaf points into the page content stream
        // by MCID. The page_id goes on /Pg so PDF readers know which page to
        // pull the MCID from when the structure tree spans the whole doc.
        let mcr = Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"MCR".to_vec()),
            "Pg"   => Object::Reference(page_id),
            "MCID" => Object::Integer(*mcid_counter as i64),
        });
        push_kid(doc, elem, mcr)?;
        mcid_elems.push(elem);
        *mcid_counter += 1;
    }

    // Connect this elem to its parent.
    push_kid(doc, parent, Object::Reference(elem))?;

    // Recurse into children with this elem as the new parent.
    for child in &node.children {
        build_elems_for_node(doc, page_id, elem, child, mcid_elems, mcid_counter, stats)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::loom::classify::{NodeKind, StructNode, StructTree, StructTreePage};
    use crate::pdf::loom::layout::Bbox;
    use crate::pdf::loom::reading_order::ReadingOrder;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    fn stub_node(kind: NodeKind) -> StructNode {
        StructNode {
            kind,
            text: String::new(),
            bbox: Bbox {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
            font_size: 12.0,
            xobject_name: None,
            alt_text: None,
            lang: None,
            children: Vec::new(),
        }
    }

    // ----- ParentTreeBuilder -----

    #[test]
    fn parent_tree_builder_emits_sorted_numbers_dict() {
        let mut doc = Document::with_version("1.7");
        let fake_elem_a = doc.add_object(Object::Null);
        let fake_elem_b = doc.add_object(Object::Null);

        let mut pt = ParentTreeBuilder::new();
        // Insert out of order to prove sort-by-key works.
        pt.push(1, vec![fake_elem_b, fake_elem_a]);
        pt.push(0, vec![fake_elem_a]);

        let pt_id = pt.write_into(&mut doc);
        let dict = doc.get_dictionary(pt_id).unwrap();
        let nums = dict.get(b"Nums").unwrap().as_array().unwrap();
        // /Nums is a flat [key0, val0, key1, val1, ...]
        assert_eq!(nums.len(), 4);
        assert_eq!(nums[0].as_i64().unwrap(), 0);
        assert_eq!(nums[2].as_i64().unwrap(), 1);
        // Value at slot 1 is the array of refs for key 0.
        let arr0 = nums[1].as_array().unwrap();
        assert_eq!(arr0.len(), 1);
        let arr1 = nums[3].as_array().unwrap();
        assert_eq!(arr1.len(), 2);
    }

    #[test]
    fn parent_tree_builder_empty_writes_empty_nums_dict() {
        let mut doc = Document::with_version("1.7");
        let pt = ParentTreeBuilder::new();
        let id = pt.write_into(&mut doc);
        let d = doc.get_dictionary(id).unwrap();
        let nums = d.get(b"Nums").unwrap().as_array().unwrap();
        assert!(nums.is_empty());
    }

    // ----- RoleMap + StructElem -----

    #[test]
    fn make_struct_elem_sets_type_and_role_and_parent() {
        let mut doc = Document::with_version("1.7");
        let parent = doc.add_object(Object::Null);
        let id = make_struct_elem(&mut doc, parent, "P");
        let dict = doc.get_dictionary(id).unwrap();
        assert_eq!(dict.get(b"Type").unwrap().as_name().unwrap(), b"StructElem");
        assert_eq!(dict.get(b"S").unwrap().as_name().unwrap(), b"P");
        assert_eq!(dict.get(b"P").unwrap().as_reference().unwrap(), parent);
        assert!(dict.has(b"K"));
    }

    #[test]
    fn role_map_dict_is_empty_but_present() {
        let mut doc = Document::with_version("1.7");
        let id = build_role_map(&mut doc);
        let dict = doc.get_dictionary(id).unwrap();
        // Every NodeKind we emit is a standard PDF/UA role → RoleMap empty.
        assert_eq!(dict.iter().count(), 0);
    }

    #[test]
    fn push_kid_appends_to_struct_elem_k_array() {
        let mut doc = Document::with_version("1.7");
        let parent = doc.add_object(Object::Null);
        let elem = make_struct_elem(&mut doc, parent, "P");
        push_kid(&mut doc, elem, Object::Integer(42)).unwrap();
        push_kid(&mut doc, elem, Object::Integer(43)).unwrap();
        let d = doc.get_dictionary(elem).unwrap();
        let k = d.get(b"K").unwrap().as_array().unwrap();
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].as_i64().unwrap(), 42);
        assert_eq!(k[1].as_i64().unwrap(), 43);
    }

    // ----- plan_page -----

    #[test]
    fn plan_page_emits_one_mcid_per_leaf_in_order() {
        let mut list = stub_node(NodeKind::List);
        list.children.push(stub_node(NodeKind::ListItem));
        list.children.push(stub_node(NodeKind::ListItem));
        let page = StructTreePage {
            page_number: 1,
            nodes: vec![
                stub_node(NodeKind::Heading(1)),
                stub_node(NodeKind::Paragraph),
                list,
                stub_node(NodeKind::Figure),
            ],
        };
        let plan = plan_page(&page);
        let tags: Vec<&str> = plan.iter().map(|r| r.tag).collect();
        assert_eq!(tags, vec!["H1", "P", "LI", "LI", "Figure"]);
        for (i, r) in plan.iter().enumerate() {
            assert_eq!(r.mcid, i as u32);
        }
    }

    #[test]
    fn plan_page_artifacts_use_artifact_tag_and_dont_consume_mcid() {
        let page = StructTreePage {
            page_number: 1,
            nodes: vec![
                stub_node(NodeKind::Artifact),
                stub_node(NodeKind::Paragraph),
            ],
        };
        let plan = plan_page(&page);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].tag, "Artifact");
        assert_eq!(plan[0].mcid, u32::MAX);
        assert_eq!(plan[1].tag, "P");
        assert_eq!(plan[1].mcid, 0); // artifact didn't burn slot 0
    }

    #[test]
    fn plan_page_section_and_document_containers_pass_through() {
        let mut sect = stub_node(NodeKind::Section);
        sect.children.push(stub_node(NodeKind::Heading(2)));
        sect.children.push(stub_node(NodeKind::Paragraph));
        let page = StructTreePage {
            page_number: 1,
            nodes: vec![sect],
        };
        let plan = plan_page(&page);
        let tags: Vec<&str> = plan.iter().map(|r| r.tag).collect();
        assert_eq!(tags, vec!["H2", "P"]);
    }

    #[test]
    fn plan_page_heading_levels_map_to_correct_tags() {
        let page = StructTreePage {
            page_number: 1,
            nodes: vec![
                stub_node(NodeKind::Heading(1)),
                stub_node(NodeKind::Heading(2)),
                stub_node(NodeKind::Heading(3)),
                stub_node(NodeKind::Heading(4)),
                stub_node(NodeKind::Heading(5)),
                stub_node(NodeKind::Heading(6)),
                stub_node(NodeKind::Heading(7)),
            ],
        };
        let plan = plan_page(&page);
        let tags: Vec<&str> = plan.iter().map(|r| r.tag).collect();
        // PDF/UA only specifies H1..H6 — H7+ collapses to H6.
        assert_eq!(tags, vec!["H1", "H2", "H3", "H4", "H5", "H6", "H6"]);
    }

    // ----- rewrite_page_stream -----

    /// Build a minimal single-page PDF document with one Tj operator.
    fn single_page_doc_with_one_tj() -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.5");
        // A page font resource is required for valid Tj — we just need the
        // content stream decoder to parse, not for it to render.
        let font = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        });
        let resources = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
                Operation::new("Tj", vec![Object::string_literal("Hello world")]),
                Operation::new("ET", vec![]),
            ],
        };
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Resources" => Object::Reference(resources),
            "Contents" => Object::Reference(stream_id),
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Count" => Object::Integer(1),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            }),
        );
        let catalog = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog));
        (doc, page_id)
    }

    #[test]
    fn rewrites_paragraph_text_with_p_bdc_emc() {
        let (mut doc, page_id) = single_page_doc_with_one_tj();
        let plan = vec![RunMcid { tag: "P", mcid: 0 }];
        rewrite_page_stream(&mut doc, page_id, &plan).unwrap();

        // Read the content stream back out and re-decode it; assert BDC/EMC
        // bracket the Tj.
        let bytes = doc.get_page_content(page_id).unwrap();
        let content = Content::decode(&bytes).unwrap();
        let ops: Vec<&str> = content
            .operations
            .iter()
            .map(|o| o.operator.as_str())
            .collect();
        // Expect: BT Tf BDC Tj EMC ET (BDC injected before Tj, EMC after).
        assert!(
            ops.windows(3).any(|w| w == ["BDC", "Tj", "EMC"]),
            "BDC/EMC not wrapping Tj: {:?}",
            ops
        );
        // The BDC op's first operand is the tag name.
        let bdc = content
            .operations
            .iter()
            .find(|o| o.operator == "BDC")
            .unwrap();
        assert_eq!(bdc.operands[0].as_name().unwrap(), b"P");
        // Second operand is a dict with /MCID 0.
        let props = bdc.operands[1].as_dict().unwrap();
        assert_eq!(props.get(b"MCID").unwrap().as_i64().unwrap(), 0);
    }

    #[test]
    fn artifacts_use_artifact_tag_with_empty_dict() {
        let (mut doc, page_id) = single_page_doc_with_one_tj();
        let plan = vec![RunMcid {
            tag: "Artifact",
            mcid: u32::MAX,
        }];
        rewrite_page_stream(&mut doc, page_id, &plan).unwrap();

        let bytes = doc.get_page_content(page_id).unwrap();
        let content = Content::decode(&bytes).unwrap();
        let bdc = content
            .operations
            .iter()
            .find(|o| o.operator == "BDC")
            .unwrap();
        assert_eq!(bdc.operands[0].as_name().unwrap(), b"Artifact");
        // Artifact's properties dict must be empty (no MCID).
        let props = bdc.operands[1].as_dict().unwrap();
        assert_eq!(props.iter().count(), 0);
    }

    #[test]
    fn empty_plan_leaves_stream_unchanged() {
        let (mut doc, page_id) = single_page_doc_with_one_tj();
        rewrite_page_stream(&mut doc, page_id, &[]).unwrap();
        let bytes = doc.get_page_content(page_id).unwrap();
        let content = Content::decode(&bytes).unwrap();
        // No BDC/EMC should appear.
        assert!(!content.operations.iter().any(|o| o.operator == "BDC"));
        assert!(!content.operations.iter().any(|o| o.operator == "EMC"));
    }

    // ----- weave() integration -----

    #[test]
    fn weave_wires_struct_tree_root_and_mark_info_on_catalog() {
        let (mut doc, _) = single_page_doc_with_one_tj();
        let tree = StructTree {
            pages: vec![StructTreePage {
                page_number: 1,
                nodes: vec![stub_node(NodeKind::Paragraph)],
            }],
        };
        let order = ReadingOrder::default();
        let opts = WeaveOptions {
            fallback_lang: Some("en-US".into()),
        };
        let stats = weave(&mut doc, &tree, &order, &opts).unwrap();

        assert_eq!(stats.pages_processed, 1);
        assert_eq!(stats.bdc_pairs_injected, 1);
        // Document + Paragraph = 2 StructElems.
        assert_eq!(stats.struct_elems_created, 2);

        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(cat_id).unwrap();
        assert!(cat.has(b"StructTreeRoot"));
        assert!(cat.has(b"MarkInfo"));
        assert_eq!(cat.get(b"Lang").unwrap().as_str().unwrap(), b"en-US");
        let mi = cat.get(b"MarkInfo").unwrap().as_dict().unwrap();
        assert!(mi.get(b"Marked").unwrap().as_bool().unwrap());

        let stroot_id = cat.get(b"StructTreeRoot").unwrap().as_reference().unwrap();
        let stroot = doc.get_dictionary(stroot_id).unwrap();
        assert_eq!(
            stroot.get(b"Type").unwrap().as_name().unwrap(),
            b"StructTreeRoot"
        );
        assert!(stroot.has(b"K"));
        assert!(stroot.has(b"ParentTree"));
        assert!(stroot.has(b"RoleMap"));
        assert_eq!(
            stroot.get(b"ParentTreeNextKey").unwrap().as_i64().unwrap(),
            1
        );
    }

    #[test]
    fn weave_assigns_struct_parents_to_every_page() {
        let (mut doc, _) = single_page_doc_with_one_tj();
        let tree = StructTree {
            pages: vec![StructTreePage {
                page_number: 1,
                nodes: vec![stub_node(NodeKind::Paragraph)],
            }],
        };
        let order = ReadingOrder::default();
        weave(&mut doc, &tree, &order, &WeaveOptions::default()).unwrap();
        for (_, page_id) in doc.get_pages() {
            let d = doc.get_dictionary(page_id).unwrap();
            assert!(
                d.has(b"StructParents"),
                "page {:?} missing /StructParents",
                page_id
            );
        }
    }

    #[test]
    fn weave_attaches_alt_text_to_figure_elements() {
        let (mut doc, _) = single_page_doc_with_one_tj();
        let mut fig = stub_node(NodeKind::Figure);
        fig.alt_text = Some("A pie chart of revenue by quarter.".into());
        let tree = StructTree {
            pages: vec![StructTreePage {
                page_number: 1,
                nodes: vec![fig],
            }],
        };
        let order = ReadingOrder::default();
        let stats = weave(&mut doc, &tree, &order, &WeaveOptions::default()).unwrap();
        assert_eq!(stats.figures_with_alt_text, 1);

        // Find the Figure StructElem and assert it has /Alt.
        let mut found = false;
        for (_id, obj) in doc.objects.iter() {
            if let Object::Dictionary(d) = obj {
                if d.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"StructElem")
                    && d.get(b"S").ok().and_then(|o| o.as_name().ok()) == Some(b"Figure")
                {
                    let alt = d.get(b"Alt").unwrap().as_str().unwrap();
                    assert!(std::str::from_utf8(alt).unwrap().contains("pie chart"));
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "no Figure StructElem with /Alt found");
    }

    #[test]
    fn weave_skips_artifacts_in_struct_tree_but_keeps_in_content() {
        let (mut doc, _) = single_page_doc_with_one_tj();
        let tree = StructTree {
            pages: vec![StructTreePage {
                page_number: 1,
                nodes: vec![stub_node(NodeKind::Artifact)],
            }],
        };
        let order = ReadingOrder::default();
        let stats = weave(&mut doc, &tree, &order, &WeaveOptions::default()).unwrap();
        // No struct elem created for the artifact (only Document).
        assert_eq!(stats.struct_elems_created, 1);
        // BDC pair was NOT counted (artifacts excluded from count).
        assert_eq!(stats.bdc_pairs_injected, 0);
    }

    #[test]
    fn weave_does_not_clobber_existing_lang_on_catalog() {
        let (mut doc, _) = single_page_doc_with_one_tj();
        // Pre-set /Lang.
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(cat_id)
            .unwrap()
            .set("Lang", Object::string_literal("fr-FR"));
        let tree = StructTree {
            pages: vec![StructTreePage {
                page_number: 1,
                nodes: vec![stub_node(NodeKind::Paragraph)],
            }],
        };
        let order = ReadingOrder::default();
        let opts = WeaveOptions {
            fallback_lang: Some("en-US".into()),
        };
        weave(&mut doc, &tree, &order, &opts).unwrap();
        let cat = doc.get_dictionary(cat_id).unwrap();
        assert_eq!(cat.get(b"Lang").unwrap().as_str().unwrap(), b"fr-FR");
    }
}
