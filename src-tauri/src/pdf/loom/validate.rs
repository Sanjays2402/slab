// src-tauri/src/pdf/loom/validate.rs
//
// Slab Loom — Slice 6: post-tag PDF/UA-1 validator.
//
// Loads a (presumably tagged) PDF and runs 8 auto-decidable failure-condition
// checks drawn from the Matterhorn Protocol 1.1 / ISO 14289-1 catalog.
//
// These eight are the ones whose decision is fully derivable from inspecting
// the PDF object graph alone — no semantic interpretation of the content
// stream required. Together they answer "is this file structurally a
// PDF/UA-1?" with high confidence (the remaining Matterhorn auto-conditions
// are content-stream-level inferences already exercised inside `weave`).
//
// Public surface:
//   * `CheckResult { id, title, passed, detail }`
//   * `ValidateReport { checks, passed, failed, overall }`
//   * `validate(doc: &Document) -> ValidateReport`
//
// `overall` is `true` iff every check passes. The UI uses `overall` for the
// green/red verdict pill and `checks` for the per-condition list.

use lopdf::{Document, Object, ObjectId};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// Matterhorn condition id (e.g. `"06-002"`) or a Slab roll-up id.
    pub id: &'static str,
    pub title: &'static str,
    pub passed: bool,
    /// Optional one-line diagnostic — empty when `passed`.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidateReport {
    pub checks: Vec<CheckResult>,
    pub passed: usize,
    pub failed: usize,
    /// True iff every check passed.
    pub overall: bool,
}

impl ValidateReport {
    fn new(checks: Vec<CheckResult>) -> Self {
        let passed = checks.iter().filter(|c| c.passed).count();
        let failed = checks.len() - passed;
        Self {
            overall: failed == 0,
            checks,
            passed,
            failed,
        }
    }
}

/// Run the eight auto-decidable PDF/UA-1 checks on `doc`.
pub fn validate(doc: &Document) -> ValidateReport {
    let checks = vec![
        check_struct_tree_present(doc),
        check_mark_info_marked(doc),
        check_catalog_lang(doc),
        check_xmp_metadata_present(doc),
        check_xmp_pdfuaid_part(doc),
        check_view_prefs_display_doc_title(doc),
        check_info_title(doc),
        check_figures_have_alt(doc),
    ];
    ValidateReport::new(checks)
}

// ---------------------------------------------------------------------------
// individual checks
// ---------------------------------------------------------------------------

fn check_struct_tree_present(doc: &Document) -> CheckResult {
    const ID: &str = "01-006";
    const TITLE: &str = "Structure tree is present and non-empty";
    let catalog = match catalog(doc) {
        Some(c) => c,
        None => return fail(ID, TITLE, "no catalog"),
    };
    let stroot_ref = match catalog
        .get(b"StructTreeRoot")
        .ok()
        .and_then(|o| o.as_reference().ok())
    {
        Some(r) => r,
        None => return fail(ID, TITLE, "catalog has no /StructTreeRoot"),
    };
    let stroot = match doc.get_dictionary(stroot_ref) {
        Ok(d) => d,
        Err(_) => return fail(ID, TITLE, "/StructTreeRoot not a dict"),
    };
    let has_k = stroot.get(b"K").ok().is_some_and(|o| match o {
        Object::Array(a) => !a.is_empty(),
        Object::Reference(_) => true,
        _ => false,
    });
    if !has_k {
        return fail(ID, TITLE, "/StructTreeRoot /K is empty");
    }
    pass(ID, TITLE)
}

fn check_mark_info_marked(doc: &Document) -> CheckResult {
    const ID: &str = "11-001";
    const TITLE: &str = "/MarkInfo /Marked is true";
    let catalog = match catalog(doc) {
        Some(c) => c,
        None => return fail(ID, TITLE, "no catalog"),
    };
    let mi = match catalog.get(b"MarkInfo").ok() {
        Some(o) => o,
        None => return fail(ID, TITLE, "catalog has no /MarkInfo"),
    };
    let dict = match mi {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(r) => match doc.get_dictionary(*r) {
            Ok(d) => d.clone(),
            Err(_) => return fail(ID, TITLE, "/MarkInfo not a dict"),
        },
        _ => return fail(ID, TITLE, "/MarkInfo not a dict"),
    };
    match dict.get(b"Marked").ok() {
        Some(Object::Boolean(true)) => pass(ID, TITLE),
        Some(Object::Boolean(false)) => fail(ID, TITLE, "/Marked is false"),
        _ => fail(ID, TITLE, "/Marked missing"),
    }
}

fn check_catalog_lang(doc: &Document) -> CheckResult {
    const ID: &str = "07-001";
    const TITLE: &str = "Document language declared (/Lang on catalog)";
    let catalog = match catalog(doc) {
        Some(c) => c,
        None => return fail(ID, TITLE, "no catalog"),
    };
    match catalog.get(b"Lang").ok() {
        Some(Object::String(s, _)) if !s.is_empty() => pass(ID, TITLE),
        _ => fail(ID, TITLE, "catalog /Lang missing or empty"),
    }
}

fn check_xmp_metadata_present(doc: &Document) -> CheckResult {
    const ID: &str = "06-002";
    const TITLE: &str = "XMP metadata stream present on catalog";
    let _ = match xmp_bytes(doc) {
        Some(b) => b,
        None => return fail(ID, TITLE, "catalog has no /Metadata stream"),
    };
    pass(ID, TITLE)
}

fn check_xmp_pdfuaid_part(doc: &Document) -> CheckResult {
    const ID: &str = "06-003";
    const TITLE: &str = "XMP declares pdfuaid:part = 1";
    let bytes = match xmp_bytes(doc) {
        Some(b) => b,
        None => return fail(ID, TITLE, "no XMP packet"),
    };
    let txt = String::from_utf8_lossy(&bytes);
    if !txt.contains("pdfuaid") {
        return fail(ID, TITLE, "no pdfuaid namespace in XMP");
    }
    if txt.contains("<pdfuaid:part>1</pdfuaid:part>")
        || txt.contains("pdfuaid:part=\"1\"")
        || txt.contains("pdfuaid:part='1'")
    {
        pass(ID, TITLE)
    } else {
        fail(ID, TITLE, "pdfuaid:part is not 1")
    }
}

fn check_view_prefs_display_doc_title(doc: &Document) -> CheckResult {
    const ID: &str = "11-002";
    const TITLE: &str = "/ViewerPreferences /DisplayDocTitle is true";
    let catalog = match catalog(doc) {
        Some(c) => c,
        None => return fail(ID, TITLE, "no catalog"),
    };
    let vp = match catalog.get(b"ViewerPreferences").ok() {
        Some(o) => o,
        None => return fail(ID, TITLE, "no /ViewerPreferences"),
    };
    let dict = match vp {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(r) => match doc.get_dictionary(*r) {
            Ok(d) => d.clone(),
            Err(_) => return fail(ID, TITLE, "/ViewerPreferences not a dict"),
        },
        _ => return fail(ID, TITLE, "/ViewerPreferences not a dict"),
    };
    match dict.get(b"DisplayDocTitle").ok() {
        Some(Object::Boolean(true)) => pass(ID, TITLE),
        _ => fail(ID, TITLE, "/DisplayDocTitle not true"),
    }
}

fn check_info_title(doc: &Document) -> CheckResult {
    const ID: &str = "06-001";
    const TITLE: &str = "Document Info dict has /Title";
    let info_ref = match doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| o.as_reference().ok())
    {
        Some(r) => r,
        None => return fail(ID, TITLE, "no /Info dictionary in trailer"),
    };
    let info = match doc.get_dictionary(info_ref) {
        Ok(d) => d,
        Err(_) => return fail(ID, TITLE, "/Info not a dict"),
    };
    match info.get(b"Title").ok() {
        Some(Object::String(s, _)) if !s.is_empty() => pass(ID, TITLE),
        _ => fail(ID, TITLE, "/Title missing or empty"),
    }
}

fn check_figures_have_alt(doc: &Document) -> CheckResult {
    // Matterhorn 17 (Figure) — every /Figure StructElem must carry /Alt or
    // /ActualText. We walk the structure tree and check each Figure.
    const ID: &str = "17-001";
    const TITLE: &str = "Every /Figure structure element has /Alt or /ActualText";
    let catalog = match catalog(doc) {
        Some(c) => c,
        None => return fail(ID, TITLE, "no catalog"),
    };
    let stroot_ref = match catalog
        .get(b"StructTreeRoot")
        .ok()
        .and_then(|o| o.as_reference().ok())
    {
        Some(r) => r,
        None => {
            // No structure tree — already failed by 01-006. Don't double-fail.
            return pass(ID, TITLE);
        }
    };
    let mut bad = 0usize;
    let mut total_figures = 0usize;
    walk_struct_tree(doc, stroot_ref, &mut |elem| {
        let s = elem
            .get(b"S")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned());
        if s.as_deref() == Some("Figure") {
            total_figures += 1;
            let has_alt = elem
                .get(b"Alt")
                .ok()
                .map(|o| matches!(o, Object::String(b, _) if !b.is_empty()))
                .unwrap_or(false);
            let has_actual = elem
                .get(b"ActualText")
                .ok()
                .map(|o| matches!(o, Object::String(b, _) if !b.is_empty()))
                .unwrap_or(false);
            if !(has_alt || has_actual) {
                bad += 1;
            }
        }
    });
    if bad == 0 {
        pass(ID, TITLE)
    } else {
        fail(
            ID,
            TITLE,
            &format!("{bad} of {total_figures} Figure elements missing /Alt"),
        )
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn catalog(doc: &Document) -> Option<&lopdf::Dictionary> {
    let id = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    doc.get_dictionary(id).ok()
}

fn xmp_bytes(doc: &Document) -> Option<Vec<u8>> {
    let cat = catalog(doc)?;
    let meta_ref = cat.get(b"Metadata").ok()?.as_reference().ok()?;
    match doc.get_object(meta_ref).ok()? {
        Object::Stream(s) => Some(s.content.clone()),
        _ => None,
    }
}

fn walk_struct_tree<F: FnMut(&lopdf::Dictionary)>(
    doc: &Document,
    root_id: ObjectId,
    visit: &mut F,
) {
    // Start at /StructTreeRoot — it doesn't itself have /S, but its /K kids do.
    let mut stack: Vec<ObjectId> = Vec::new();
    if let Ok(root) = doc.get_dictionary(root_id) {
        push_kids(root, &mut stack);
    }
    let mut seen: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Ok(d) = doc.get_dictionary(id) else {
            continue;
        };
        visit(d);
        push_kids(d, &mut stack);
    }
}

fn push_kids(d: &lopdf::Dictionary, stack: &mut Vec<ObjectId>) {
    let Ok(k) = d.get(b"K") else { return };
    match k {
        Object::Reference(r) => stack.push(*r),
        Object::Array(arr) => {
            for item in arr {
                if let Ok(r) = item.as_reference() {
                    stack.push(r);
                }
            }
        }
        _ => {}
    }
}

fn pass(id: &'static str, title: &'static str) -> CheckResult {
    CheckResult {
        id,
        title,
        passed: true,
        detail: None,
    }
}

fn fail(id: &'static str, title: &'static str, why: &str) -> CheckResult {
    CheckResult {
        id,
        title,
        passed: false,
        detail: Some(why.to_string()),
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::loom::metadata::{apply_pdfua_metadata, MetadataOptions};
    use lopdf::{dictionary, Object};

    fn blank_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let cat = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![]),
            "Count" => 0,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        doc.trailer.set("Root", cat);
        doc
    }

    /// Synthetic doc: tagged + metadata-applied. Mimics the slab pipeline.
    fn fully_compliant_doc() -> Document {
        let mut doc = blank_doc();
        // Build minimal StructTreeRoot with one Document elem holding one P.
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        // StructTreeRoot placeholder.
        let stroot_id = doc.add_object(dictionary! {});
        let doc_elem_id = doc.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(b"Document".to_vec()),
            "P" => Object::Reference(stroot_id),
        });
        // Fill StructTreeRoot.
        {
            let st = doc.get_dictionary_mut(stroot_id).unwrap();
            st.set("Type", Object::Name(b"StructTreeRoot".to_vec()));
            st.set("K", Object::Array(vec![Object::Reference(doc_elem_id)]));
        }
        {
            let cat = doc.get_dictionary_mut(cat_id).unwrap();
            cat.set("StructTreeRoot", Object::Reference(stroot_id));
            cat.set(
                "MarkInfo",
                Object::Dictionary(dictionary! { "Marked" => true }),
            );
        }
        // Now apply metadata layer (title, viewerprefs, lang, xmp).
        apply_pdfua_metadata(
            &mut doc,
            &MetadataOptions {
                title: Some("Spec".into()),
                fallback_lang: Some("en-US".into()),
                timestamp: Some("2026-05-23T16:00:00Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        doc
    }

    #[test]
    fn blank_doc_fails_everything() {
        let doc = blank_doc();
        let r = validate(&doc);
        assert!(!r.overall);
        // 8 checks, all but possibly figure-check (which passes vacuously when
        // there's no struct tree) should fail. We expect ≥6 failures.
        assert!(r.failed >= 6, "got {} failures", r.failed);
    }

    #[test]
    fn fully_compliant_doc_passes_all() {
        let doc = fully_compliant_doc();
        let r = validate(&doc);
        if !r.overall {
            let failures: Vec<_> = r
                .checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| format!("{}: {}", c.id, c.detail.as_deref().unwrap_or("")))
                .collect();
            panic!("expected overall pass, failures: {failures:#?}");
        }
        assert_eq!(r.passed, 8);
        assert_eq!(r.failed, 0);
    }

    #[test]
    fn missing_struct_tree_root_flagged_as_01_006() {
        let doc = blank_doc();
        let r = validate(&doc);
        let c = r.checks.iter().find(|c| c.id == "01-006").unwrap();
        assert!(!c.passed);
        assert!(c.detail.as_deref().unwrap().contains("StructTreeRoot"));
    }

    #[test]
    fn missing_xmp_flagged_as_06_002_and_06_003() {
        let doc = blank_doc();
        let r = validate(&doc);
        assert!(!r.checks.iter().find(|c| c.id == "06-002").unwrap().passed);
        assert!(!r.checks.iter().find(|c| c.id == "06-003").unwrap().passed);
    }

    #[test]
    fn figure_without_alt_flagged() {
        // Build a tagged doc, then add a Figure elem with no /Alt and verify
        // 17-001 fails.
        let mut doc = fully_compliant_doc();
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let stroot_id = doc
            .get_dictionary(cat_id)
            .unwrap()
            .get(b"StructTreeRoot")
            .unwrap()
            .as_reference()
            .unwrap();
        let doc_elem_id = doc
            .get_dictionary(stroot_id)
            .unwrap()
            .get(b"K")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        let figure_id = doc.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(b"Figure".to_vec()),
            "P" => Object::Reference(doc_elem_id),
        });
        // Add Figure to Document elem's /K.
        let doc_elem = doc.get_dictionary_mut(doc_elem_id).unwrap();
        doc_elem.set("K", Object::Array(vec![Object::Reference(figure_id)]));

        let r = validate(&doc);
        let c = r.checks.iter().find(|c| c.id == "17-001").unwrap();
        assert!(!c.passed);
        assert!(c.detail.as_deref().unwrap().contains("missing /Alt"));
    }

    #[test]
    fn figure_with_alt_passes() {
        let mut doc = fully_compliant_doc();
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let stroot_id = doc
            .get_dictionary(cat_id)
            .unwrap()
            .get(b"StructTreeRoot")
            .unwrap()
            .as_reference()
            .unwrap();
        let doc_elem_id = doc
            .get_dictionary(stroot_id)
            .unwrap()
            .get(b"K")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        let figure_id = doc.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(b"Figure".to_vec()),
            "P" => Object::Reference(doc_elem_id),
            "Alt" => Object::string_literal("A line chart"),
        });
        let doc_elem = doc.get_dictionary_mut(doc_elem_id).unwrap();
        doc_elem.set("K", Object::Array(vec![Object::Reference(figure_id)]));
        let r = validate(&doc);
        assert!(r.checks.iter().find(|c| c.id == "17-001").unwrap().passed);
    }

    #[test]
    fn report_counts_consistent() {
        let r = validate(&fully_compliant_doc());
        assert_eq!(r.checks.len(), 8);
        assert_eq!(r.passed + r.failed, r.checks.len());
        assert_eq!(r.overall, r.failed == 0);
    }
}
