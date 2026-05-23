//! Orchestrator: glue passes 1-6 into one PDF/X-4 conversion entry point.
//!
//! ISO 15930-7 conformance requires the passes run in this exact order:
//!
//! 1. **sanitize**       — strip JavaScript, AA actions, encryption.
//!    Reuses [`crate::pdf::pdfa::sanitize::sanitize_for_pdfa`].
//! 2. **font_embed**     — embed Standard-14 substitutes for any font
//!    that lacks `/FontDescriptor`. Reuses
//!    [`crate::pdf::pdfa::font_embed::embed_missing_in_doc`].
//! 3. **color_normalize** — install ICC `/DefaultRGB|Gray|CMYK` on every
//!    page. See [`super::color::normalize_color`].
//! 4. **geometry**       — synthesize `/TrimBox`, optional `/BleedBox`.
//!    See [`super::geometry::ensure_print_boxes`].
//! 5. **metadata_inject** — write the XMP packet with `pdfxid` namespace.
//!    See [`build_pdfx_xmp_packet`].
//! 6. **output_intent**  — write `/OutputIntents` array with `/S /GTS_PDFX`
//!    and the output-intent ICC profile as a stream. See
//!    [`write_pdfx_output_intent`].
//!
//! The result is then saved to `output_path`. The conversion is *not*
//! validated here — that's Slice 5's [`super::validate::validate_pdfx`].

use std::path::{Path, PathBuf};
use std::time::Instant;

use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use super::color::{normalize_color, ColorOptions};
use super::geometry::{ensure_print_boxes, GeometryOptions};
use super::OutputIntent;
use crate::pdf::pdfa::font_embed::embed_missing_in_doc;
use crate::pdf::pdfa::sanitize::sanitize_for_pdfa;

/// User-facing knobs for [`convert_to_pdfx4`].
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// Which bundled ICC profile to use as the OutputIntent. Defaults
    /// to FOGRA51 (European coated).
    pub output_intent: OutputIntent,
    /// If `true`, add a 3 mm BleedBox outset on every page.
    pub add_bleed: bool,
    /// Optional title to inject into the XMP packet's `dc:title`.
    pub title: Option<String>,
    /// Optional `xmp:CreatorTool` override. Defaults to "Slab Press v3.8.0".
    pub creator_tool: Option<String>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            output_intent: OutputIntent::Fogra51Coated,
            add_bleed: false,
            title: None,
            creator_tool: None,
        }
    }
}

/// Summary returned by [`convert_to_pdfx4`].
#[derive(Debug, Clone, Default)]
pub struct ConvertReport {
    pub output_path: PathBuf,
    pub elapsed_ms: u128,
    pub fonts_embedded: usize,
    pub javascript_stripped: usize,
    pub annotations_sanitized: usize,
    pub color_pages_touched: usize,
    pub color_default_entries_added: usize,
    pub trimbox_synthesized: usize,
    pub trimbox_preserved: usize,
    pub bleed_added: usize,
    pub output_intent_id: Option<ObjectId>,
    pub xmp_metadata_id: Option<ObjectId>,
    /// Pretty label e.g. "FOGRA51" / "GRACoL2013_CRPC6" copied from the
    /// chosen intent for the UI.
    pub intent_label: String,
}

const DEFAULT_CREATOR_TOOL: &str = "Slab Press v3.8.0";

/// Convert an input PDF on disk into a PDF/X-4 document at `output_path`.
///
/// Errors surface as `String` (consistent with the rest of the press
/// module and the Tauri command boundary).
pub fn convert_to_pdfx4(
    input_path: &Path,
    output_path: &Path,
    opts: &ConvertOptions,
) -> Result<ConvertReport, String> {
    let started = Instant::now();
    let mut doc = Document::load(input_path).map_err(|e| format!("load {input_path:?}: {e}"))?;

    let mut report = ConvertReport {
        output_path: output_path.to_path_buf(),
        intent_label: opts.output_intent.condition_identifier().to_string(),
        ..Default::default()
    };

    // ── Pass 1: sanitize (strip JS, AA, Encrypt) ───────────────────
    let san = sanitize_for_pdfa(&mut doc).map_err(|e| format!("sanitize: {e}"))?;
    report.javascript_stripped = san
        .removed
        .iter()
        .filter(|s| {
            let l = s.to_ascii_lowercase();
            l.contains("javascript") || l.contains("js") || l.contains("openaction")
        })
        .count();
    report.annotations_sanitized = san.removed.len();

    // ── Pass 2: embed missing fonts ────────────────────────────────
    report.fonts_embedded =
        embed_missing_in_doc(&mut doc).map_err(|e| format!("font_embed: {e}"))?;

    // ── Pass 3: color normalization ────────────────────────────────
    let color_opts = ColorOptions {
        target_intent: opts.output_intent,
        cmyk_only: false,
    };
    let color_stats = normalize_color(&mut doc, &color_opts).map_err(|e| format!("color: {e}"))?;
    report.color_pages_touched = color_stats.pages_touched;
    report.color_default_entries_added = color_stats.default_entries_added;

    // ── Pass 4: geometry ───────────────────────────────────────────
    let geom_opts = GeometryOptions {
        add_bleed: opts.add_bleed,
        ..Default::default()
    };
    let geom_stats =
        ensure_print_boxes(&mut doc, &geom_opts).map_err(|e| format!("geometry: {e}"))?;
    report.trimbox_synthesized = geom_stats.trimbox_synthesized;
    report.trimbox_preserved = geom_stats.trimbox_preserved;
    report.bleed_added = geom_stats.bleed_added;

    // ── Pass 5+6: XMP metadata + OutputIntent (PDF/X-4 flavored) ──
    let xmp_id = write_pdfx_xmp_metadata(&mut doc, opts)?;
    report.xmp_metadata_id = Some(xmp_id);
    let oi_id = write_pdfx_output_intent(&mut doc, opts.output_intent)?;
    report.output_intent_id = Some(oi_id);

    // Set /Catalog /Version = /1.6 (PDF/X-4 requires PDF >= 1.6).
    ensure_version_at_least_1_6(&mut doc)?;

    // ── Save ───────────────────────────────────────────────────────
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
        }
    }
    doc.save(output_path)
        .map_err(|e| format!("save {output_path:?}: {e}"))?;

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

/// Build a PDF/X-4 conformant XMP packet (pdfxid namespace) and attach
/// it to the catalog's `/Metadata` entry. Returns the stream object id.
fn write_pdfx_xmp_metadata(doc: &mut Document, opts: &ConvertOptions) -> Result<ObjectId, String> {
    let creator = opts
        .creator_tool
        .clone()
        .unwrap_or_else(|| DEFAULT_CREATOR_TOOL.to_string());
    let title = opts.title.clone();
    let bytes = build_pdfx_xmp_packet(&creator, title.as_deref());

    let dict = dictionary! {
        "Type" => "Metadata",
        "Subtype" => "XML",
        "Length" => bytes.len() as i64,
    };
    // ISO 15930-7 §6.7: XMP packet MUST NOT be compressed/encrypted.
    let stream = Stream::new(dict, bytes).with_compression(false);
    let xmp_id = doc.add_object(Object::Stream(stream));

    let cat_id = catalog_id(doc)?;
    let cat = doc
        .get_object_mut(cat_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| format!("catalog: {e}"))?;
    cat.set("Metadata", Object::Reference(xmp_id));
    Ok(xmp_id)
}

/// Build the PDF/X-4 XMP packet bytes. Includes `pdfxid:GTS_PDFXVersion =
/// PDF/X-4` + `pdfxid:GTS_PDFXConformance = PDF/X-4` per ISO 15930-7 §6.7.4.
pub fn build_pdfx_xmp_packet(creator_tool: &str, title: Option<&str>) -> Vec<u8> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut s = String::with_capacity(2048);

    s.push_str("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    s.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Slab Press 3.8.0\">\n");
    s.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");

    // pdfxid — THE PDF/X-4 mandatory marker.
    s.push_str(
        "    <rdf:Description rdf:about=\"\" xmlns:pdfxid=\"http://www.npes.org/pdfx/ns/id/\">\n",
    );
    s.push_str("      <pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion>\n");
    s.push_str("      <pdfxid:GTS_PDFXConformance>PDF/X-4</pdfxid:GTS_PDFXConformance>\n");
    s.push_str("    </rdf:Description>\n");

    // Dublin Core (title) when provided.
    if let Some(t) = title {
        s.push_str(
            "    <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n",
        );
        s.push_str("      <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">");
        s.push_str(&xml_escape(t));
        s.push_str("</rdf:li></rdf:Alt></dc:title>\n");
        s.push_str("    </rdf:Description>\n");
    }

    // pdf:Producer + xmp:CreatorTool + dates.
    s.push_str("    <rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n");
    s.push_str(&format!(
        "      <pdf:Producer>{}</pdf:Producer>\n",
        xml_escape(creator_tool)
    ));
    s.push_str("    </rdf:Description>\n");

    s.push_str("    <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n");
    s.push_str(&format!(
        "      <xmp:CreatorTool>{}</xmp:CreatorTool>\n",
        xml_escape(creator_tool)
    ));
    s.push_str(&format!("      <xmp:CreateDate>{now}</xmp:CreateDate>\n"));
    s.push_str(&format!("      <xmp:ModifyDate>{now}</xmp:ModifyDate>\n"));
    s.push_str("    </rdf:Description>\n");

    s.push_str("  </rdf:RDF>\n");
    s.push_str("</x:xmpmeta>\n");

    // Whitespace padding for in-place edits (Adobe XMP §7.3.2).
    for _ in 0..32 {
        s.push_str("                                                  \n");
    }
    s.push_str("<?xpacket end=\"w\"?>");

    s.into_bytes()
}

/// Write a PDF/X-4 OutputIntent dict referencing the chosen bundled
/// ICC profile, idempotent (won't add a second GTS_PDFX entry if one
/// already exists). Returns the OutputIntent dict's object id.
pub fn write_pdfx_output_intent(
    doc: &mut Document,
    intent: OutputIntent,
) -> Result<ObjectId, String> {
    let cat_id = catalog_id(doc)?;

    // If an existing GTS_PDFX entry exists, return its id and skip.
    if let Some(existing) = find_existing_pdfx_intent(doc, cat_id) {
        return Ok(existing);
    }

    // ICC profile stream (Flate compressed).
    let icc_bytes = flate_compress(intent.profile_bytes());
    let n = read_icc_component_count(intent.profile_bytes()).unwrap_or(4);
    let icc_dict = dictionary! {
        "N" => n as i64,
        "Filter" => "FlateDecode",
        "Length" => icc_bytes.len() as i64,
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes).with_compression(false);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

    let oi_dict = dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFX",
        "OutputConditionIdentifier" => Object::string_literal(intent.condition_identifier()),
        "RegistryName" => Object::string_literal(intent.registry_name()),
        "Info" => Object::string_literal(intent.info_string()),
        "DestOutputProfile" => Object::Reference(icc_id),
    };
    let oi_id = doc.add_object(Object::Dictionary(oi_dict));

    let cat = doc
        .get_object_mut(cat_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| format!("catalog: {e}"))?;
    let existing = cat.get(b"OutputIntents").ok().cloned();
    let new_array = match existing {
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(oi_id));
            Object::Array(arr)
        }
        _ => Object::Array(vec![Object::Reference(oi_id)]),
    };
    cat.set("OutputIntents", new_array);
    Ok(oi_id)
}

fn catalog_id(doc: &Document) -> Result<ObjectId, String> {
    match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => Ok(*id),
        _ => Err("trailer /Root missing or not a reference".to_string()),
    }
}

fn find_existing_pdfx_intent(doc: &Document, cat_id: ObjectId) -> Option<ObjectId> {
    let cat = doc.get_object(cat_id).ok()?.as_dict().ok()?;
    let arr = cat.get(b"OutputIntents").ok()?.as_array().ok()?;
    for entry in arr {
        let (id, dict) = match entry {
            Object::Reference(id) => {
                let d = doc.get_object(*id).ok()?.as_dict().ok()?;
                (Some(*id), d)
            }
            Object::Dictionary(d) => (None, d),
            _ => continue,
        };
        if let Ok(Object::Name(n)) = dict.get(b"S") {
            if n == b"GTS_PDFX" {
                return id;
            }
        }
    }
    None
}

fn ensure_version_at_least_1_6(doc: &mut Document) -> Result<(), String> {
    // Set the file-level version. lopdf stores it on the Document.
    if doc.version.as_str() < "1.6" {
        doc.version = "1.6".to_string();
    }
    // Also set /Catalog /Version per ISO 32000 §7.5.5 in case the
    // file-level header is older — readers prefer the catalog entry.
    let cat_id = catalog_id(doc)?;
    let cat = doc
        .get_object_mut(cat_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| format!("catalog: {e}"))?;
    cat.set("Version", Object::Name(b"1.6".to_vec()));
    Ok(())
}

fn read_icc_component_count(profile: &[u8]) -> Result<u8, String> {
    if profile.len() < 20 {
        return Err("icc profile too small".into());
    }
    // ICC header colour space signature at offset 16..20.
    match &profile[16..20] {
        b"RGB " => Ok(3),
        b"GRAY" => Ok(1),
        b"CMYK" => Ok(4),
        other => Err(format!(
            "unsupported ICC colour space sig: {:?}",
            std::str::from_utf8(other).unwrap_or("??")
        )),
    }
}

fn flate_compress(bytes: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes)
        .expect("flate write to Vec is infallible");
    enc.finish().expect("flate finish to Vec is infallible")
}

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' && c != '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};
    use std::io::Write;

    fn minimal_pdf_path() -> tempfile::NamedTempFile {
        let mut doc = Document::with_version("1.4");
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0), Object::Real(0.0),
                Object::Real(612.0), Object::Real(792.0),
            ]),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1i64,
        });
        {
            let p = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
            p.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(cat_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        let mut f = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        f.write_all(&buf).unwrap();
        f
    }

    #[test]
    fn build_pdfx_xmp_packet_contains_required_markers() {
        let bytes = build_pdfx_xmp_packet("Slab Press v3.8.0", Some("Brochure"));
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("pdfxid:GTS_PDFXVersion>PDF/X-4<"));
        assert!(s.contains("pdfxid:GTS_PDFXConformance>PDF/X-4<"));
        assert!(s.contains("<dc:title>"));
        assert!(s.contains("Brochure"));
        assert!(s.contains("<xmp:CreatorTool>Slab Press v3.8.0</xmp:CreatorTool>"));
        assert!(s.starts_with("<?xpacket begin=\"\u{feff}\""));
        assert!(s.ends_with("<?xpacket end=\"w\"?>"));
    }

    #[test]
    fn build_pdfx_xmp_packet_omits_title_when_none() {
        let bytes = build_pdfx_xmp_packet("Creator", None);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(!s.contains("xmlns:dc="));
    }

    #[test]
    fn read_icc_component_count_recognizes_known_sigs() {
        // FOGRA51 is CMYK.
        let cmyk = OutputIntent::Fogra51Coated.profile_bytes();
        assert_eq!(read_icc_component_count(cmyk).unwrap(), 4);
        let gracol = OutputIntent::Gracol2013Crpc6.profile_bytes();
        assert_eq!(read_icc_component_count(gracol).unwrap(), 4);
    }

    #[test]
    fn write_pdfx_output_intent_installs_gts_pdfx_entry() {
        let mut doc = Document::with_version("1.6");
        let cat_id = doc.add_object(dictionary! { "Type" => "Catalog" });
        doc.trailer.set("Root", Object::Reference(cat_id));

        let oi_id = write_pdfx_output_intent(&mut doc, OutputIntent::Fogra51Coated).unwrap();
        let cat = doc.get_object(cat_id).unwrap().as_dict().unwrap();
        let arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
        match &arr[0] {
            Object::Reference(id) => assert_eq!(*id, oi_id),
            other => panic!("expected reference, got {other:?}"),
        }

        let oi = doc.get_object(oi_id).unwrap().as_dict().unwrap();
        assert_eq!(oi.get(b"S").unwrap().as_name().unwrap(), b"GTS_PDFX");
        assert_eq!(
            oi.get(b"OutputConditionIdentifier")
                .unwrap()
                .as_str()
                .unwrap(),
            b"FOGRA51"
        );
    }

    #[test]
    fn write_pdfx_output_intent_idempotent() {
        let mut doc = Document::with_version("1.6");
        let cat_id = doc.add_object(dictionary! { "Type" => "Catalog" });
        doc.trailer.set("Root", Object::Reference(cat_id));
        let first = write_pdfx_output_intent(&mut doc, OutputIntent::Fogra51Coated).unwrap();
        let second = write_pdfx_output_intent(&mut doc, OutputIntent::Fogra51Coated).unwrap();
        assert_eq!(first, second);
        let cat = doc.get_object(cat_id).unwrap().as_dict().unwrap();
        let arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
    }

    #[test]
    fn convert_to_pdfx4_end_to_end_minimal_doc() {
        let input = minimal_pdf_path();
        let out = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let opts = ConvertOptions {
            output_intent: OutputIntent::Fogra51Coated,
            add_bleed: true,
            title: Some("Test".into()),
            creator_tool: None,
        };
        let report = convert_to_pdfx4(input.path(), out.path(), &opts).unwrap();

        assert_eq!(report.intent_label, "FOGRA51");
        assert!(report.output_intent_id.is_some());
        assert!(report.xmp_metadata_id.is_some());
        // Page had only MediaBox — TrimBox should be synthesized.
        assert_eq!(report.trimbox_synthesized, 1);
        assert_eq!(report.bleed_added, 1);

        // Re-open and verify the resulting PDF has the markers we need.
        let doc = Document::load(out.path()).unwrap();
        let cat = doc.catalog().unwrap();
        let oi_arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        assert_eq!(oi_arr.len(), 1);
        let oi_ref = match &oi_arr[0] {
            Object::Reference(id) => *id,
            _ => panic!("OutputIntent should be indirect"),
        };
        let oi = doc.get_object(oi_ref).unwrap().as_dict().unwrap();
        assert_eq!(oi.get(b"S").unwrap().as_name().unwrap(), b"GTS_PDFX");

        // XMP packet present.
        let meta_ref = cat.get(b"Metadata").unwrap();
        let meta_id = match meta_ref {
            Object::Reference(id) => *id,
            _ => panic!("Metadata should be indirect"),
        };
        let xmp_stream = doc.get_object(meta_id).unwrap().as_stream().unwrap();
        let xmp_str = std::str::from_utf8(&xmp_stream.content).unwrap();
        assert!(xmp_str.contains("PDF/X-4"));

        // Version >= 1.6.
        assert!(doc.version.as_str() >= "1.6");
    }

    #[test]
    fn convert_strips_javascript_action() {
        let mut doc = Document::with_version("1.4");
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0), Object::Real(0.0),
                Object::Real(612.0), Object::Real(792.0),
            ]),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1i64,
        });
        {
            let p = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
            p.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "OpenAction" => Object::Dictionary(dictionary! {
                "Type" => "Action",
                "S" => "JavaScript",
                "JS" => Object::string_literal("app.alert('pwn')"),
            }),
        });
        doc.trailer.set("Root", Object::Reference(cat_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        let mut input = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        input.write_all(&buf).unwrap();

        let out = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let report =
            convert_to_pdfx4(input.path(), out.path(), &ConvertOptions::default()).unwrap();
        assert!(report.javascript_stripped >= 1);

        // Output must not contain the JavaScript action anymore.
        let doc2 = Document::load(out.path()).unwrap();
        let cat2 = doc2.catalog().unwrap();
        assert!(cat2.get(b"OpenAction").is_err());
    }
}
