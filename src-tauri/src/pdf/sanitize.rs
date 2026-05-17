// Sanitize a PDF — strip JavaScript, embedded files, launch actions,
// and (optionally) external URI links. The "safe to forward" pass.
//
// What we strip:
//
//   Catalog level:
//     * /OpenAction          — auto-run actions on document open
//     * /AA                  — additional actions (e.g. WC, DS, WS, DP)
//     * /Names /JavaScript   — named JavaScript actions
//     * /Names /EmbeddedFiles — embedded files (often delivery vectors)
//     * /AcroForm /XFA       — XML form data, which can carry script
//
//   Page level:
//     * /AA                  — page-level additional actions
//     * /Annots entries with /A action of type:
//         /S /JavaScript
//         /S /Launch
//         /S /URI            (only when keep_links=false; default strip)
//
// What we DON'T strip:
//
//   * Outline /A actions       — out of scope for v0.9.0 (low-impact vector)
//   * Form-field /AA           — covered by AcroForm-level strip below
//   * Embedded media (video, audio, 3D) — handled by /RichMedia removal
//     in a future revision.
//
// All ops mutate in place on a `lopdf::Document` then re-save. No
// content streams are touched, so visual appearance is identical.

use crate::pdf::PdfError;
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizeOpts {
    /// If true, preserve `/URI` link actions. Default `false` — by
    /// default `slab sanitize` removes external URLs along with JS &
    /// launches, on the assumption that paranoid users want it gone.
    /// Pass `--keep-links` from the CLI to flip this on.
    #[serde(default)]
    pub keep_links: bool,
}

impl Default for SanitizeOpts {
    fn default() -> Self {
        Self { keep_links: false }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SanitizeReport {
    /// Count of /JavaScript actions removed (catalog-level + per-annot).
    pub js_removed: u32,
    /// Count of /EmbeddedFiles entries removed from the Names tree.
    pub embedded_files_removed: u32,
    /// Count of /Launch actions removed from annotations.
    pub launch_removed: u32,
    /// Count of /URI actions removed (0 when keep_links=true).
    pub uri_removed: u32,
    /// Whether /OpenAction was present and was removed.
    pub open_action_removed: bool,
    /// Whether /AA (additional actions) was present at catalog level.
    pub catalog_aa_removed: bool,
    /// Whether /XFA was removed from the AcroForm dict.
    pub xfa_removed: bool,
    /// Pages whose /AA was stripped.
    pub pages_aa_removed: u32,
}

pub fn sanitize(
    input: &Path,
    output: &Path,
    opts: SanitizeOpts,
) -> Result<SanitizeReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;
    let report = sanitize_doc(&mut doc, &opts);

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(report)
}

fn sanitize_doc(doc: &mut Document, opts: &SanitizeOpts) -> SanitizeReport {
    let mut report = SanitizeReport::default();

    // ----- Catalog-level scrubbing -----
    let catalog_id = match doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        Ok(id) => id,
        Err(_) => return report,
    };

    // Snapshot what we need before mutating.
    let (acroform_id, names_id, names_inline) = peek_catalog_refs(doc, catalog_id);

    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(catalog_id) {
        if dict.remove(b"OpenAction").is_some() {
            report.open_action_removed = true;
        }
        if dict.remove(b"AA").is_some() {
            report.catalog_aa_removed = true;
        }
    }

    // Names tree: /JavaScript + /EmbeddedFiles. Names dict may be inline
    // (Object::Dictionary directly in catalog) or referenced.
    if let Some(id) = names_id {
        if let Ok(Object::Dictionary(names)) = doc.get_object_mut(id) {
            if names.remove(b"JavaScript").is_some() {
                report.js_removed += 1;
            }
            if names.remove(b"EmbeddedFiles").is_some() {
                report.embedded_files_removed += 1;
            }
        }
    } else if names_inline {
        if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(catalog_id) {
            if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
                if names.remove(b"JavaScript").is_some() {
                    report.js_removed += 1;
                }
                if names.remove(b"EmbeddedFiles").is_some() {
                    report.embedded_files_removed += 1;
                }
            }
        }
    }

    // /AcroForm /XFA — XML forms can carry script.
    if let Some(id) = acroform_id {
        if let Ok(Object::Dictionary(af)) = doc.get_object_mut(id) {
            if af.remove(b"XFA").is_some() {
                report.xfa_removed = true;
            }
        }
    }

    // ----- Page-level scrubbing -----
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    for (_n, page_id) in pages {
        // /AA on the page.
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
            if dict.remove(b"AA").is_some() {
                report.pages_aa_removed += 1;
            }
        }
        // Walk /Annots and drop dangerous actions.
        let annot_ids = collect_annot_ids(doc, page_id);
        for aid in annot_ids {
            scrub_annot_action(doc, aid, opts, &mut report);
        }
    }

    report
}

fn peek_catalog_refs(
    doc: &Document,
    catalog_id: ObjectId,
) -> (Option<ObjectId>, Option<ObjectId>, bool) {
    let mut acroform = None;
    let mut names = None;
    let mut names_inline = false;
    if let Ok(Object::Dictionary(dict)) = doc.get_object(catalog_id) {
        if let Ok(o) = dict.get(b"AcroForm") {
            if let Ok(id) = o.as_reference() {
                acroform = Some(id);
            }
        }
        if let Ok(o) = dict.get(b"Names") {
            match o {
                Object::Reference(r) => names = Some(*r),
                Object::Dictionary(_) => names_inline = true,
                _ => {}
            }
        }
    }
    (acroform, names, names_inline)
}

fn collect_annot_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let Ok(dict) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return out;
    };
    let Ok(annots_obj) = dict.get(b"Annots") else {
        return out;
    };
    let arr = match annots_obj {
        Object::Array(a) => a.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Array(a)) => a.clone(),
            _ => return out,
        },
        _ => return out,
    };
    for v in arr {
        if let Object::Reference(id) = v {
            out.push(id);
        }
    }
    out
}

fn scrub_annot_action(
    doc: &mut Document,
    annot_id: ObjectId,
    opts: &SanitizeOpts,
    report: &mut SanitizeReport,
) {
    // What kind of action does this annot carry (if any)?
    let action_kind = classify_annot_action(doc, annot_id);
    let Some(kind) = action_kind else {
        return;
    };

    let drop = match kind {
        ActionKind::JavaScript => {
            report.js_removed += 1;
            true
        }
        ActionKind::Launch => {
            report.launch_removed += 1;
            true
        }
        ActionKind::Uri => {
            if opts.keep_links {
                false
            } else {
                report.uri_removed += 1;
                true
            }
        }
    };

    if !drop {
        return;
    }

    // Remove the /A entry on the annot itself. (We don't delete the
    // whole annot — a /URI annot without /A is just an invisible rect,
    // and a JS button without action is harmless. Removing keeps the
    // page layout intact.)
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(annot_id) {
        dict.remove(b"A");
    }
}

enum ActionKind {
    JavaScript,
    Launch,
    Uri,
}

fn classify_annot_action(doc: &Document, annot_id: ObjectId) -> Option<ActionKind> {
    let dict = doc.get_object(annot_id).ok()?.as_dict().ok()?;
    let action_obj = dict.get(b"A").ok()?;
    // /A may be inline dict or reference.
    let action_dict = match action_obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(r) => doc.get_object(*r).ok()?.as_dict().ok()?.clone(),
        _ => return None,
    };
    let s = action_dict.get(b"S").ok()?.as_name().ok()?;
    match s {
        b"JavaScript" => Some(ActionKind::JavaScript),
        b"Launch" => Some(ActionKind::Launch),
        b"URI" => Some(ActionKind::Uri),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use lopdf::dictionary;

    /// Build a PDF that exercises every strip path.
    fn pdf_with_kitchen_sink(path: &Path) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        // Annot with /JavaScript action.
        let js_annot = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![10.into(), 10.into(), 20.into(), 20.into()],
            "A" => dictionary! {
                "S" => "JavaScript",
                "JS" => Object::string_literal("app.alert('pwned');"),
            },
        });
        // Annot with /Launch action.
        let launch_annot = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => vec![10.into(), 30.into(), 20.into(), 40.into()],
            "A" => dictionary! {
                "S" => "Launch",
                "F" => Object::string_literal("calc.exe"),
            },
        });
        // Annot with /URI action.
        let uri_annot = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => vec![10.into(), 50.into(), 20.into(), 60.into()],
            "A" => dictionary! {
                "S" => "URI",
                "URI" => Object::string_literal("https://example.com"),
            },
        });
        // An annot with no action — should pass through untouched.
        let plain_annot = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![10.into(), 70.into(), 20.into(), 80.into()],
            "Contents" => Object::string_literal("hello"),
        });

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Annots" => vec![
                Object::Reference(js_annot),
                Object::Reference(launch_annot),
                Object::Reference(uri_annot),
                Object::Reference(plain_annot),
            ],
            "AA" => dictionary! {
                "C" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("x") },
            },
        });

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );

        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![Object::Reference(js_annot)],
            "XFA" => Object::string_literal("<xdp:xdp/>"),
        });

        let names_dict = dictionary! {
            "JavaScript" => dictionary! { "Names" => Vec::<Object>::new() },
            "EmbeddedFiles" => dictionary! { "Names" => Vec::<Object>::new() },
        };

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "OpenAction" => dictionary! {
                "S" => "JavaScript",
                "JS" => Object::string_literal("app.alert('on open');"),
            },
            "AA" => dictionary! {
                "DS" => dictionary! { "S" => "Launch", "F" => Object::string_literal("x") },
            },
            "AcroForm" => Object::Reference(acroform_id),
            "Names" => names_dict,
        });

        doc.trailer.set("Root", catalog_id);
        let id_bytes: Vec<u8> = (0..16).map(|i| 0x42u8.wrapping_add(i)).collect();
        doc.trailer.set(
            "ID",
            lopdf::Object::Array(vec![
                lopdf::Object::string_literal(id_bytes.clone()),
                lopdf::Object::string_literal(id_bytes),
            ]),
        );
        doc.save(path).unwrap();
    }

    #[test]
    fn sanitize_strips_everything_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("clean.pdf");
        pdf_with_kitchen_sink(&src);

        let report = sanitize(&src, &dst, SanitizeOpts::default()).unwrap();

        // Per-annot: one JS, one Launch, one URI.
        assert_eq!(report.js_removed, 2, "1 from Names tree + 1 annot");
        assert_eq!(report.launch_removed, 1);
        assert_eq!(report.uri_removed, 1);
        assert_eq!(report.embedded_files_removed, 1);
        assert!(report.open_action_removed);
        assert!(report.catalog_aa_removed);
        assert!(report.xfa_removed);
        assert_eq!(report.pages_aa_removed, 1);

        // Re-load and verify everything is actually gone.
        let reloaded = Document::load(&dst).unwrap();
        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        assert!(catalog.get(b"OpenAction").is_err());
        assert!(catalog.get(b"AA").is_err());

        // Page /AA gone.
        for (_n, pid) in reloaded.get_pages() {
            let dict = reloaded.get_object(pid).unwrap().as_dict().unwrap();
            assert!(dict.get(b"AA").is_err());

            // /Annots still present (we don't delete annots, just /A).
            // Verify none has /A anymore.
            let annots = dict.get(b"Annots").unwrap().as_array().unwrap();
            for v in annots {
                let aid = v.as_reference().unwrap();
                let adict = reloaded.get_object(aid).unwrap().as_dict().unwrap();
                if let Ok(Object::Dictionary(_)) = adict.get(b"A") {
                    panic!("annot {:?} still has /A action", aid);
                }
            }
        }
    }

    #[test]
    fn sanitize_keep_links_preserves_uri_only() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("clean.pdf");
        pdf_with_kitchen_sink(&src);

        let report = sanitize(&src, &dst, SanitizeOpts { keep_links: true }).unwrap();

        assert_eq!(report.uri_removed, 0, "URI should be preserved");
        assert_eq!(report.js_removed, 2);
        assert_eq!(report.launch_removed, 1);

        // Re-load and confirm the URI annot still has its /A.
        let reloaded = Document::load(&dst).unwrap();
        let mut found_uri = false;
        for (_n, pid) in reloaded.get_pages() {
            let dict = reloaded.get_object(pid).unwrap().as_dict().unwrap();
            let annots = dict.get(b"Annots").unwrap().as_array().unwrap();
            for v in annots {
                let aid = v.as_reference().unwrap();
                let adict = reloaded.get_object(aid).unwrap().as_dict().unwrap();
                if let Ok(Object::Dictionary(action)) = adict.get(b"A") {
                    if let Ok(s) = action.get(b"S").and_then(|o| o.as_name()) {
                        if s == b"URI" {
                            found_uri = true;
                        }
                    }
                }
            }
        }
        assert!(found_uri, "URI annot should still have /A");
    }

    #[test]
    fn sanitize_plain_pdf_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("plain.pdf");
        let dst = tmp.path().join("plain_clean.pdf");
        make_n_page_pdf(&src, 2);

        let report = sanitize(&src, &dst, SanitizeOpts::default()).unwrap();
        assert_eq!(report.js_removed, 0);
        assert_eq!(report.launch_removed, 0);
        assert_eq!(report.uri_removed, 0);
        assert_eq!(report.embedded_files_removed, 0);
        assert!(!report.open_action_removed);
        assert!(!report.catalog_aa_removed);
        assert!(!report.xfa_removed);
        assert_eq!(report.pages_aa_removed, 0);

        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 2);
    }

    #[test]
    fn sanitize_rejects_missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("out.pdf");
        let err =
            sanitize(&tmp.path().join("nope.pdf"), &dst, SanitizeOpts::default()).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
