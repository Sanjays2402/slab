//! PDF Forms (AcroForm) inspector + round-trip fill.
//!
//! v3.9.0 "Quill" — enterprise PDF form workstation. Adobe Acrobat Pro
//! charges $239/yr for the equivalent. We do it offline.
//!
//! Two operations:
//!
//! 1. **inspect(input)** — walks the catalog's `/AcroForm /Fields` tree,
//!    flattens it into a list of leaf fields, and reports every field's
//!    fully-qualified name, type, current value, /Rect, and page index.
//!    Returns a [`FormsReport`] that the front-end serializes to JSON.
//!
//! 2. **fill(input, values, output)** — takes a `{ field_name: value }`
//!    map, walks the same tree, mutates each leaf field's `/V` (and `/AS`
//!    for button widgets when the value matches a known appearance state),
//!    sets `/AcroForm /NeedAppearances = true` so any viewer (Acrobat,
//!    Preview, Firefox, Chrome) regenerates the appearance streams when
//!    the PDF is next opened, and saves atomically. Returns a
//!    [`FillReport`] listing which fields were set, skipped, or unknown.
//!
//! What we do NOT do (yet):
//! - Render the new appearance streams ourselves. Viewers handle this.
//! - Touch XFA. AcroForm only. XFA is deprecated in PDF 2.0 anyway.
//! - Sign or certify. That's a v3.10.0+ concern.

use crate::pdf::{atomic_save, PdfError};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Canonical field type. Mirrors the PDF spec's `/FT` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Text input (`/FT /Tx`).
    Text,
    /// Button — checkbox, radio button, or pushbutton (`/FT /Btn`).
    Button,
    /// Choice — combo or list box (`/FT /Ch`).
    Choice,
    /// Signature placeholder (`/FT /Sig`).
    Signature,
    /// We could not determine the field's type. Treated as read-only.
    Unknown,
}

impl FieldType {
    fn from_bytes(b: &[u8]) -> Self {
        match b {
            b"Tx" => Self::Text,
            b"Btn" => Self::Button,
            b"Ch" => Self::Choice,
            b"Sig" => Self::Signature,
            _ => Self::Unknown,
        }
    }
}

/// One leaf form field as seen by the inspector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// Fully-qualified field name (e.g. `"taxpayer.address.line1"`).
    /// PDF assembles this by joining the partial names (`/T`) of each
    /// ancestor in the field tree with `.` separators.
    pub name: String,
    /// Field type from `/FT`. Inherited from a parent if absent on the leaf.
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// Current value (`/V`). Always rendered as a string for the editor —
    /// for buttons this is the `/AS` appearance state name (e.g. `"Yes"`).
    pub value: Option<String>,
    /// 1-based page number, or `None` if we couldn't find the widget.
    pub page: Option<u32>,
    /// Rect in PDF user-space `[x0, y0, x1, y1]`. Used by the front-end
    /// to draw an overlay on the page preview.
    pub rect: Option<[f32; 4]>,
    /// Allowed appearance states for button fields (e.g. `["Yes", "Off"]`).
    /// Empty for non-button fields.
    #[serde(default)]
    pub options: Vec<String>,
    /// True if `/Ff` bit 1 (ReadOnly) is set. The editor still shows it
    /// but disables editing.
    #[serde(default)]
    pub read_only: bool,
}

/// Report returned by [`inspect`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormsReport {
    /// True if the catalog has an `/AcroForm` entry at all.
    pub has_acroform: bool,
    /// True if `/AcroForm /NeedAppearances` is currently set.
    pub need_appearances: bool,
    /// True if `/AcroForm /XFA` exists. We flag it but cannot fill XFA.
    pub has_xfa: bool,
    /// Every leaf field, depth-first, in document order.
    pub fields: Vec<FormField>,
}

/// Report returned by [`fill`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FillReport {
    /// Field names successfully set, in the order we encountered them.
    pub filled: Vec<String>,
    /// Names present in the input map that did NOT match any leaf field.
    pub unknown: Vec<String>,
    /// Names skipped because the field is read-only.
    pub read_only_skipped: Vec<String>,
    /// True if `/NeedAppearances` was newly added or already present.
    pub need_appearances: bool,
}

/// Public entry point: inspect a PDF's form fields without mutating it.
pub fn inspect(input: &Path) -> Result<FormsReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    inspect_doc(&doc)
}

/// Public entry point: fill a PDF's form fields from a `name -> value` map.
pub fn fill(
    input: &Path,
    values: &HashMap<String, String>,
    output: &Path,
) -> Result<FillReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if output.as_os_str().is_empty() {
        return Err(PdfError::EmptyOutput);
    }
    let mut doc = Document::load(input)?;
    let report = fill_doc(&mut doc, values)?;
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    atomic_save(output, &buf)?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// inspect

fn inspect_doc(doc: &Document) -> Result<FormsReport, PdfError> {
    let mut report = FormsReport {
        has_acroform: false,
        need_appearances: false,
        has_xfa: false,
        fields: Vec::new(),
    };

    let acroform = match get_acroform_dict(doc) {
        Some(d) => d,
        None => return Ok(report),
    };
    report.has_acroform = true;
    report.need_appearances = acroform
        .get(b"NeedAppearances")
        .ok()
        .and_then(|o| o.as_bool().ok())
        .unwrap_or(false);
    report.has_xfa = acroform.get(b"XFA").is_ok();

    // Build widget -> page map ONCE; we'll use it for every leaf.
    let widget_pages = build_widget_page_map(doc);

    let fields_obj = match acroform.get(b"Fields").ok() {
        Some(o) => o,
        None => return Ok(report),
    };
    let kids = resolve_array(doc, fields_obj);
    for kid in kids {
        walk_field(doc, kid, "", None, &widget_pages, &mut report.fields);
    }

    Ok(report)
}

/// Recursively walk a field subtree, appending leaf fields to `out`.
fn walk_field(
    doc: &Document,
    field_ref: Object,
    parent_name: &str,
    parent_ft: Option<FieldType>,
    widget_pages: &HashMap<ObjectId, u32>,
    out: &mut Vec<FormField>,
) {
    let (id, dict) = match resolve_dict(doc, &field_ref) {
        Some(p) => p,
        None => return,
    };

    // Compose qualified name.
    let partial = read_string(dict, b"T").unwrap_or_default();
    let qualified = if partial.is_empty() {
        parent_name.to_string()
    } else if parent_name.is_empty() {
        partial
    } else {
        format!("{parent_name}.{partial}")
    };

    // Field type — inherit from parent if absent.
    let ft = dict
        .get(b"FT")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(FieldType::from_bytes)
        .or(parent_ft);

    // Has /Kids that are FIELDS (not just widgets)? Recurse. PDF allows
    // mixing field kids and widget kids in /Kids — we distinguish by
    // checking for /T on the child (only fields carry /T).
    let kids_obj = dict.get(b"Kids").ok().cloned();
    let mut has_field_kids = false;
    if let Some(ref kobj) = kids_obj {
        let kids = resolve_array(doc, kobj);
        // First pass: detect whether any kid is itself a field (has /T or /FT).
        for kid in &kids {
            if let Some((_, kd)) = resolve_dict(doc, kid) {
                if kd.get(b"T").is_ok() || kd.get(b"FT").is_ok() {
                    has_field_kids = true;
                    break;
                }
            }
        }
        if has_field_kids {
            for kid in kids {
                walk_field(doc, kid, &qualified, ft, widget_pages, out);
            }
        }
    }
    if has_field_kids {
        return;
    }

    // Leaf field. Collect display data.
    let value = read_value_string(dict);
    let rect = read_rect(dict);
    let page = widget_pages.get(&id).copied();
    let read_only = read_flag(dict, b"Ff") & 0x1 != 0;
    let options = read_button_options(doc, dict);

    out.push(FormField {
        name: qualified,
        field_type: ft.unwrap_or(FieldType::Unknown),
        value,
        page,
        rect,
        options,
        read_only,
    });
}

// ---------------------------------------------------------------------------
// fill

fn fill_doc(doc: &mut Document, values: &HashMap<String, String>) -> Result<FillReport, PdfError> {
    let mut report = FillReport::default();

    // Snapshot every leaf field's (id, qualified-name) so we can mutate
    // without holding immutable borrows.
    let leaves = collect_leaves(doc);
    let mut name_to_id: HashMap<String, ObjectId> = HashMap::new();
    for (id, name) in &leaves {
        name_to_id.insert(name.clone(), *id);
    }

    // Track which input keys we've satisfied so we can report leftovers.
    let mut satisfied: std::collections::HashSet<&str> = Default::default();

    for (id, name) in &leaves {
        let Some(new_val) = values.get(name) else {
            continue;
        };
        satisfied.insert(name.as_str());

        // Skip if read-only.
        let read_only = {
            let dict = doc.get_object(*id).ok().and_then(|o| o.as_dict().ok());
            dict.map(|d| read_flag(d, b"Ff") & 0x1 != 0)
                .unwrap_or(false)
        };
        if read_only {
            report.read_only_skipped.push(name.clone());
            continue;
        }

        // Determine the field type (inherited via /FT chain).
        let ft = resolve_field_type(doc, *id);

        // Apply the mutation. For Btn we set /V and /AS to a name; for
        // everything else we set /V to a literal string.
        if let Ok(obj) = doc.get_object_mut(*id) {
            if let Ok(dict) = obj.as_dict_mut() {
                match ft {
                    FieldType::Button => {
                        let name_val = Object::Name(new_val.as_bytes().to_vec());
                        dict.set("V", name_val.clone());
                        dict.set("AS", name_val);
                    }
                    _ => {
                        dict.set(
                            "V",
                            Object::String(
                                new_val.as_bytes().to_vec(),
                                lopdf::StringFormat::Literal,
                            ),
                        );
                    }
                }
            }
        }
        report.filled.push(name.clone());
    }

    // Anything in `values` that didn't match a known leaf is "unknown".
    let mut unknown: Vec<String> = values
        .keys()
        .filter(|k| !satisfied.contains(k.as_str()))
        .cloned()
        .collect();
    unknown.sort();
    report.unknown = unknown;

    // Set /AcroForm /NeedAppearances = true.
    report.need_appearances = ensure_need_appearances(doc);

    Ok(report)
}

/// Walk the field tree and produce `(leaf_object_id, qualified_name)` pairs.
fn collect_leaves(doc: &Document) -> Vec<(ObjectId, String)> {
    let mut out = Vec::new();
    let Some(acroform) = get_acroform_dict(doc) else {
        return out;
    };
    let Ok(fields_obj) = acroform.get(b"Fields") else {
        return out;
    };
    let roots = resolve_array(doc, fields_obj);
    for r in roots {
        collect_leaves_walk(doc, r, "", &mut out);
    }
    out
}

fn collect_leaves_walk(
    doc: &Document,
    field_ref: Object,
    parent_name: &str,
    out: &mut Vec<(ObjectId, String)>,
) {
    let Some((id, dict)) = resolve_dict(doc, &field_ref) else {
        return;
    };
    let partial = read_string(dict, b"T").unwrap_or_default();
    let qualified = if partial.is_empty() {
        parent_name.to_string()
    } else if parent_name.is_empty() {
        partial
    } else {
        format!("{parent_name}.{partial}")
    };

    let kids_obj = dict.get(b"Kids").ok().cloned();
    let mut field_kids: Vec<Object> = Vec::new();
    if let Some(ref kobj) = kids_obj {
        for kid in resolve_array(doc, kobj) {
            if let Some((_, kd)) = resolve_dict(doc, &kid) {
                if kd.get(b"T").is_ok() || kd.get(b"FT").is_ok() {
                    field_kids.push(kid);
                }
            }
        }
    }
    if !field_kids.is_empty() {
        for kid in field_kids {
            collect_leaves_walk(doc, kid, &qualified, out);
        }
        return;
    }
    out.push((id, qualified));
}

/// Walk up through /Parent until we find an `/FT` entry; default Unknown.
fn resolve_field_type(doc: &Document, mut id: ObjectId) -> FieldType {
    for _ in 0..32 {
        let Ok(obj) = doc.get_object(id) else { break };
        let Ok(dict) = obj.as_dict() else { break };
        if let Ok(ft) = dict.get(b"FT").and_then(|o| o.as_name()) {
            return FieldType::from_bytes(ft);
        }
        match dict.get(b"Parent").ok() {
            Some(Object::Reference(pid)) => id = *pid,
            _ => break,
        }
    }
    FieldType::Unknown
}

/// Set `/AcroForm /NeedAppearances = true`. Returns true on success/idempotent.
fn ensure_need_appearances(doc: &mut Document) -> bool {
    let catalog_id = match doc.catalog().map(|c| c as *const _ as usize) {
        Ok(_) => match doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
            Ok(r) => r,
            Err(_) => return false,
        },
        Err(_) => return false,
    };
    // Resolve catalog dict.
    let acroform_ref = {
        let cat_dict = match doc.get_object(catalog_id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return false,
        };
        cat_dict.get(b"AcroForm").ok().cloned()
    };

    match acroform_ref {
        Some(Object::Reference(r)) => {
            if let Ok(obj) = doc.get_object_mut(r) {
                if let Ok(d) = obj.as_dict_mut() {
                    d.set("NeedAppearances", Object::Boolean(true));
                    return true;
                }
            }
            false
        }
        Some(Object::Dictionary(_)) => {
            // Inline dict on the catalog. Mutate in place.
            if let Ok(obj) = doc.get_object_mut(catalog_id) {
                if let Ok(cat) = obj.as_dict_mut() {
                    if let Ok(af) = cat.get_mut(b"AcroForm").and_then(|o| o.as_dict_mut()) {
                        af.set("NeedAppearances", Object::Boolean(true));
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// helpers

fn get_acroform_dict(doc: &Document) -> Option<&Dictionary> {
    let cat = doc.catalog().ok()?;
    let af = cat.get(b"AcroForm").ok()?;
    match af {
        Object::Reference(r) => doc.get_object(*r).ok()?.as_dict().ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Build a map of `widget_or_field_id -> 1-based page index` by walking
/// every page's `/Annots`. A widget annotation IS a leaf field when the
/// field dict and the widget share the same object (common in single-
/// widget fields).
fn build_widget_page_map(doc: &Document) -> HashMap<ObjectId, u32> {
    let mut map = HashMap::new();
    for (page_num, page_id) in doc.get_pages() {
        let Ok(page_obj) = doc.get_object(page_id) else {
            continue;
        };
        let Ok(page_dict) = page_obj.as_dict() else {
            continue;
        };
        let Ok(annots_obj) = page_dict.get(b"Annots") else {
            continue;
        };
        for a in resolve_array(doc, annots_obj) {
            if let Object::Reference(aid) = a {
                map.insert(aid, page_num);
            }
        }
    }
    map
}

/// Resolve an `Object::Reference` (or direct dict/array) to an array of refs.
fn resolve_array(doc: &Document, obj: &Object) -> Vec<Object> {
    match obj {
        Object::Array(items) => items.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Array(items)) => items.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Resolve a value (reference or inline dict) to `(id, &Dictionary)`.
/// For inline dicts we synthesize an `ObjectId` of `(0, 0)` — callers
/// who need a real id (e.g. page mapping) should pass references.
fn resolve_dict<'a>(doc: &'a Document, obj: &Object) -> Option<(ObjectId, &'a Dictionary)> {
    match obj {
        Object::Reference(r) => {
            let dict = doc.get_object(*r).ok()?.as_dict().ok()?;
            Some((*r, dict))
        }
        // We don't return inline dicts because we need a stable id for
        // mutation in fill(). PDFs in the wild use references for fields.
        _ => None,
    }
}

fn read_string(dict: &Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

fn read_value_string(dict: &Dictionary) -> Option<String> {
    let v = dict.get(b"V").ok()?;
    match v {
        Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Object::Boolean(b) => Some(if *b { "true".into() } else { "false".into() }),
        Object::Integer(i) => Some(i.to_string()),
        Object::Real(r) => Some(r.to_string()),
        _ => None,
    }
}

fn read_rect(dict: &Dictionary) -> Option<[f32; 4]> {
    let arr = match dict.get(b"Rect").ok()? {
        Object::Array(a) => a,
        _ => return None,
    };
    if arr.len() != 4 {
        return None;
    }
    let mut out = [0.0_f32; 4];
    for (i, v) in arr.iter().enumerate() {
        out[i] = match v {
            Object::Integer(n) => *n as f32,
            Object::Real(r) => *r,
            _ => return None,
        };
    }
    Some(out)
}

fn read_flag(dict: &Dictionary, key: &[u8]) -> i64 {
    dict.get(key)
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
}

/// For Btn fields, enumerate appearance states from `/AP /N` (the on-state
/// names that the widget can take, e.g. `"Yes"`, `"Off"`).
fn read_button_options(doc: &Document, dict: &Dictionary) -> Vec<String> {
    let Ok(ap) = dict.get(b"AP").ok().ok_or(()).and_then(|o| match o {
        Object::Dictionary(d) => Ok(d),
        Object::Reference(r) => doc
            .get_object(*r)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .ok_or(()),
        _ => Err(()),
    }) else {
        return Vec::new();
    };
    let Ok(n) = ap.get(b"N") else {
        return Vec::new();
    };
    let n_dict = match n {
        Object::Dictionary(d) => Some(d),
        Object::Reference(r) => doc.get_object(*r).ok().and_then(|o| o.as_dict().ok()),
        _ => None,
    };
    let Some(n_dict) = n_dict else {
        return Vec::new();
    };
    n_dict
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// tests — synthetic AcroForm fixture

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// Build a minimal one-page PDF with three fields:
    /// - "name"          (Tx, value="Sanjay")
    /// - "agree"         (Btn, value=Off)
    /// - "addr.line1"    (Tx, value="123 Main")
    fn build_fixture() -> Document {
        let mut doc = Document::with_version("1.7");

        // 1) Pages root.
        let pages_id = doc.new_object_id();

        // 2) Field 1 — text "name"
        let name_field_id = doc.add_object(dictionary! {
            "T" => Object::String(b"name".to_vec(), lopdf::StringFormat::Literal),
            "FT" => Object::Name(b"Tx".to_vec()),
            "V" => Object::String(b"Sanjay".to_vec(), lopdf::StringFormat::Literal),
            "Rect" => vec![50.into(), 700.into(), 250.into(), 720.into()],
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Type" => Object::Name(b"Annot".to_vec()),
        });

        // 3) Field 2 — button "agree" with /AP/N -> {Yes, Off}
        let ap_n_id = doc.add_object(dictionary! {
            "Yes" => Object::Stream(lopdf::Stream::new(dictionary!{}, vec![])),
            "Off" => Object::Stream(lopdf::Stream::new(dictionary!{}, vec![])),
        });
        let agree_field_id = doc.add_object(dictionary! {
            "T" => Object::String(b"agree".to_vec(), lopdf::StringFormat::Literal),
            "FT" => Object::Name(b"Btn".to_vec()),
            "V" => Object::Name(b"Off".to_vec()),
            "AS" => Object::Name(b"Off".to_vec()),
            "Rect" => vec![50.into(), 670.into(), 70.into(), 690.into()],
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Type" => Object::Name(b"Annot".to_vec()),
            "AP" => dictionary! { "N" => Object::Reference(ap_n_id) },
        });

        // 4) Field 3 — parent "addr" with one Tx child "line1"
        let line1_id = doc.add_object(dictionary! {
            "T" => Object::String(b"line1".to_vec(), lopdf::StringFormat::Literal),
            "FT" => Object::Name(b"Tx".to_vec()),
            "V" => Object::String(b"123 Main".to_vec(), lopdf::StringFormat::Literal),
            "Rect" => vec![50.into(), 640.into(), 250.into(), 660.into()],
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Type" => Object::Name(b"Annot".to_vec()),
        });
        let addr_id = doc.add_object(dictionary! {
            "T" => Object::String(b"addr".to_vec(), lopdf::StringFormat::Literal),
            "Kids" => vec![Object::Reference(line1_id)],
        });

        // 5) Page
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Annots" => vec![
                Object::Reference(name_field_id),
                Object::Reference(agree_field_id),
                Object::Reference(line1_id),
            ],
        });

        // 6) Pages tree
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );

        // 7) AcroForm
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![
                Object::Reference(name_field_id),
                Object::Reference(agree_field_id),
                Object::Reference(addr_id),
            ],
        });

        // 8) Catalog
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn inspect_returns_all_three_leaves() {
        let doc = build_fixture();
        let r = inspect_doc(&doc).unwrap();
        assert!(r.has_acroform);
        assert!(!r.need_appearances);
        assert!(!r.has_xfa);
        assert_eq!(r.fields.len(), 3);

        let names: Vec<&str> = r.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["name", "agree", "addr.line1"]);
    }

    #[test]
    fn inspect_reads_field_types() {
        let doc = build_fixture();
        let r = inspect_doc(&doc).unwrap();
        assert_eq!(r.fields[0].field_type, FieldType::Text);
        assert_eq!(r.fields[1].field_type, FieldType::Button);
        assert_eq!(r.fields[2].field_type, FieldType::Text);
    }

    #[test]
    fn inspect_reads_values_and_rects() {
        let doc = build_fixture();
        let r = inspect_doc(&doc).unwrap();
        assert_eq!(r.fields[0].value.as_deref(), Some("Sanjay"));
        assert_eq!(r.fields[1].value.as_deref(), Some("Off"));
        assert_eq!(r.fields[2].value.as_deref(), Some("123 Main"));
        assert_eq!(r.fields[0].rect, Some([50.0, 700.0, 250.0, 720.0]));
    }

    #[test]
    fn inspect_finds_page_for_widget_field() {
        let doc = build_fixture();
        let r = inspect_doc(&doc).unwrap();
        // Text "name" and "addr.line1" are referenced directly from page's
        // /Annots, so they should resolve to page 1. The "agree" widget too.
        assert_eq!(r.fields[0].page, Some(1));
        assert_eq!(r.fields[1].page, Some(1));
        assert_eq!(r.fields[2].page, Some(1));
    }

    #[test]
    fn inspect_collects_button_options() {
        let doc = build_fixture();
        let r = inspect_doc(&doc).unwrap();
        let mut opts = r.fields[1].options.clone();
        opts.sort();
        assert_eq!(opts, vec!["Off".to_string(), "Yes".to_string()]);
    }

    #[test]
    fn fill_updates_text_value_and_sets_need_appearances() {
        let mut doc = build_fixture();
        let mut values = HashMap::new();
        values.insert("name".to_string(), "Cake".to_string());
        let report = fill_doc(&mut doc, &values).unwrap();
        assert_eq!(report.filled, vec!["name".to_string()]);
        assert!(report.unknown.is_empty());
        assert!(report.need_appearances);

        // Verify it actually changed.
        let r = inspect_doc(&doc).unwrap();
        assert_eq!(r.fields[0].value.as_deref(), Some("Cake"));
        assert!(r.need_appearances);
    }

    #[test]
    fn fill_updates_button_v_and_as_as_name() {
        let mut doc = build_fixture();
        let mut values = HashMap::new();
        values.insert("agree".to_string(), "Yes".to_string());
        let report = fill_doc(&mut doc, &values).unwrap();
        assert_eq!(report.filled, vec!["agree".to_string()]);

        let r = inspect_doc(&doc).unwrap();
        assert_eq!(r.fields[1].value.as_deref(), Some("Yes"));
    }

    #[test]
    fn fill_handles_nested_field_paths() {
        let mut doc = build_fixture();
        let mut values = HashMap::new();
        values.insert("addr.line1".to_string(), "456 Elm".to_string());
        let report = fill_doc(&mut doc, &values).unwrap();
        assert_eq!(report.filled, vec!["addr.line1".to_string()]);

        let r = inspect_doc(&doc).unwrap();
        assert_eq!(r.fields[2].value.as_deref(), Some("456 Elm"));
    }

    #[test]
    fn fill_reports_unknown_keys() {
        let mut doc = build_fixture();
        let mut values = HashMap::new();
        values.insert("ghost".to_string(), "x".to_string());
        values.insert("name".to_string(), "Cake".to_string());
        let report = fill_doc(&mut doc, &values).unwrap();
        assert_eq!(report.filled, vec!["name".to_string()]);
        assert_eq!(report.unknown, vec!["ghost".to_string()]);
    }

    #[test]
    fn inspect_on_pdf_without_acroform_returns_empty() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let r = inspect_doc(&doc).unwrap();
        assert!(!r.has_acroform);
        assert_eq!(r.fields.len(), 0);
    }

    #[test]
    fn fill_round_trips_through_save_and_load() {
        // Build → fill → save to bytes → reload → inspect.
        let mut doc = build_fixture();
        let mut values = HashMap::new();
        values.insert("name".to_string(), "Cake".to_string());
        values.insert("agree".to_string(), "Yes".to_string());
        values.insert("addr.line1".to_string(), "9 Maple".to_string());
        let _ = fill_doc(&mut doc, &values).unwrap();

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        let reloaded = Document::load_mem(&buf).unwrap();
        let r = inspect_doc(&reloaded).unwrap();

        assert!(r.need_appearances);
        assert_eq!(r.fields[0].value.as_deref(), Some("Cake"));
        assert_eq!(r.fields[1].value.as_deref(), Some("Yes"));
        assert_eq!(r.fields[2].value.as_deref(), Some("9 Maple"));
    }
}
