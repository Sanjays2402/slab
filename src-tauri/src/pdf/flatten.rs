// Flatten interactive annotations (and AcroForm fields) into the page
// content stream.
//
// PDF spec §12.5 / §12.7: every page may carry an `/Annots` array of
// annotation dictionaries. When an annotation has a `/AP /N` (normal
// appearance) entry pointing to a Form XObject, the visual representation
// of the annotation lives in that Form. Flattening = drawing those Forms
// directly into the page's content stream and then deleting the
// annotations, so the resulting PDF looks identical but has no editable
// fields and no live annotations.
//
// This is what pdftk's `flatten` and Acrobat's "Flatten Form Fields" do.
//
// What we DON'T do (out of scope for v0.9.0):
//   * Bake widget fields that have no `/AP` (i.e. text input boxes that
//     never had their appearance generated). For those, viewers compute
//     the appearance on-the-fly from `/DA`/`/V`. A future revision could
//     synthesize the appearance, but it's a lot of extra code.
//   * Annotations whose `/AP /N` value is a state-dictionary (e.g.
//     checkboxes with /Yes and /Off). We pick the entry matching `/AS`
//     when present, else the first one; if neither yields a stream we
//     skip that annotation (but still delete it from /Annots).
//
// Coordinates: each annot has a `/Rect [llx lly urx ury]` in PDF user
// space. The Form XObject has a `/BBox [bx0 by0 bx1 by1]` in its own
// space (plus an optional `/Matrix` which the consumer of the Do
// operator already honors). We need a CTM that maps the BBox's
// bottom-left corner to the Rect's bottom-left corner and scales BBox
// width/height to Rect width/height. That CTM is:
//
//     [ sx  0  0  sy  tx  ty ]
//
//   where sx = rect_w / bbox_w, sy = rect_h / bbox_h,
//   tx = rect_llx - bbox_llx * sx, ty = rect_lly - bbox_lly * sy.
//
// We append `q sx 0 0 sy tx ty cm /SlabFlat<n> Do Q` to the page contents,
// register the Form XObject under `/SlabFlat<n>` in the page's
// `/Resources /XObject`, then once we're done with the page we wipe
// `/Annots`. Finally we drop `/AcroForm` from the catalog so no fields
// remain at the document level.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Choose between fast annotation-bake and full raster legal-grade flatten.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlattenMode {
    /// Bake annotations + AcroForm into page content streams. Fast,
    /// preserves text searchability. Default.
    Annotations,
    /// Stage A (annotation bake) + Stage B (re-render every page at
    /// `dpi` via Poppler `pdftoppm`, replacing the page content stream
    /// with a single ImageXObject). Court-admissible, zero editable
    /// text, irreversible.
    Raster { dpi: u32 },
}

impl Default for FlattenMode {
    fn default() -> Self {
        FlattenMode::Annotations
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FlattenOpts {
    /// If true, also flatten widget annotations (form fields). Default
    /// `true` — flattening fields is the whole point of the operation.
    #[serde(default = "default_true")]
    pub include_widgets: bool,
    /// Annotation-only or full raster.
    #[serde(default)]
    pub mode: FlattenMode,
}

impl Default for FlattenOpts {
    fn default() -> Self {
        Self {
            include_widgets: true,
            mode: FlattenMode::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlattenReport {
    /// Total annotations on the input across all pages (any subtype).
    pub annotations_in: u32,
    /// Annotations whose appearance was successfully baked into the page.
    pub annotations_flattened: u32,
    /// Annotations dropped without baking (e.g. no /AP, no Form XObject).
    pub annotations_dropped: u32,
    /// Pages that had at least one annot before flattening.
    pub pages_with_annotations: u32,
    /// Whether the document had an `/AcroForm` entry (which we remove).
    pub had_acroform: bool,
    /// Pages rebuilt as a single ImageXObject (Raster mode only).
    #[serde(default)]
    pub pages_rasterized: u32,
    /// DPI used when rasterizing (0 in Annotations mode).
    #[serde(default)]
    pub dpi: u32,
}

/// Bake annotations into page content streams and remove the original
/// `/Annots` arrays + `/AcroForm` from the catalog. In `Raster` mode,
/// also re-renders every page at `dpi` and swaps the content stream
/// for a single ImageXObject.
pub fn flatten(input: &Path, output: &Path, opts: FlattenOpts) -> Result<FlattenReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;
    let mut report = flatten_doc(&mut doc, &opts)?;

    if let FlattenMode::Raster { dpi } = opts.mode {
        if !(36..=600).contains(&dpi) {
            return Err(PdfError::Other(format!("dpi {dpi} out of range (36-600)")));
        }
        // Save Stage A to a temp file so pdftoppm can read it.
        let tmp = tempfile::tempdir().map_err(|e| PdfError::Other(format!("tempdir: {e}")))?;
        let stage_a = tmp.path().join("stage_a.pdf");
        doc.compress();
        doc.save(&stage_a)?;
        let mut raster_doc = Document::load(&stage_a)?;
        let pages_rasterized = rasterize_doc(&mut raster_doc, &stage_a, dpi)?;
        report.pages_rasterized = pages_rasterized;
        report.dpi = dpi;
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        raster_doc.compress();
        raster_doc.save(output)?;
        return Ok(report);
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(report)
}

/// Stage B: shell out to `pdftoppm` to rasterize every page at `dpi`,
/// then rebuild each page in `doc` as a single ImageXObject `Do`.
fn rasterize_doc(doc: &mut Document, stage_a: &Path, dpi: u32) -> Result<u32, PdfError> {
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    if pages.is_empty() {
        return Ok(0);
    }
    let tmp = tempfile::tempdir().map_err(|e| PdfError::Other(format!("tempdir: {e}")))?;
    let prefix = tmp.path().join("p");
    let status = std::process::Command::new("pdftoppm")
        .arg("-r")
        .arg(dpi.to_string())
        .arg("-png")
        .arg(stage_a)
        .arg(&prefix)
        .status()
        .map_err(|e| {
            PdfError::Other(format!(
                "pdftoppm not found ({e}). Install poppler: \
                 `brew install poppler` (macOS) / `apt install poppler-utils` (Linux)."
            ))
        })?;
    if !status.success() {
        return Err(PdfError::Other(format!(
            "pdftoppm exited {}",
            status.code().unwrap_or(-1)
        )));
    }
    let mut pngs: Vec<std::path::PathBuf> = std::fs::read_dir(tmp.path())
        .map_err(|e| PdfError::Other(format!("read tmp: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    pngs.sort();
    if pngs.len() != pages.len() {
        return Err(PdfError::Other(format!(
            "pdftoppm produced {} PNGs for {} pages",
            pngs.len(),
            pages.len()
        )));
    }
    rebuild_pages_from_pngs(doc, &pages, &pngs)?;
    Ok(pages.len() as u32)
}

fn rebuild_pages_from_pngs(
    doc: &mut Document,
    pages: &[(u32, ObjectId)],
    pngs: &[std::path::PathBuf],
) -> Result<(), PdfError> {
    for ((_num, page_id), png_path) in pages.iter().zip(pngs.iter()) {
        let img = image::open(png_path)
            .map_err(|e| PdfError::Other(format!("decode {}: {e}", png_path.display())))?;
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        let raw = rgb.into_raw();

        // FlateDecode the raw RGB bytes.
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&raw)
            .map_err(|e| PdfError::Other(format!("flate write: {e}")))?;
        let compressed = enc
            .finish()
            .map_err(|e| PdfError::Other(format!("flate finish: {e}")))?;

        let img_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
        };
        let img_stream = Stream::new(img_dict, compressed);
        let img_id = doc.add_object(Object::Stream(img_stream));

        // Inherited /MediaBox: in points (user-space units).
        let media_box = page_media_box(doc, *page_id);
        let pw = media_box[2] - media_box[0];
        let ph = media_box[3] - media_box[1];

        // Build the new content stream: q pw 0 0 ph llx lly cm /Img Do Q
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        Object::Real(pw),
                        Object::Real(0.0),
                        Object::Real(0.0),
                        Object::Real(ph),
                        Object::Real(media_box[0]),
                        Object::Real(media_box[1]),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"Img".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let content_bytes = content
            .encode()
            .map_err(|e| PdfError::Other(format!("encode content: {e}")))?;
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content_bytes)));

        let page = doc.get_object_mut(*page_id)?.as_dict_mut()?;
        page.set("Contents", Object::Reference(content_id));
        let resources = dictionary! {
            "XObject" => dictionary! { "Img" => Object::Reference(img_id) },
            "ProcSet" => Object::Array(vec![
                Object::Name(b"PDF".to_vec()),
                Object::Name(b"ImageC".to_vec()),
            ]),
        };
        page.set("Resources", Object::Dictionary(resources));
        page.remove(b"Annots");
        // Also drop any inherited /Group / /Fonts on the page node — defensive.
    }
    Ok(())
}

fn page_media_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    let mut current = page_id;
    loop {
        let dict = match doc.get_object(current).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return [0.0, 0.0, 612.0, 792.0],
        };
        if let Ok(arr) = dict.get(b"MediaBox").and_then(|o| o.as_array()) {
            let v: Vec<f32> = arr
                .iter()
                .filter_map(|o| match o {
                    Object::Integer(i) => Some(*i as f32),
                    Object::Real(f) => Some(*f),
                    _ => None,
                })
                .collect();
            if v.len() == 4 {
                return [v[0], v[1], v[2], v[3]];
            }
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(pid)) => current = *pid,
            _ => return [0.0, 0.0, 612.0, 792.0],
        }
    }
}

fn flatten_doc(doc: &mut Document, opts: &FlattenOpts) -> Result<FlattenReport, PdfError> {
    let mut report = FlattenReport::default();

    // Snapshot page list (num → id) so mutating the doc doesn't trip the borrow.
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();

    for (_num, page_id) in pages {
        let annot_ids = collect_annot_ids(doc, page_id);
        if annot_ids.is_empty() {
            continue;
        }
        report.pages_with_annotations += 1;
        report.annotations_in += annot_ids.len() as u32;

        // Plan: for each annot, try to extract (rect, form_xobject_id). We
        // collect everything first so we can append a single content stream
        // and add resources in one shot.
        let mut overlays: Vec<Overlay> = Vec::new();
        for aid in &annot_ids {
            match plan_annot(doc, *aid, opts) {
                Some(o) => overlays.push(o),
                None => report.annotations_dropped += 1,
            }
        }

        if !overlays.is_empty() {
            apply_overlays(doc, page_id, &overlays)?;
            report.annotations_flattened += overlays.len() as u32;
        }

        // Always clear /Annots — flattening means no live annots remain.
        clear_annots(doc, page_id);
    }

    // Drop /AcroForm from the catalog if present.
    report.had_acroform = remove_acroform(doc);

    Ok(report)
}

/// One annotation we plan to bake into a page.
struct Overlay {
    rect: [f32; 4],
    bbox: [f32; 4],
    form_id: ObjectId,
}

fn plan_annot(doc: &Document, annot_id: ObjectId, opts: &FlattenOpts) -> Option<Overlay> {
    let annot = doc.get_object(annot_id).ok()?.as_dict().ok()?;

    // Filter widgets if include_widgets=false (caller's choice).
    if !opts.include_widgets {
        if let Ok(subtype_bytes) = annot.get(b"Subtype").and_then(|o| o.as_name()) {
            if subtype_bytes == b"Widget" {
                return None;
            }
        }
    }

    let rect = read_rect(annot.get(b"Rect").ok()?)?;

    // /AP /N is either a direct/indirect Form stream, or a dictionary
    // keyed by appearance state. Resolve to a single Form stream id.
    let ap = annot.get(b"AP").ok()?.as_dict().ok()?;
    let n_obj = ap.get(b"N").ok()?;
    let form_id = resolve_appearance(doc, n_obj, annot)?;

    // Now read the Form's /BBox from its dict.
    let bbox = {
        let obj = doc.get_object(form_id).ok()?;
        let dict = match obj {
            Object::Stream(s) => &s.dict,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        read_rect(dict.get(b"BBox").ok()?)?
    };

    Some(Overlay {
        rect,
        bbox,
        form_id,
    })
}

fn resolve_appearance(
    doc: &Document,
    n_obj: &Object,
    annot: &lopdf::Dictionary,
) -> Option<ObjectId> {
    // Direct reference: easiest case.
    if let Ok(id) = n_obj.as_reference() {
        // Could itself be a dict (state-keyed) or a stream — re-read.
        let inner = doc.get_object(id).ok()?;
        match inner {
            Object::Stream(_) => return Some(id),
            Object::Dictionary(d) => return pick_state(doc, d, annot),
            _ => return None,
        }
    }
    // Inline dictionary keyed by /AS.
    if let Ok(d) = n_obj.as_dict() {
        return pick_state(doc, d, annot);
    }
    None
}

fn pick_state(
    doc: &Document,
    state_dict: &lopdf::Dictionary,
    annot: &lopdf::Dictionary,
) -> Option<ObjectId> {
    // Prefer the entry matching /AS.
    let preferred_key: Option<Vec<u8>> = annot
        .get(b"AS")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n.to_vec());

    if let Some(key) = &preferred_key {
        if let Ok(o) = state_dict.get(key) {
            if let Ok(id) = o.as_reference() {
                if let Ok(Object::Stream(_)) = doc.get_object(id) {
                    return Some(id);
                }
            }
        }
    }
    // Else first stream-valued entry.
    for (_k, v) in state_dict.iter() {
        if let Ok(id) = v.as_reference() {
            if let Ok(Object::Stream(_)) = doc.get_object(id) {
                return Some(id);
            }
        }
    }
    None
}

fn read_rect(obj: &Object) -> Option<[f32; 4]> {
    let arr = obj.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let mut out = [0.0f32; 4];
    for (i, v) in arr.iter().take(4).enumerate() {
        out[i] = match v {
            Object::Integer(n) => *n as f32,
            Object::Real(r) => *r,
            _ => return None,
        };
    }
    // Normalize so [0]/[1] = lower-left, [2]/[3] = upper-right.
    if out[0] > out[2] {
        out.swap(0, 2);
    }
    if out[1] > out[3] {
        out.swap(1, 3);
    }
    Some(out)
}

fn collect_annot_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let Ok(page) = doc.get_object(page_id) else {
        return out;
    };
    let Ok(dict) = page.as_dict() else {
        return out;
    };
    let annots_obj = match dict.get(b"Annots") {
        Ok(o) => o,
        Err(_) => return out,
    };
    // /Annots may be inline array or an indirect reference.
    let arr = match annots_obj {
        Object::Array(a) => a.clone(),
        Object::Reference(r) => {
            let Ok(o) = doc.get_object(*r) else {
                return out;
            };
            match o {
                Object::Array(a) => a.clone(),
                _ => return out,
            }
        }
        _ => return out,
    };
    for v in arr {
        if let Object::Reference(id) = v {
            out.push(id);
        }
    }
    out
}

fn clear_annots(doc: &mut Document, page_id: ObjectId) {
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
        dict.remove(b"Annots");
    }
}

fn remove_acroform(doc: &mut Document) -> bool {
    let Ok(root_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) else {
        return false;
    };
    let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(root_ref) else {
        return false;
    };
    catalog.remove(b"AcroForm").is_some()
}

fn apply_overlays(
    doc: &mut Document,
    page_id: ObjectId,
    overlays: &[Overlay],
) -> Result<(), PdfError> {
    // Build the content stream that draws every overlay.
    let mut ops: Vec<Operation> = Vec::with_capacity(overlays.len() * 6);
    let mut resource_entries: Vec<(String, ObjectId)> = Vec::with_capacity(overlays.len());

    for (i, ov) in overlays.iter().enumerate() {
        let name = format!("SlabFlat{}", i);
        let bbox_w = (ov.bbox[2] - ov.bbox[0]).max(f32::MIN_POSITIVE);
        let bbox_h = (ov.bbox[3] - ov.bbox[1]).max(f32::MIN_POSITIVE);
        let rect_w = ov.rect[2] - ov.rect[0];
        let rect_h = ov.rect[3] - ov.rect[1];
        let sx = rect_w / bbox_w;
        let sy = rect_h / bbox_h;
        let tx = ov.rect[0] - ov.bbox[0] * sx;
        let ty = ov.rect[1] - ov.bbox[1] * sy;

        ops.push(Operation::new("q", vec![]));
        ops.push(Operation::new(
            "cm",
            vec![
                Object::Real(sx),
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(sy),
                Object::Real(tx),
                Object::Real(ty),
            ],
        ));
        ops.push(Operation::new(
            "Do",
            vec![Object::Name(name.clone().into_bytes())],
        ));
        ops.push(Operation::new("Q", vec![]));

        resource_entries.push((name, ov.form_id));
    }

    let content = Content { operations: ops };
    let stream_bytes = content
        .encode()
        .map_err(|e| PdfError::Other(format!("flatten: encode content stream: {e}")))?;
    let new_stream_id = doc.add_object(Stream::new(dictionary! {}, stream_bytes));

    // Append the new stream onto /Contents.
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
        let new_contents = match dict.get(b"Contents") {
            Ok(Object::Reference(r)) => {
                vec![Object::Reference(*r), Object::Reference(new_stream_id)]
            }
            Ok(Object::Array(arr)) => {
                let mut v = arr.clone();
                v.push(Object::Reference(new_stream_id));
                v
            }
            _ => vec![Object::Reference(new_stream_id)],
        };
        dict.set("Contents", new_contents);
    }

    // Merge resource entries into /Resources /XObject.
    merge_xobject_resources(doc, page_id, &resource_entries)?;
    Ok(())
}

fn merge_xobject_resources(
    doc: &mut Document,
    page_id: ObjectId,
    entries: &[(String, ObjectId)],
) -> Result<(), PdfError> {
    // Read current /Resources (may be inline dict, an indirect ref, or absent).
    let resources_value = match doc.get_object(page_id) {
        Ok(Object::Dictionary(d)) => d.get(b"Resources").ok().cloned(),
        _ => None,
    };
    let mut resources_dict = match resources_value {
        Some(Object::Dictionary(d)) => d.clone(),
        Some(Object::Reference(r)) => doc
            .get_object(r)
            .ok()
            .and_then(|o| o.as_dict().ok().cloned())
            .unwrap_or_default(),
        _ => lopdf::Dictionary::new(),
    };

    let mut xobject = match resources_dict.get(b"XObject") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .ok()
            .and_then(|o| o.as_dict().ok().cloned())
            .unwrap_or_default(),
        _ => lopdf::Dictionary::new(),
    };

    for (name, id) in entries {
        xobject.set(name.as_str(), Object::Reference(*id));
    }
    resources_dict.set("XObject", Object::Dictionary(xobject));

    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
        dict.set("Resources", Object::Dictionary(resources_dict));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use lopdf::dictionary;

    /// Build a PDF with one page, one widget annotation whose /AP /N is a
    /// real Form XObject containing a simple "draw a 10×10 black square"
    /// content stream. We use this to verify that `flatten` bakes the
    /// Form into the page contents and strips /Annots.
    fn pdf_with_one_widget(path: &Path) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        // The Form XObject: BBox 0..10, content = filled square.
        let form_stream = b"0 0 10 10 re f\n".to_vec();
        let form_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            },
            form_stream,
        ));

        // Annot dict referencing the Form.
        let annot_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![100.into(), 100.into(), 200.into(), 200.into()],
            "AP" => dictionary! {
                "N" => Object::Reference(form_id),
            },
            "T" => Object::string_literal("test_field"),
        });

        // Existing page content (a label so the page isn't empty).
        let label_content = b"BT /F1 12 Tf 50 700 Td (page) Tj ET\n".to_vec();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let contents_id = doc.add_object(Stream::new(dictionary! {}, label_content));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => contents_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => resources_id,
            "Annots" => vec![Object::Reference(annot_id)],
        });

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );

        // AcroForm with one field reference, so we can verify removal.
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![Object::Reference(annot_id)],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => Object::Reference(acroform_id),
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
    fn flatten_bakes_widget_appearance() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("widget.pdf");
        let dst = tmp.path().join("flat.pdf");
        pdf_with_one_widget(&src);

        let report = flatten(&src, &dst, FlattenOpts::default()).unwrap();
        assert_eq!(report.annotations_in, 1, "should see the widget");
        assert_eq!(report.annotations_flattened, 1, "should bake the widget");
        assert_eq!(report.annotations_dropped, 0);
        assert_eq!(report.pages_with_annotations, 1);
        assert!(report.had_acroform, "should detect & remove AcroForm");

        // Reload and verify no /Annots remain, no /AcroForm.
        let reloaded = Document::load(&dst).unwrap();
        for (_n, pid) in reloaded.get_pages() {
            let dict = reloaded.get_object(pid).unwrap().as_dict().unwrap();
            assert!(
                dict.get(b"Annots").is_err(),
                "page should have no /Annots after flatten"
            );
            // Resources should have /XObject /SlabFlat0 now.
            let resources = dict.get(b"Resources").unwrap().as_dict().unwrap();
            let xobj = resources.get(b"XObject").unwrap().as_dict().unwrap();
            assert!(xobj.get(b"SlabFlat0").is_ok(), "expected /SlabFlat0 entry");
        }
        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        assert!(
            catalog.get(b"AcroForm").is_err(),
            "AcroForm should be removed"
        );
    }

    #[test]
    fn flatten_passes_through_pdf_without_annots() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("plain.pdf");
        let dst = tmp.path().join("plain_flat.pdf");
        make_n_page_pdf(&src, 2);

        let report = flatten(&src, &dst, FlattenOpts::default()).unwrap();
        assert_eq!(report.annotations_in, 0);
        assert_eq!(report.annotations_flattened, 0);
        assert_eq!(report.pages_with_annotations, 0);
        assert!(!report.had_acroform);

        // Output is still a valid 2-page PDF.
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 2);
    }

    #[test]
    fn flatten_drops_annot_without_appearance() {
        // An annot with no /AP — common for freshly-created widget fields
        // whose appearance hasn't been generated. We can't bake it, so we
        // drop it (and count it under annotations_dropped).
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("no_ap.pdf");
        let dst = tmp.path().join("no_ap_flat.pdf");

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let annot_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![10.into(), 10.into(), 20.into(), 20.into()],
        });
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Annots" => vec![Object::Reference(annot_id)],
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
            "Pages" => pages_id,
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
        doc.save(&src).unwrap();

        let report = flatten(&src, &dst, FlattenOpts::default()).unwrap();
        assert_eq!(report.annotations_in, 1);
        assert_eq!(report.annotations_flattened, 0);
        assert_eq!(report.annotations_dropped, 1);

        // /Annots removed regardless.
        let reloaded = Document::load(&dst).unwrap();
        for (_n, pid) in reloaded.get_pages() {
            let dict = reloaded.get_object(pid).unwrap().as_dict().unwrap();
            assert!(dict.get(b"Annots").is_err());
        }
    }

    #[test]
    fn flatten_rejects_missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("out.pdf");
        let err = flatten(
            &tmp.path().join("does_not_exist.pdf"),
            &dst,
            FlattenOpts::default(),
        )
        .unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
