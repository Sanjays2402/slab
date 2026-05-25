//! v3.26.0 "Quill Designer" — author AcroForm fields on flat PDFs.
//!
//! Companion to v3.9.0 [`forms`] (fill) and v3.25.0 [`forms_batch`] (mail-merge fill).
//! Replaces Adobe Acrobat Pro's "Prepare Form" tool ($239/yr) offline + free.
//!
//! Three primitives:
//! - [`add_fields`]   — install a new widget annotation and field entry.
//! - [`edit_fields`]  — mutate name/default/required/read-only/tooltip on existing fields.
//! - [`delete_fields`] — remove fields from both `/AcroForm /Fields` and page `/Annots`.
//!
//! Round-trips through [`forms::inspect`] so the resulting PDF can be filled
//! immediately by [`forms::fill`] or [`forms_batch::run_batch`].

use crate::pdf::{atomic_save, PdfError};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// DTOs

/// The kind of widget being authored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DraftKind {
    Text {
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        max_len: Option<u32>,
        #[serde(default)]
        default: Option<String>,
    },
    Checkbox {
        #[serde(default)]
        default_checked: bool,
    },
    Radio {
        /// Radio button export value (the "on" name). Group is the field
        /// name (each [`FieldDraft`] in a group shares the same `name`).
        value: String,
        #[serde(default)]
        default_selected: bool,
    },
    Dropdown {
        #[serde(default)]
        options: Vec<String>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        editable: bool,
    },
    Signature,
}

/// A single field the user is drafting on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDraft {
    /// Fully-qualified field name. Radio members share this; values differ.
    pub name: String,
    #[serde(flatten)]
    pub kind: DraftKind,
    /// 1-based page index.
    pub page: u32,
    /// PDF user-space rect [x0, y0, x1, y1].
    pub rect: [f32; 4],
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub tooltip: Option<String>,
}

/// In-place mutation of an existing field, matched by `current_name`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldEdit {
    pub current_name: String,
    #[serde(default)]
    pub new_name: Option<String>,
    #[serde(default)]
    pub new_default: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub read_only: Option<bool>,
    #[serde(default)]
    pub tooltip: Option<String>,
}

/// Per-operation report; mirrors the shape of [`FillReport`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesignReport {
    pub added: Vec<String>,
    pub edited: Vec<String>,
    pub deleted: Vec<String>,
    pub unknown: Vec<String>,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry points

/// Install new fields. Idempotent for `/AcroForm` creation; duplicates rejected.
pub fn add_fields(
    input: &Path,
    drafts: &[FieldDraft],
    output: &Path,
) -> Result<DesignReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if output.as_os_str().is_empty() {
        return Err(PdfError::EmptyOutput);
    }
    let mut doc = Document::load(input)?;
    let report = add_fields_doc(&mut doc, drafts)?;
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    atomic_save(output, &buf)?;
    Ok(report)
}

/// Edit existing fields in place.
pub fn edit_fields(
    input: &Path,
    edits: &[FieldEdit],
    output: &Path,
) -> Result<DesignReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if output.as_os_str().is_empty() {
        return Err(PdfError::EmptyOutput);
    }
    let mut doc = Document::load(input)?;
    let report = edit_fields_doc(&mut doc, edits)?;
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    atomic_save(output, &buf)?;
    Ok(report)
}

/// Delete fields from `/AcroForm /Fields` and the page's `/Annots`.
pub fn delete_fields(
    input: &Path,
    names: &[String],
    output: &Path,
) -> Result<DesignReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if output.as_os_str().is_empty() {
        return Err(PdfError::EmptyOutput);
    }
    let mut doc = Document::load(input)?;
    let report = delete_fields_doc(&mut doc, names)?;
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    atomic_save(output, &buf)?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// add

fn add_fields_doc(doc: &mut Document, drafts: &[FieldDraft]) -> Result<DesignReport, PdfError> {
    let mut report = DesignReport::default();
    if drafts.is_empty() {
        return Ok(report);
    }
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let acroform_id = ensure_acroform(doc)?;
    let mut existing: HashSet<String> = existing_field_names(doc);

    // Group radios by name → emit one parent /Kids field per group.
    let (radio_groups, singles) = split_radios(drafts);

    for d in singles {
        if existing.contains(&d.name) {
            report
                .errors
                .push(format!("{}: duplicate field name", d.name));
            continue;
        }
        match install_single(doc, &pages, acroform_id, &d) {
            Ok(()) => {
                existing.insert(d.name.clone());
                report.added.push(d.name.clone());
            }
            Err(e) => report.errors.push(format!("{}: {}", d.name, e)),
        }
    }
    for (name, members) in radio_groups {
        if existing.contains(&name) {
            report.errors.push(format!("{name}: duplicate field name"));
            continue;
        }
        match install_radio_group(doc, &pages, acroform_id, &name, &members) {
            Ok(()) => {
                existing.insert(name.clone());
                report.added.push(name);
            }
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }

    Ok(report)
}

fn split_radios(drafts: &[FieldDraft]) -> (Vec<(String, Vec<FieldDraft>)>, Vec<FieldDraft>) {
    let mut groups: Vec<(String, Vec<FieldDraft>)> = Vec::new();
    let mut singles: Vec<FieldDraft> = Vec::new();
    for d in drafts {
        if matches!(d.kind, DraftKind::Radio { .. }) {
            if let Some((_, members)) = groups.iter_mut().find(|(n, _)| n == &d.name) {
                members.push(d.clone());
            } else {
                groups.push((d.name.clone(), vec![d.clone()]));
            }
        } else {
            singles.push(d.clone());
        }
    }
    (groups, singles)
}

fn ensure_acroform(doc: &mut Document) -> Result<ObjectId, PdfError> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|_| PdfError::Other("missing /Root".into()))?;

    // Look up existing acroform reference (cloned to drop borrow).
    let acroform_ref_opt = {
        let cat = doc
            .get_object(catalog_id)
            .and_then(|o| o.as_dict())
            .map_err(|_| PdfError::Other("catalog not a dict".into()))?;
        cat.get(b"AcroForm").ok().cloned()
    };

    match acroform_ref_opt {
        Some(Object::Reference(r)) => Ok(r),
        Some(Object::Dictionary(d)) => {
            // Move inline dict to its own object so we can mutate by id.
            let id = doc.add_object(Object::Dictionary(d));
            patch_catalog_acroform(doc, catalog_id, id)?;
            Ok(id)
        }
        _ => {
            let af = dictionary! {
                "Fields" => Object::Array(Vec::new()),
                "NeedAppearances" => Object::Boolean(true),
            };
            let id = doc.add_object(Object::Dictionary(af));
            patch_catalog_acroform(doc, catalog_id, id)?;
            Ok(id)
        }
    }
}

fn patch_catalog_acroform(
    doc: &mut Document,
    catalog_id: ObjectId,
    acroform_id: ObjectId,
) -> Result<(), PdfError> {
    let cat = doc
        .get_object_mut(catalog_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|_| PdfError::Other("cannot mutate catalog".into()))?;
    cat.set("AcroForm", Object::Reference(acroform_id));
    Ok(())
}

fn existing_field_names(doc: &Document) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(af) = get_acroform_dict_ref(doc) else {
        return out;
    };
    let Ok(fields_obj) = doc.get_object(af).and_then(|o| o.as_dict()) else {
        return out;
    };
    let Ok(fields) = fields_obj.get(b"Fields") else {
        return out;
    };
    let refs = match fields {
        Object::Array(items) => items.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Array(items)) => items.clone(),
            _ => return out,
        },
        _ => return out,
    };
    for r in refs {
        if let Object::Reference(rid) = r {
            if let Ok(d) = doc.get_object(rid).and_then(|o| o.as_dict()) {
                if let Ok(Object::String(name, _)) = d.get(b"T") {
                    out.insert(String::from_utf8_lossy(name).into_owned());
                }
            }
        }
    }
    out
}

fn get_acroform_dict_ref(doc: &Document) -> Option<ObjectId> {
    let catalog_id = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let cat = doc.get_object(catalog_id).ok()?.as_dict().ok()?;
    match cat.get(b"AcroForm").ok()? {
        Object::Reference(r) => Some(*r),
        _ => None,
    }
}

fn install_single(
    doc: &mut Document,
    pages: &[(u32, ObjectId)],
    acroform_id: ObjectId,
    d: &FieldDraft,
) -> Result<(), PdfError> {
    let page_id = page_for(pages, d.page)?;
    let merged = build_field_dict(d, page_id, /*as_kid_of_parent=*/ false, None);
    let field_id = doc.add_object(Object::Dictionary(merged));
    push_annot(doc, page_id, field_id)?;
    push_acroform_field(doc, acroform_id, field_id)
}

fn install_radio_group(
    doc: &mut Document,
    pages: &[(u32, ObjectId)],
    acroform_id: ObjectId,
    name: &str,
    members: &[FieldDraft],
) -> Result<(), PdfError> {
    // Parent field: /FT Btn, /Ff Radio+NoToggleToOff, /T <name>, /Kids [...]
    let mut ff: i64 = (1 << 15) | (1 << 14); // Radio + NoToggleToOff
                                             // Inherit common flags from first member.
    if let Some(first) = members.first() {
        if first.required {
            ff |= 1 << 1;
        }
        if first.read_only {
            ff |= 1 << 0;
        }
    }
    let parent = dictionary! {
        "FT" => Object::Name(b"Btn".to_vec()),
        "T" => Object::string_literal(name.to_string()),
        "Ff" => Object::Integer(ff),
        "Kids" => Object::Array(Vec::new()),
    };
    let parent_id = doc.add_object(Object::Dictionary(parent));

    // Default value: first member where default_selected=true; else "Off".
    let default_value = members
        .iter()
        .find_map(|m| match &m.kind {
            DraftKind::Radio {
                value,
                default_selected,
            } if *default_selected => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Off".into());

    if let Ok(p) = doc.get_object_mut(parent_id).and_then(|o| o.as_dict_mut()) {
        p.set("V", Object::Name(default_value.as_bytes().to_vec()));
        p.set("DV", Object::Name(default_value.as_bytes().to_vec()));
    }

    let mut kid_refs: Vec<Object> = Vec::with_capacity(members.len());
    for m in members {
        let page_id = page_for(pages, m.page)?;
        let kid = build_field_dict(m, page_id, /*as_kid_of_parent=*/ true, Some(parent_id));
        let kid_id = doc.add_object(Object::Dictionary(kid));
        push_annot(doc, page_id, kid_id)?;
        kid_refs.push(Object::Reference(kid_id));
    }
    if let Ok(p) = doc.get_object_mut(parent_id).and_then(|o| o.as_dict_mut()) {
        p.set("Kids", Object::Array(kid_refs));
    }

    push_acroform_field(doc, acroform_id, parent_id)
}

fn page_for(pages: &[(u32, ObjectId)], page: u32) -> Result<ObjectId, PdfError> {
    if page == 0 {
        return Err(PdfError::Other("page index 0 is invalid (1-based)".into()));
    }
    pages
        .iter()
        .find(|(n, _)| *n == page)
        .map(|(_, id)| *id)
        .ok_or_else(|| PdfError::Other(format!("page {page} out of range")))
}

fn push_annot(doc: &mut Document, page_id: ObjectId, annot_id: ObjectId) -> Result<(), PdfError> {
    // First, classify what /Annots currently holds without keeping a mut borrow.
    enum AnnotKind {
        InlineArray,
        RefToArray(ObjectId),
        MissingOrOther,
    }
    let kind = {
        let page = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|_| PdfError::Other("page dict read failed".into()))?;
        match page.get(b"Annots") {
            Ok(Object::Array(_)) => AnnotKind::InlineArray,
            Ok(Object::Reference(r)) => AnnotKind::RefToArray(*r),
            _ => AnnotKind::MissingOrOther,
        }
    };
    match kind {
        AnnotKind::InlineArray => {
            let page = doc
                .get_object_mut(page_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|_| PdfError::Other("page dict mut failed".into()))?;
            if let Ok(Object::Array(arr)) = page.get_mut(b"Annots") {
                arr.push(Object::Reference(annot_id));
            }
        }
        AnnotKind::RefToArray(r) => {
            if let Ok(Object::Array(mut arr)) = doc.get_object(r).cloned() {
                arr.push(Object::Reference(annot_id));
                doc.objects.insert(r, Object::Array(arr));
            } else {
                let page = doc
                    .get_object_mut(page_id)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|_| PdfError::Other("page dict mut failed".into()))?;
                page.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
            }
        }
        AnnotKind::MissingOrOther => {
            let page = doc
                .get_object_mut(page_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|_| PdfError::Other("page dict mut failed".into()))?;
            page.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
        }
    }
    Ok(())
}

fn push_acroform_field(
    doc: &mut Document,
    acroform_id: ObjectId,
    field_id: ObjectId,
) -> Result<(), PdfError> {
    let af = doc
        .get_object_mut(acroform_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|_| PdfError::Other("acroform dict mut failed".into()))?;
    match af.get_mut(b"Fields") {
        Ok(Object::Array(arr)) => {
            arr.push(Object::Reference(field_id));
        }
        _ => {
            af.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
        }
    }
    // Ensure NeedAppearances stays on so viewers regenerate appearances.
    af.set("NeedAppearances", Object::Boolean(true));
    Ok(())
}

fn build_field_dict(
    d: &FieldDraft,
    page_id: ObjectId,
    as_kid_of_parent: bool,
    parent_id: Option<ObjectId>,
) -> Dictionary {
    let mut dict = Dictionary::new();
    // Widget annotation fields (merged on leaf for single-widget fields).
    dict.set("Type", Object::Name(b"Annot".to_vec()));
    dict.set("Subtype", Object::Name(b"Widget".to_vec()));
    dict.set("F", Object::Integer(4)); // Print
    dict.set(
        "Rect",
        Object::Array(d.rect.iter().map(|f| Object::Real(*f)).collect()),
    );
    dict.set("P", Object::Reference(page_id));
    dict.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(1),
        ]),
    );

    if let Some(pid) = parent_id {
        dict.set("Parent", Object::Reference(pid));
    }

    let mut ff: i64 = 0;
    if d.required {
        ff |= 1 << 1;
    }
    if d.read_only {
        ff |= 1 << 0;
    }

    match &d.kind {
        DraftKind::Text {
            multiline,
            max_len,
            default,
        } => {
            dict.set("FT", Object::Name(b"Tx".to_vec()));
            dict.set("T", Object::string_literal(d.name.clone()));
            if *multiline {
                ff |= 1 << 12;
            }
            if let Some(m) = max_len {
                dict.set("MaxLen", Object::Integer(*m as i64));
            }
            if let Some(v) = default {
                dict.set("V", Object::string_literal(v.clone()));
                dict.set("DV", Object::string_literal(v.clone()));
            }
        }
        DraftKind::Checkbox { default_checked } => {
            dict.set("FT", Object::Name(b"Btn".to_vec()));
            dict.set("T", Object::string_literal(d.name.clone()));
            let state = if *default_checked { "Yes" } else { "Off" };
            dict.set("V", Object::Name(state.as_bytes().to_vec()));
            dict.set("DV", Object::Name(state.as_bytes().to_vec()));
            dict.set("AS", Object::Name(state.as_bytes().to_vec()));
        }
        DraftKind::Radio {
            value,
            default_selected,
        } => {
            // Radio kids carry no /FT (inherited from parent); /T omitted (parent owns name).
            // The on-state name is /AS. The kid's value-name appearance dict lives under /MK,
            // but lopdf viewers render unhinted radios fine without a custom /AP for simple use.
            let on_state = value.as_str();
            let state = if *default_selected { on_state } else { "Off" };
            dict.set("AS", Object::Name(state.as_bytes().to_vec()));
            // For non-merged kids we don't override /FT or /T.
            // For the unusual case of as_kid_of_parent=false (shouldn't happen for radio),
            // fall back to making it a single-widget radio:
            if !as_kid_of_parent {
                dict.set("FT", Object::Name(b"Btn".to_vec()));
                dict.set("T", Object::string_literal(d.name.clone()));
                ff |= 1 << 15;
                ff |= 1 << 14;
            }
        }
        DraftKind::Dropdown {
            options,
            default,
            editable,
        } => {
            dict.set("FT", Object::Name(b"Ch".to_vec()));
            dict.set("T", Object::string_literal(d.name.clone()));
            ff |= 1 << 17; // Combo
            if *editable {
                ff |= 1 << 18; // Edit
            }
            let opts: Vec<Object> = options
                .iter()
                .map(|o| Object::string_literal(o.clone()))
                .collect();
            dict.set("Opt", Object::Array(opts));
            if let Some(v) = default {
                dict.set("V", Object::string_literal(v.clone()));
                dict.set("DV", Object::string_literal(v.clone()));
            }
        }
        DraftKind::Signature => {
            dict.set("FT", Object::Name(b"Sig".to_vec()));
            dict.set("T", Object::string_literal(d.name.clone()));
        }
    }

    if ff != 0 {
        dict.set("Ff", Object::Integer(ff));
    }
    if let Some(t) = &d.tooltip {
        dict.set("TU", Object::string_literal(t.clone()));
    }
    dict
}

// ---------------------------------------------------------------------------
// edit

fn edit_fields_doc(doc: &mut Document, edits: &[FieldEdit]) -> Result<DesignReport, PdfError> {
    let mut report = DesignReport::default();
    if edits.is_empty() {
        return Ok(report);
    }

    // Build name -> field-id index by walking /AcroForm /Fields one level deep.
    let index = build_top_level_field_index(doc);

    for e in edits {
        let Some(&fid) = index.get(&e.current_name) else {
            report.unknown.push(e.current_name.clone());
            continue;
        };
        if let Err(err) = apply_edit(doc, fid, e) {
            report.errors.push(format!("{}: {}", e.current_name, err));
            continue;
        }
        report.edited.push(e.current_name.clone());
    }

    Ok(report)
}

fn build_top_level_field_index(doc: &Document) -> std::collections::HashMap<String, ObjectId> {
    let mut idx = std::collections::HashMap::new();
    let Some(af) = get_acroform_dict_ref(doc) else {
        return idx;
    };
    let Ok(af_dict) = doc.get_object(af).and_then(|o| o.as_dict()) else {
        return idx;
    };
    let Ok(fields) = af_dict.get(b"Fields") else {
        return idx;
    };
    let refs = match fields {
        Object::Array(items) => items.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Array(items)) => items.clone(),
            _ => return idx,
        },
        _ => return idx,
    };
    for r in refs {
        if let Object::Reference(rid) = r {
            if let Ok(d) = doc.get_object(rid).and_then(|o| o.as_dict()) {
                if let Ok(Object::String(name, _)) = d.get(b"T") {
                    idx.insert(String::from_utf8_lossy(name).into_owned(), rid);
                }
            }
        }
    }
    idx
}

fn apply_edit(doc: &mut Document, fid: ObjectId, e: &FieldEdit) -> Result<(), PdfError> {
    let dict = doc
        .get_object_mut(fid)
        .and_then(|o| o.as_dict_mut())
        .map_err(|_| PdfError::Other("field dict mut failed".into()))?;
    if let Some(new_name) = &e.new_name {
        dict.set("T", Object::string_literal(new_name.clone()));
    }
    if let Some(new_default) = &e.new_default {
        dict.set("V", Object::string_literal(new_default.clone()));
        dict.set("DV", Object::string_literal(new_default.clone()));
    }
    if e.required.is_some() || e.read_only.is_some() {
        let mut ff: i64 = dict
            .get(b"Ff")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        if let Some(req) = e.required {
            if req {
                ff |= 1 << 1;
            } else {
                ff &= !(1 << 1);
            }
        }
        if let Some(ro) = e.read_only {
            if ro {
                ff |= 1 << 0;
            } else {
                ff &= !(1 << 0);
            }
        }
        dict.set("Ff", Object::Integer(ff));
    }
    if let Some(tip) = &e.tooltip {
        dict.set("TU", Object::string_literal(tip.clone()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// delete

fn delete_fields_doc(doc: &mut Document, names: &[String]) -> Result<DesignReport, PdfError> {
    let mut report = DesignReport::default();
    if names.is_empty() {
        return Ok(report);
    }
    let index = build_top_level_field_index(doc);
    let mut ids_to_remove: Vec<(String, ObjectId)> = Vec::new();
    for n in names {
        match index.get(n) {
            Some(id) => ids_to_remove.push((n.clone(), *id)),
            None => report.unknown.push(n.clone()),
        }
    }
    if ids_to_remove.is_empty() {
        return Ok(report);
    }
    let id_set: HashSet<ObjectId> = ids_to_remove.iter().map(|(_, id)| *id).collect();

    // 1. Strip from /AcroForm /Fields.
    if let Some(af) = get_acroform_dict_ref(doc) {
        if let Ok(d) = doc.get_object_mut(af).and_then(|o| o.as_dict_mut()) {
            if let Ok(Object::Array(arr)) = d.get_mut(b"Fields") {
                arr.retain(|o| match o {
                    Object::Reference(r) => !id_set.contains(r),
                    _ => true,
                });
            }
        }
    }

    // 2. Strip from every page's /Annots. Also include any /Kids of the deleted
    //    field (radio kid widgets) — gather them first.
    let mut widget_kids: HashSet<ObjectId> = HashSet::new();
    for (_, fid) in &ids_to_remove {
        if let Ok(d) = doc.get_object(*fid).and_then(|o| o.as_dict()) {
            if let Ok(Object::Array(kids)) = d.get(b"Kids") {
                for k in kids {
                    if let Object::Reference(r) = k {
                        widget_kids.insert(*r);
                    }
                }
            }
        }
    }
    let strip: HashSet<ObjectId> = id_set.union(&widget_kids).copied().collect();

    let page_ids: Vec<ObjectId> = doc.page_iter().collect();
    for pid in page_ids {
        // First try mutating in place (array on page dict).
        let inline_done = {
            let Ok(page) = doc.get_object_mut(pid).and_then(|o| o.as_dict_mut()) else {
                continue;
            };
            if let Ok(Object::Array(arr)) = page.get_mut(b"Annots") {
                arr.retain(|o| match o {
                    Object::Reference(r) => !strip.contains(r),
                    _ => true,
                });
                true
            } else {
                false
            }
        };
        if inline_done {
            continue;
        }
        // Otherwise it might be a reference to an array object.
        let annots_ref = {
            let Ok(page) = doc.get_object(pid).and_then(|o| o.as_dict()) else {
                continue;
            };
            match page.get(b"Annots") {
                Ok(Object::Reference(r)) => Some(*r),
                _ => None,
            }
        };
        if let Some(r) = annots_ref {
            if let Ok(Object::Array(mut arr)) = doc.get_object(r).cloned() {
                arr.retain(|o| match o {
                    Object::Reference(r2) => !strip.contains(r2),
                    _ => true,
                });
                doc.objects.insert(r, Object::Array(arr));
            }
        }
    }

    for (n, _) in ids_to_remove {
        report.deleted.push(n);
    }
    Ok(report)
}

// ---------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::forms;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use tempfile::tempdir;

    fn draft_text(name: &str, default: Option<&str>) -> FieldDraft {
        FieldDraft {
            name: name.into(),
            kind: DraftKind::Text {
                multiline: false,
                max_len: Some(64),
                default: default.map(|s| s.into()),
            },
            page: 1,
            rect: [72.0, 700.0, 300.0, 720.0],
            required: true,
            read_only: false,
            tooltip: Some("Test tip".into()),
        }
    }

    #[test]
    fn field_draft_text_default_serializes() {
        let d = draft_text("applicant.email", Some("user@example.com"));
        let j = serde_json::to_string(&d).unwrap();
        assert!(j.contains("\"kind\":\"text\""));
        assert!(j.contains("\"max_len\":64"));
        assert!(j.contains("\"required\":true"));
    }

    #[test]
    fn add_single_text_field_round_trips() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);

        let drafts = vec![draft_text("applicant.name", Some("Jane Doe"))];
        let report = add_fields(&input, &drafts, &output).unwrap();
        assert_eq!(report.added, vec!["applicant.name"]);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        let inspected = forms::inspect(&output).unwrap();
        assert!(inspected.has_acroform);
        assert_eq!(inspected.fields.len(), 1);
        assert_eq!(inspected.fields[0].name, "applicant.name");
        assert_eq!(inspected.fields[0].value.as_deref(), Some("Jane Doe"));
        assert!(inspected.fields[0].rect.is_some());
        assert_eq!(inspected.fields[0].page, Some(1));
    }

    #[test]
    fn add_checkbox_round_trips() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);

        let d = FieldDraft {
            name: "agree".into(),
            kind: DraftKind::Checkbox {
                default_checked: true,
            },
            page: 1,
            rect: [72.0, 600.0, 92.0, 620.0],
            required: false,
            read_only: false,
            tooltip: None,
        };
        let report = add_fields(&input, &[d], &output).unwrap();
        assert_eq!(report.added, vec!["agree"]);
        let r = forms::inspect(&output).unwrap();
        assert_eq!(r.fields.len(), 1);
        assert_eq!(r.fields[0].name, "agree");
        assert_eq!(r.fields[0].value.as_deref(), Some("Yes"));
    }

    #[test]
    fn add_dropdown_round_trips() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);

        let d = FieldDraft {
            name: "country".into(),
            kind: DraftKind::Dropdown {
                options: vec!["US".into(), "CA".into(), "MX".into()],
                default: Some("US".into()),
                editable: false,
            },
            page: 1,
            rect: [72.0, 500.0, 220.0, 520.0],
            required: false,
            read_only: false,
            tooltip: None,
        };
        let report = add_fields(&input, &[d], &output).unwrap();
        assert_eq!(report.added, vec!["country"]);
        let r = forms::inspect(&output).unwrap();
        assert_eq!(r.fields.len(), 1);
        assert_eq!(r.fields[0].name, "country");
        assert_eq!(r.fields[0].value.as_deref(), Some("US"));
    }

    #[test]
    fn add_signature_placeholder() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);
        let d = FieldDraft {
            name: "sig".into(),
            kind: DraftKind::Signature,
            page: 1,
            rect: [72.0, 400.0, 300.0, 440.0],
            required: false,
            read_only: false,
            tooltip: None,
        };
        let report = add_fields(&input, &[d], &output).unwrap();
        assert_eq!(report.added, vec!["sig"]);
        let r = forms::inspect(&output).unwrap();
        assert_eq!(r.fields.len(), 1);
        assert_eq!(r.fields[0].name, "sig");
    }

    #[test]
    fn add_radio_group_three_members() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);

        let mk = |value: &str, y: f32, sel: bool| FieldDraft {
            name: "color".into(),
            kind: DraftKind::Radio {
                value: value.into(),
                default_selected: sel,
            },
            page: 1,
            rect: [72.0, y, 92.0, y + 20.0],
            required: false,
            read_only: false,
            tooltip: None,
        };
        let drafts = vec![
            mk("Red", 700.0, false),
            mk("Green", 670.0, true),
            mk("Blue", 640.0, false),
        ];
        let report = add_fields(&input, &drafts, &output).unwrap();
        assert_eq!(report.added, vec!["color"]);
        let r = forms::inspect(&output).unwrap();
        // The parent radio field appears as a single leaf with /V = "Green".
        let radio = r.fields.iter().find(|f| f.name == "color").unwrap();
        assert_eq!(radio.value.as_deref(), Some("Green"));
    }

    #[test]
    fn page_out_of_range_returns_error_in_report() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);
        let mut d = draft_text("ghost", None);
        d.page = 99;
        let report = add_fields(&input, &[d], &output).unwrap();
        assert!(report.added.is_empty());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("out of range"));
    }

    #[test]
    fn duplicate_name_collected_in_report_errors() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);

        let report = add_fields(
            &input,
            &[draft_text("dup", None), draft_text("dup", None)],
            &output,
        )
        .unwrap();
        assert_eq!(report.added, vec!["dup"]);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("duplicate"));
    }

    #[test]
    fn edit_field_updates_default_and_required() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let stage1 = dir.path().join("s1.pdf");
        let stage2 = dir.path().join("s2.pdf");
        make_n_page_pdf(&input, 1);
        add_fields(&input, &[draft_text("email", Some("a@b.co"))], &stage1).unwrap();

        let edit = FieldEdit {
            current_name: "email".into(),
            new_default: Some("new@x.io".into()),
            required: Some(false),
            ..Default::default()
        };
        let report = edit_fields(&stage1, &[edit], &stage2).unwrap();
        assert_eq!(report.edited, vec!["email"]);
        let r = forms::inspect(&stage2).unwrap();
        assert_eq!(r.fields[0].value.as_deref(), Some("new@x.io"));
    }

    #[test]
    fn edit_unknown_field_reported() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let s1 = dir.path().join("s1.pdf");
        let s2 = dir.path().join("s2.pdf");
        make_n_page_pdf(&input, 1);
        add_fields(&input, &[draft_text("real", None)], &s1).unwrap();
        let report = edit_fields(
            &s1,
            &[FieldEdit {
                current_name: "nope".into(),
                new_default: Some("x".into()),
                ..Default::default()
            }],
            &s2,
        )
        .unwrap();
        assert!(report.edited.is_empty());
        assert_eq!(report.unknown, vec!["nope"]);
    }

    #[test]
    fn delete_field_removes_from_acroform_and_annots() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let s1 = dir.path().join("s1.pdf");
        let s2 = dir.path().join("s2.pdf");
        make_n_page_pdf(&input, 1);
        add_fields(
            &input,
            &[draft_text("keep", None), draft_text("drop", None)],
            &s1,
        )
        .unwrap();

        let report = delete_fields(&s1, &["drop".into()], &s2).unwrap();
        assert_eq!(report.deleted, vec!["drop"]);
        let r = forms::inspect(&s2).unwrap();
        assert_eq!(r.fields.len(), 1);
        assert_eq!(r.fields[0].name, "keep");
    }

    #[test]
    fn delete_unknown_reported() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let s1 = dir.path().join("s1.pdf");
        let s2 = dir.path().join("s2.pdf");
        make_n_page_pdf(&input, 1);
        add_fields(&input, &[draft_text("real", None)], &s1).unwrap();
        let report = delete_fields(&s1, &["ghost".into()], &s2).unwrap();
        assert_eq!(report.unknown, vec!["ghost"]);
        assert!(report.deleted.is_empty());
    }

    #[test]
    fn empty_input_path_errors() {
        let r = add_fields(
            Path::new("/nope/does/not/exist.pdf"),
            &[draft_text("x", None)],
            Path::new("/tmp/out.pdf"),
        );
        assert!(matches!(r, Err(PdfError::InputMissing(_))));
    }

    #[test]
    fn empty_drafts_returns_empty_report() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        make_n_page_pdf(&input, 1);
        let report = add_fields(&input, &[], &output).unwrap();
        assert!(report.added.is_empty());
        assert!(report.errors.is_empty());
    }

    #[test]
    fn add_fields_then_fill_works_end_to_end() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let designed = dir.path().join("designed.pdf");
        let filled = dir.path().join("filled.pdf");
        make_n_page_pdf(&input, 1);
        add_fields(
            &input,
            &[
                draft_text("first_name", None),
                draft_text("last_name", None),
            ],
            &designed,
        )
        .unwrap();

        let mut values = std::collections::HashMap::new();
        values.insert("first_name".into(), "Ada".into());
        values.insert("last_name".into(), "Lovelace".into());
        let fill_report = forms::fill(&designed, &values, &filled).unwrap();
        assert_eq!(fill_report.filled.len(), 2);
        assert!(fill_report.unknown.is_empty());

        let r = forms::inspect(&filled).unwrap();
        let names: Vec<&str> = r.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"first_name"));
        assert!(names.contains(&"last_name"));
        for f in &r.fields {
            match f.name.as_str() {
                "first_name" => assert_eq!(f.value.as_deref(), Some("Ada")),
                "last_name" => assert_eq!(f.value.as_deref(), Some("Lovelace")),
                _ => {}
            }
        }
    }
}
