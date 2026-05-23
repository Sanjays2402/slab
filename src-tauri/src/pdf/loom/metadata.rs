// src-tauri/src/pdf/loom/metadata.rs
//
// Slab Loom — Slice 6: PDF/UA-1 metadata.
//
// Public entry: [`apply_pdfua_metadata`] mutates a (typically already-tagged)
// `lopdf::Document` in place to satisfy the metadata-shaped clauses of
// ISO 14289-1:2014/Amd.1:2018 (PDF/UA-1).
//
// What this emits (per the spec):
//   * `/Metadata` stream object on the catalog containing an XMP packet with
//     the `pdfuaid:part="1"` identifier (Matterhorn 06-002 + 06-003 + 07).
//   * `/ViewerPreferences << /DisplayDocTitle true >>` on the catalog so
//     readers display the document title rather than the file name
//     (Matterhorn 11-002 / ISO 14289-1 §7.1.7).
//   * `/Lang` on the catalog if absent, defaulted to the caller's
//     `fallback_lang` (e.g. "en-US"), so screen readers pick a voice.
//   * `/Title` in the Info dict synced into the XMP `<dc:title>` (Matterhorn
//     06-001).
//
// XMP packet shape: we hand-template the canonical Adobe XMP packet shipped
// with PDF/UA-1 sample files. No XML library required — the surface area is
// small, the output is byte-stable, and we test it.
//
// Idempotency: applying twice yields exactly the same catalog (one Metadata
// stream, one ViewerPreferences dict). Safe to re-run after edits.

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

/// Caller-supplied knobs.
#[derive(Debug, Clone, Default)]
pub struct MetadataOptions {
    /// Document title. If `None`, we read the existing Info dict `/Title`.
    /// If both are absent we leave Title unset (validator will fail 06-001).
    pub title: Option<String>,
    /// Author for `<dc:creator>` / Info `/Author`. Optional.
    pub author: Option<String>,
    /// BCP-47 language tag (e.g. "en-US"). Used for catalog `/Lang` if absent
    /// and for the XMP `<dc:language>` field.
    pub fallback_lang: Option<String>,
    /// ISO-8601 timestamp embedded as `<xmp:CreateDate>`/`<xmp:ModifyDate>`.
    /// If `None`, we use "now" in UTC.
    pub timestamp: Option<String>,
}

/// What `apply_pdfua_metadata` did.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataStats {
    pub xmp_bytes: usize,
    pub title_set: bool,
    pub lang_set: bool,
    pub viewer_prefs_set: bool,
}

/// Compose the XMP packet. Exposed for testing.
pub fn build_xmp_packet(opts: &MetadataOptions) -> String {
    let title = opts.title.as_deref().unwrap_or("");
    let author = opts.author.as_deref().unwrap_or("");
    let lang = opts.fallback_lang.as_deref().unwrap_or("en-US");
    let ts = opts.timestamp.clone().unwrap_or_else(now_iso8601);
    let title_xml = xml_escape(title);
    let author_xml = xml_escape(author);

    // Canonical XMP packet — schemas: dc, xmp, pdf, pdfuaid.
    format!(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Slab Loom\">\n\
  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
    <rdf:Description rdf:about=\"\"\n\
      xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
      xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n\
      xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"\n\
      xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\">\n\
      <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></dc:title>\n\
      <dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>\n\
      <dc:language><rdf:Bag><rdf:li>{lang}</rdf:li></rdf:Bag></dc:language>\n\
      <xmp:CreateDate>{ts}</xmp:CreateDate>\n\
      <xmp:ModifyDate>{ts}</xmp:ModifyDate>\n\
      <xmp:CreatorTool>Slab Loom (Slab v3.1.0)</xmp:CreatorTool>\n\
      <pdf:Producer>Slab Loom</pdf:Producer>\n\
      <pdfuaid:part>1</pdfuaid:part>\n\
    </rdf:Description>\n\
  </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"r\"?>\n",
        title = title_xml,
        author = author_xml,
        lang = lang,
        ts = ts,
    )
}

/// Mutate `doc` to carry full PDF/UA-1 metadata. Idempotent.
pub fn apply_pdfua_metadata(
    doc: &mut Document,
    opts: &MetadataOptions,
) -> Result<MetadataStats, String> {
    let mut stats = MetadataStats::default();

    // ---- Resolve final title: opts.title > Info /Title > nothing.
    let info_title = read_info_title(doc);
    let final_title = opts
        .title
        .clone()
        .or(info_title)
        .filter(|s| !s.trim().is_empty());

    // Effective opts used in XMP (so xmp <dc:title> matches Info /Title).
    let effective_opts = MetadataOptions {
        title: final_title.clone(),
        author: opts.author.clone(),
        fallback_lang: opts.fallback_lang.clone(),
        timestamp: opts.timestamp.clone(),
    };

    // ---- Update Info dict /Title and /Author.
    if let Some(t) = final_title.as_ref() {
        ensure_info_dict(doc)?;
        let info_id = doc
            .trailer
            .get(b"Info")
            .map_err(|e| e.to_string())?
            .as_reference()
            .map_err(|e| e.to_string())?;
        let info = doc.get_dictionary_mut(info_id).map_err(|e| e.to_string())?;
        info.set("Title", Object::string_literal(t.clone()));
        stats.title_set = true;
        if let Some(a) = effective_opts.author.as_ref() {
            info.set("Author", Object::string_literal(a.clone()));
        }
    }

    // ---- Build/replace /Metadata stream on the catalog.
    let xmp = build_xmp_packet(&effective_opts);
    let xmp_bytes = xmp.into_bytes();
    stats.xmp_bytes = xmp_bytes.len();

    let cat_id = catalog_id(doc)?;

    // If a /Metadata reference already exists, overwrite the stream's content
    // and dict (keeps idempotency: same ObjectId reused).
    let existing_meta_ref = doc
        .get_dictionary(cat_id)
        .ok()
        .and_then(|c| c.get(b"Metadata").ok().cloned())
        .and_then(|o| o.as_reference().ok());

    let meta_id: ObjectId = match existing_meta_ref {
        Some(id) => {
            if let Ok(Object::Stream(s)) = doc.get_object_mut(id) {
                s.dict = dictionary! {
                    "Type" => Object::Name(b"Metadata".to_vec()),
                    "Subtype" => Object::Name(b"XML".to_vec()),
                    "Length" => Object::Integer(xmp_bytes.len() as i64),
                };
                s.content = xmp_bytes.clone();
            } else {
                // Existing /Metadata was malformed — replace with a fresh stream.
                let new_id = doc.add_object(Stream::new(
                    dictionary! {
                        "Type" => Object::Name(b"Metadata".to_vec()),
                        "Subtype" => Object::Name(b"XML".to_vec()),
                    },
                    xmp_bytes.clone(),
                ));
                let cat = doc.get_dictionary_mut(cat_id).map_err(|e| e.to_string())?;
                cat.set("Metadata", Object::Reference(new_id));
                return Ok(stats);
            }
            id
        }
        None => {
            let id = doc.add_object(Stream::new(
                dictionary! {
                    "Type" => Object::Name(b"Metadata".to_vec()),
                    "Subtype" => Object::Name(b"XML".to_vec()),
                },
                xmp_bytes.clone(),
            ));
            let cat = doc.get_dictionary_mut(cat_id).map_err(|e| e.to_string())?;
            cat.set("Metadata", Object::Reference(id));
            id
        }
    };
    let _ = meta_id; // suppress unused warning when both branches return.

    // ---- /ViewerPreferences << /DisplayDocTitle true >>.
    {
        let cat = doc.get_dictionary_mut(cat_id).map_err(|e| e.to_string())?;
        // Merge into existing ViewerPreferences if present.
        let mut vp = match cat.get(b"ViewerPreferences").ok().cloned() {
            Some(Object::Dictionary(d)) => d,
            _ => lopdf::Dictionary::new(),
        };
        vp.set("DisplayDocTitle", Object::Boolean(true));
        cat.set("ViewerPreferences", Object::Dictionary(vp));
        stats.viewer_prefs_set = true;
    }

    // ---- Ensure catalog /Lang.
    if let Some(lang) = effective_opts.fallback_lang.as_ref() {
        let cat = doc.get_dictionary_mut(cat_id).map_err(|e| e.to_string())?;
        if !cat.has(b"Lang") {
            cat.set("Lang", Object::string_literal(lang.clone()));
            stats.lang_set = true;
        }
    }

    Ok(stats)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn catalog_id(doc: &Document) -> Result<ObjectId, String> {
    doc.trailer
        .get(b"Root")
        .map_err(|e| e.to_string())?
        .as_reference()
        .map_err(|e| e.to_string())
}

fn read_info_title(doc: &Document) -> Option<String> {
    let info_ref = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
    let info = doc.get_dictionary(info_ref).ok()?;
    let raw = info.get(b"Title").ok()?;
    bytes_to_string(raw)
}

fn bytes_to_string(o: &Object) -> Option<String> {
    match o {
        Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

fn ensure_info_dict(doc: &mut Document) -> Result<(), String> {
    if doc.trailer.get(b"Info").is_err() {
        let id = doc.add_object(lopdf::Dictionary::new());
        doc.trailer.set("Info", Object::Reference(id));
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Cheap UTC formatter — avoids pulling chrono for this one use.
    // YYYY-MM-DDTHH:MM:SSZ, computed without timezone math.
    let (y, mo, d, h, mi, s) = unix_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn unix_to_ymdhms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // Simple civil calendar algo (Howard Hinnant). UTC.
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let h = (secs_of_day / 3600) as u32;
    let mi = ((secs_of_day % 3600) / 60) as u32;
    let s = (secs_of_day % 60) as u32;
    let z = days + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (if mo <= 2 { y + 1 } else { y }) as u32;
    (y, mo, d, h, mi, s)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn xmp_packet_contains_pdfuaid_part_1() {
        let xmp = build_xmp_packet(&MetadataOptions {
            title: Some("Spec".into()),
            ..Default::default()
        });
        assert!(xmp.contains("<pdfuaid:part>1</pdfuaid:part>"));
        assert!(xmp.contains("xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\""));
    }

    #[test]
    fn xmp_packet_escapes_title() {
        let xmp = build_xmp_packet(&MetadataOptions {
            title: Some("A & B <tag>".into()),
            ..Default::default()
        });
        assert!(xmp.contains("A &amp; B &lt;tag&gt;"));
    }

    #[test]
    fn apply_writes_metadata_and_viewerprefs() {
        let mut doc = blank_doc();
        let stats = apply_pdfua_metadata(
            &mut doc,
            &MetadataOptions {
                title: Some("Hello".into()),
                fallback_lang: Some("en-US".into()),
                timestamp: Some("2026-05-23T16:00:00Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(stats.viewer_prefs_set);
        assert!(stats.lang_set);
        assert!(stats.xmp_bytes > 200);

        let cat_id = catalog_id(&doc).unwrap();
        let cat = doc.get_dictionary(cat_id).unwrap();
        assert!(cat.has(b"Metadata"));
        assert!(cat.has(b"ViewerPreferences"));
        assert!(cat.has(b"Lang"));

        // ViewerPreferences carries DisplayDocTitle true.
        let vp = cat.get(b"ViewerPreferences").unwrap().as_dict().unwrap();
        match vp.get(b"DisplayDocTitle").unwrap() {
            Object::Boolean(b) => assert!(*b),
            _ => panic!("DisplayDocTitle not a bool"),
        }
    }

    #[test]
    fn apply_syncs_title_into_info_and_xmp() {
        let mut doc = blank_doc();
        apply_pdfua_metadata(
            &mut doc,
            &MetadataOptions {
                title: Some("Quarterly Report".into()),
                ..Default::default()
            },
        )
        .unwrap();

        // Info /Title should be set.
        let info_ref = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
        let info = doc.get_dictionary(info_ref).unwrap();
        let title = bytes_to_string(info.get(b"Title").unwrap()).unwrap();
        assert_eq!(title, "Quarterly Report");

        // XMP stream contains the title.
        let cat_id = catalog_id(&doc).unwrap();
        let meta_ref = doc
            .get_dictionary(cat_id)
            .unwrap()
            .get(b"Metadata")
            .unwrap()
            .as_reference()
            .unwrap();
        match doc.get_object(meta_ref).unwrap() {
            Object::Stream(s) => {
                let body = String::from_utf8_lossy(&s.content);
                assert!(body.contains("Quarterly Report"));
            }
            _ => panic!("Metadata not a stream"),
        }
    }

    #[test]
    fn apply_is_idempotent() {
        let mut doc = blank_doc();
        apply_pdfua_metadata(
            &mut doc,
            &MetadataOptions {
                title: Some("Doc".into()),
                fallback_lang: Some("en-US".into()),
                timestamp: Some("2026-05-23T16:00:00Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let meta_ref_1 = doc
            .get_dictionary(catalog_id(&doc).unwrap())
            .unwrap()
            .get(b"Metadata")
            .unwrap()
            .as_reference()
            .unwrap();
        let stream_count_1 = doc
            .objects
            .iter()
            .filter(|(_, o)| matches!(o, Object::Stream(_)))
            .count();

        apply_pdfua_metadata(
            &mut doc,
            &MetadataOptions {
                title: Some("Doc".into()),
                fallback_lang: Some("en-US".into()),
                timestamp: Some("2026-05-23T16:00:00Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let meta_ref_2 = doc
            .get_dictionary(catalog_id(&doc).unwrap())
            .unwrap()
            .get(b"Metadata")
            .unwrap()
            .as_reference()
            .unwrap();
        let stream_count_2 = doc
            .objects
            .iter()
            .filter(|(_, o)| matches!(o, Object::Stream(_)))
            .count();

        assert_eq!(
            meta_ref_1, meta_ref_2,
            "Metadata stream id should be reused"
        );
        assert_eq!(
            stream_count_1, stream_count_2,
            "no duplicate stream objects"
        );
    }

    #[test]
    fn apply_preserves_existing_catalog_lang() {
        let mut doc = blank_doc();
        {
            let cat_id = catalog_id(&doc).unwrap();
            let cat = doc.get_dictionary_mut(cat_id).unwrap();
            cat.set("Lang", Object::string_literal("fr-CA"));
        }
        let stats = apply_pdfua_metadata(
            &mut doc,
            &MetadataOptions {
                title: Some("X".into()),
                fallback_lang: Some("en-US".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!stats.lang_set, "should not overwrite existing /Lang");
        let cat = doc.get_dictionary(catalog_id(&doc).unwrap()).unwrap();
        let lang = bytes_to_string(cat.get(b"Lang").unwrap()).unwrap();
        assert_eq!(lang, "fr-CA");
    }

    #[test]
    fn unix_to_ymdhms_known_value() {
        // 1779580800 == 2026-05-24T00:00:00Z (verified via `date -u -r 1779580800`).
        let (y, mo, d, h, mi, s) = unix_to_ymdhms(1_779_580_800);
        assert_eq!((y, mo, d, h, mi, s), (2026, 5, 24, 0, 0, 0));
        // Epoch sanity: 1970-01-01T00:00:00Z.
        assert_eq!(unix_to_ymdhms(0), (1970, 1, 1, 0, 0, 0));
    }
}
