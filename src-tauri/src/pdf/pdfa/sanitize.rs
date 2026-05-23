//! Pass 1 of the PDF/A pipeline: remove forbidden interactive features.
//!
//! ISO 19005-2 §6.6 forbids JavaScript actions, launch actions, additional
//! actions (`/AA`), and XFA forms in conformant documents. This module
//! removes them in place on a `lopdf::Document` and returns a report of
//! what was stripped so the UI can show a pre-flight summary.
//!
//! We deliberately do NOT strip `/AcroForm` itself — interactive forms
//! (AcroForm without XFA) are permitted in PDF/A-2 and removing them would
//! lose user data. We only strip the script/legacy bits.

use lopdf::{Document, Object};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SanitizeError {
    #[error("document is encrypted; decrypt before PDF/A conversion")]
    Encrypted,
    #[error("malformed document: {0}")]
    Malformed(String),
}

#[derive(Debug, Default, Clone)]
pub struct SanitizeReport {
    /// Human-readable list of catalog/page entries we removed.
    /// Useful for the pre-flight UI ("we'll remove: OpenAction, AA, ...").
    pub removed: Vec<String>,
}

impl SanitizeReport {
    /// True when sanitize found nothing to remove. Useful for fast-paths
    /// where the caller wants to skip re-saving.
    pub fn is_clean(&self) -> bool {
        self.removed.is_empty()
    }
}

/// Non-destructive variant of [`sanitize_for_pdfa`]. Clones the document
/// internally and returns the report describing what *would* be removed,
/// without touching the caller's document.
///
/// Used by the Loupe inspector (v3.0.1) to preview the sanitize delta
/// before the user commits to a conversion.
pub fn sanitize_dry_run(doc: &Document) -> Result<SanitizeReport, SanitizeError> {
    let mut scratch = doc.clone();
    sanitize_for_pdfa(&mut scratch)
}

/// Catalog-level keys that PDF/A-2 §6.6 forbids outright. They are
/// always stripped (no opt-in / opt-out).
const FORBIDDEN_CATALOG_KEYS: &[&[u8]] = &[
    b"OpenAction", // forbidden if it triggers JS or external code
    b"AA",         // additional actions — script triggers on doc open/close
];

/// Sub-trees inside `/Names` that carry JavaScript or other script
/// vectors. We strip those entries, but keep the rest of `/Names`
/// (e.g. `/Dests` for named destinations, which is allowed).
const FORBIDDEN_NAMES_SUBTREES: &[&[u8]] = &[b"JavaScript"];

/// Remove every entry ISO 19005-2 §6.6 forbids from the catalog and pages.
/// Returns a report listing what we removed (for the pre-flight UI).
///
/// Idempotent — running twice on the same document yields an empty
/// report the second time.
pub fn sanitize_for_pdfa(doc: &mut Document) -> Result<SanitizeReport, SanitizeError> {
    if doc.trailer.get(b"Encrypt").is_ok() {
        return Err(SanitizeError::Encrypted);
    }

    let mut removed = Vec::new();

    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Err(SanitizeError::Malformed("Root missing".into())),
    };

    // 1) Strip forbidden catalog keys.
    {
        let catalog = doc
            .get_object_mut(catalog_id)
            .map_err(|e| SanitizeError::Malformed(format!("catalog: {e}")))?
            .as_dict_mut()
            .map_err(|e| SanitizeError::Malformed(format!("catalog dict: {e}")))?;

        for key in FORBIDDEN_CATALOG_KEYS {
            if catalog.has(key) {
                catalog.remove(key);
                removed.push(String::from_utf8_lossy(key).into_owned());
            }
        }

        // 2) Strip /XFA out of AcroForm (keep the rest of the form).
        if catalog.has(b"AcroForm") {
            if let Ok(form) = catalog.get_mut(b"AcroForm").and_then(|o| o.as_dict_mut()) {
                if form.has(b"XFA") {
                    form.remove(b"XFA");
                    removed.push("AcroForm.XFA".into());
                }
                // NeedAppearances=true is fine in PDF/A-2.
            }
        }
    }

    // 3) Strip /Names/JavaScript while keeping other named trees.
    //    We resolve Names through one level of indirection because the
    //    catalog usually holds it as a Reference.
    let names_obj_id = {
        let catalog = doc
            .get_object(catalog_id)
            .map_err(|e| SanitizeError::Malformed(format!("catalog re-get: {e}")))?
            .as_dict()
            .map_err(|e| SanitizeError::Malformed(format!("catalog re-dict: {e}")))?;
        match catalog.get(b"Names") {
            Ok(Object::Reference(id)) => Some(*id),
            Ok(Object::Dictionary(_)) => None, // inline — handled below
            _ => None,
        }
    };

    if let Some(id) = names_obj_id {
        if let Ok(names_dict) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
            for sub in FORBIDDEN_NAMES_SUBTREES {
                if names_dict.has(sub) {
                    names_dict.remove(sub);
                    removed.push(format!("Names.{}", String::from_utf8_lossy(sub)));
                }
            }
        }
    } else {
        // Inline Names dict on the catalog (rarer but valid).
        if let Ok(catalog) = doc.get_object_mut(catalog_id).and_then(|o| o.as_dict_mut()) {
            if let Ok(names_dict) = catalog.get_mut(b"Names").and_then(|o| o.as_dict_mut()) {
                for sub in FORBIDDEN_NAMES_SUBTREES {
                    if names_dict.has(sub) {
                        names_dict.remove(sub);
                        removed.push(format!("Names.{}", String::from_utf8_lossy(sub)));
                    }
                }
            }
        }
    }

    // 4) Strip /AA from every page.
    //    Collect page IDs first so we don't borrow `doc` while iterating.
    let page_ids: Vec<lopdf::ObjectId> = doc.get_pages().values().copied().collect();
    let mut pages_aa_stripped: u32 = 0;
    for page_id in page_ids {
        if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
            if page.has(b"AA") {
                page.remove(b"AA");
                pages_aa_stripped += 1;
            }
        }
    }
    if pages_aa_stripped > 0 {
        removed.push(format!("Pages.AA × {pages_aa_stripped}"));
    }

    Ok(SanitizeReport { removed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary, Document, Object};

    fn fresh_doc_with_catalog(catalog: Dictionary) -> Document {
        let mut doc = Document::with_version("1.7");
        let _pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Count" => 0,
            "Kids" => vec![],
        });
        let cat_id = doc.add_object(catalog);
        doc.trailer.set("Root", Object::Reference(cat_id));
        doc
    }

    #[test]
    fn sanitize_removes_open_action() {
        let mut doc = fresh_doc_with_catalog(dictionary! {
            "Type" => "Catalog",
            "OpenAction" => dictionary! {
                "Type" => "Action",
                "S" => "JavaScript",
                "JS" => Object::string_literal("app.alert('PWNED')"),
            },
        });
        let report = sanitize_for_pdfa(&mut doc).unwrap();
        assert!(report.removed.iter().any(|s| s == "OpenAction"));

        let root_id = match doc.trailer.get(b"Root").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!(),
        };
        let catalog = doc.get_object(root_id).unwrap().as_dict().unwrap();
        assert!(catalog.get(b"OpenAction").is_err());
    }

    #[test]
    fn sanitize_removes_catalog_aa() {
        let mut doc = fresh_doc_with_catalog(dictionary! {
            "Type" => "Catalog",
            "AA" => dictionary! { "WC" => dictionary! { "S" => "JavaScript" } },
        });
        let report = sanitize_for_pdfa(&mut doc).unwrap();
        assert!(report.removed.iter().any(|s| s == "AA"));
    }

    #[test]
    fn sanitize_removes_xfa_inside_acroform_but_keeps_form() {
        let mut doc = fresh_doc_with_catalog(dictionary! {
            "Type" => "Catalog",
            "AcroForm" => dictionary! {
                "Fields" => vec![],
                "XFA" => Object::string_literal("<xdp:xdp xmlns:xdp=\"...\"/>"),
            },
        });
        let report = sanitize_for_pdfa(&mut doc).unwrap();
        assert!(report.removed.iter().any(|s| s == "AcroForm.XFA"));

        // AcroForm itself must still be present.
        let root_id = match doc.trailer.get(b"Root").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!(),
        };
        let catalog = doc.get_object(root_id).unwrap().as_dict().unwrap();
        let form = catalog.get(b"AcroForm").unwrap().as_dict().unwrap();
        assert!(form.has(b"Fields"));
        assert!(!form.has(b"XFA"));
    }

    #[test]
    fn sanitize_removes_names_javascript_subtree() {
        let mut doc = fresh_doc_with_catalog(dictionary! {
            "Type" => "Catalog",
            "Names" => dictionary! {
                "JavaScript" => dictionary! { "Names" => vec![] },
                "Dests" => dictionary! { "Names" => vec![] },
            },
        });
        let report = sanitize_for_pdfa(&mut doc).unwrap();
        assert!(
            report.removed.iter().any(|s| s == "Names.JavaScript"),
            "expected Names.JavaScript removal, got {:?}",
            report.removed
        );

        let root_id = match doc.trailer.get(b"Root").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!(),
        };
        let catalog = doc.get_object(root_id).unwrap().as_dict().unwrap();
        let names = catalog.get(b"Names").unwrap().as_dict().unwrap();
        assert!(!names.has(b"JavaScript"));
        assert!(names.has(b"Dests"), "Dests must be preserved");
    }

    #[test]
    fn sanitize_rejects_encrypted_documents() {
        let mut doc = Document::with_version("1.7");
        doc.trailer.set("Encrypt", Object::Reference((1, 0)));
        let err = sanitize_for_pdfa(&mut doc).unwrap_err();
        assert!(matches!(err, SanitizeError::Encrypted));
    }

    #[test]
    fn sanitize_is_idempotent_on_clean_document() {
        let mut doc = fresh_doc_with_catalog(dictionary! { "Type" => "Catalog" });
        let r1 = sanitize_for_pdfa(&mut doc).unwrap();
        let r2 = sanitize_for_pdfa(&mut doc).unwrap();
        assert!(r1.is_clean());
        assert!(r2.is_clean());
    }

    #[test]
    fn sanitize_report_clean_flag() {
        let r = SanitizeReport::default();
        assert!(r.is_clean());
        let r = SanitizeReport {
            removed: vec!["x".into()],
        };
        assert!(!r.is_clean());
    }

    #[test]
    fn sanitize_missing_root_is_malformed() {
        let mut doc = Document::with_version("1.7");
        let err = sanitize_for_pdfa(&mut doc).unwrap_err();
        assert!(matches!(err, SanitizeError::Malformed(_)));
    }
}
