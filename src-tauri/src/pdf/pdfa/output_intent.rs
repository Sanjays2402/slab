//! OutputIntent injection — ISO 19005-2 §6.2.2.
//!
//! Every PDF/A document MUST carry an `/OutputIntents` array on its
//! catalog with at least one entry whose `/S` is `/GTS_PDFA1` and
//! whose `/DestOutputProfile` is a stream containing an ICC profile.
//!
//! This module knows how to:
//! 1. Add the sRGB v4 ICC profile (from `super::icc`) as a flate-
//!    compressed PDF stream.
//! 2. Build the OutputIntent dictionary that references that stream.
//! 3. Splice the dictionary into the catalog's `/OutputIntents` array,
//!    creating the array if absent or appending only if no matching
//!    entry exists (idempotent).
//! 4. Set the catalog's `/Metadata` reference to an XMP packet stream
//!    built by `super::xmp`.

use super::icc::{SRGB_INFO, SRGB_OUTPUT_CONDITION_IDENTIFIER, SRGB_V4_ICC};
use super::xmp::{build_xmp_packet, XmpMetadata};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputIntentError {
    #[error("malformed document: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, Default)]
pub struct InjectReport {
    pub added_output_intent: bool,
    pub added_xmp_metadata: bool,
    pub icc_stream_id: Option<ObjectId>,
    pub xmp_stream_id: Option<ObjectId>,
}

/// Inject the OutputIntent + XMP metadata required for PDF/A.
///
/// Idempotent — if a matching OutputIntent (same `/OutputConditionIdentifier`)
/// already exists, it's left in place. The XMP stream is always rewritten
/// because the caller controls the metadata fields.
pub fn inject_output_intent_and_metadata(
    doc: &mut Document,
    meta: &XmpMetadata,
) -> Result<InjectReport, OutputIntentError> {
    let mut report = InjectReport::default();

    let catalog_id = catalog_id(doc)?;

    // 1) XMP metadata stream — always (re)create.
    let xmp_bytes = build_xmp_packet(meta);
    let xmp_dict = dictionary! {
        "Type" => "Metadata",
        "Subtype" => "XML",
        // Per PDF/A spec, the XMP packet must NOT be compressed and MUST NOT
        // be encrypted. We rely on the lopdf default behaviour of leaving
        // explicitly-supplied stream bytes verbatim.
        "Length" => xmp_bytes.len() as i64,
    };
    let xmp_stream = Stream::new(xmp_dict, xmp_bytes).with_compression(false);
    let xmp_id = doc.add_object(Object::Stream(xmp_stream));
    report.xmp_stream_id = Some(xmp_id);

    {
        let catalog = doc
            .get_object_mut(catalog_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| OutputIntentError::Malformed(format!("catalog: {e}")))?;
        catalog.set("Metadata", Object::Reference(xmp_id));
        report.added_xmp_metadata = true;
    }

    // 2) OutputIntent — only add if no GTS_PDFA1 entry exists yet.
    if has_pdfa_output_intent(doc, catalog_id)? {
        return Ok(report);
    }

    // ICC profile stream. The Filter is /FlateDecode and we hand-
    // compress the bytes ourselves so lopdf doesn't double-compress
    // or skip compression based on size heuristics.
    let icc_bytes = flate_compress(SRGB_V4_ICC);
    let icc_dict = dictionary! {
        "N" => 3i64, // sRGB has 3 colour components
        "Filter" => "FlateDecode",
        "Length" => icc_bytes.len() as i64,
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes).with_compression(false);
    let icc_id = doc.add_object(Object::Stream(icc_stream));
    report.icc_stream_id = Some(icc_id);

    let oi_dict = dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::string_literal(SRGB_OUTPUT_CONDITION_IDENTIFIER),
        "Info" => Object::string_literal(SRGB_INFO),
        "DestOutputProfile" => Object::Reference(icc_id),
    };

    {
        let catalog = doc
            .get_object_mut(catalog_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| OutputIntentError::Malformed(format!("catalog: {e}")))?;

        let existing = catalog.get(b"OutputIntents").ok().cloned();
        let new_array = match existing {
            Some(Object::Array(mut arr)) => {
                arr.push(Object::Dictionary(oi_dict));
                Object::Array(arr)
            }
            _ => Object::Array(vec![Object::Dictionary(oi_dict)]),
        };
        catalog.set("OutputIntents", new_array);
        report.added_output_intent = true;
    }

    Ok(report)
}

fn catalog_id(doc: &Document) -> Result<ObjectId, OutputIntentError> {
    match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => Ok(*id),
        _ => Err(OutputIntentError::Malformed("Root missing".into())),
    }
}

fn has_pdfa_output_intent(doc: &Document, catalog_id: ObjectId) -> Result<bool, OutputIntentError> {
    let catalog = doc
        .get_object(catalog_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| OutputIntentError::Malformed(format!("catalog: {e}")))?;
    let Ok(arr) = catalog.get(b"OutputIntents").and_then(|o| o.as_array()) else {
        return Ok(false);
    };
    for entry in arr {
        let dict = match entry {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
                Ok(d) => d.clone(),
                Err(_) => continue,
            },
            _ => continue,
        };
        if matches_gts_pdfa1(&dict) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn matches_gts_pdfa1(d: &Dictionary) -> bool {
    let Ok(s) = d.get(b"S") else { return false };
    match s {
        Object::Name(n) => n == b"GTS_PDFA1",
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::pdfa::ConformanceLevel;
    use lopdf::{dictionary, Document, Object};

    fn minimal_doc() -> Document {
        // Build the smallest possible /Catalog + /Pages structure so we
        // can test catalog mutations end-to-end.
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![]),
            "Count" => 0i64,
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    #[test]
    fn injects_xmp_metadata_reference_into_catalog() {
        let mut doc = minimal_doc();
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
                .unwrap();
        assert!(report.added_xmp_metadata);
        let cat = doc.catalog().unwrap();
        let meta_ref = cat.get(b"Metadata").unwrap();
        assert!(matches!(meta_ref, Object::Reference(_)));
    }

    #[test]
    fn injects_output_intent_with_gts_pdfa1() {
        let mut doc = minimal_doc();
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
                .unwrap();
        assert!(report.added_output_intent);
        assert!(report.icc_stream_id.is_some());

        let cat = doc.catalog().unwrap();
        let arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let entry = arr[0].as_dict().unwrap();
        assert_eq!(entry.get(b"S").unwrap().as_name().unwrap(), b"GTS_PDFA1");
    }

    #[test]
    fn output_intent_carries_srgb_identifier() {
        let mut doc = minimal_doc();
        inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
            .unwrap();
        let cat = doc.catalog().unwrap();
        let arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        let entry = arr[0].as_dict().unwrap();
        let id = entry.get(b"OutputConditionIdentifier").unwrap();
        assert_eq!(id.as_str().unwrap(), b"sRGB IEC61966-2.1");
    }

    #[test]
    fn icc_stream_is_flate_compressed_and_decompresses_to_real_profile() {
        use flate2::read::ZlibDecoder;
        use std::io::Read;

        let mut doc = minimal_doc();
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
                .unwrap();

        let icc_id = report.icc_stream_id.unwrap();
        let stream = doc.get_object(icc_id).unwrap().as_stream().unwrap();
        assert_eq!(
            stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        assert_eq!(stream.dict.get(b"N").unwrap().as_i64().unwrap(), 3);

        let mut z = ZlibDecoder::new(&stream.content[..]);
        let mut out = Vec::new();
        z.read_to_end(&mut out).unwrap();
        assert_eq!(out, SRGB_V4_ICC);
    }

    #[test]
    fn idempotent_when_pdfa1_intent_already_present() {
        let mut doc = minimal_doc();
        inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
            .unwrap();
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
                .unwrap();
        // Second call leaves OutputIntents alone (was already present)
        // but re-writes the XMP packet.
        assert!(!report.added_output_intent);
        assert!(report.added_xmp_metadata);
        let cat = doc.catalog().unwrap();
        let arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
    }

    #[test]
    fn appends_to_existing_non_pdfa_output_intents_array() {
        let mut doc = minimal_doc();
        // Pre-seed a non-PDFA OutputIntent (e.g. GTS_PDFX).
        {
            let cat_id = catalog_id(&doc).unwrap();
            let cat = doc.get_object_mut(cat_id).unwrap().as_dict_mut().unwrap();
            cat.set(
                "OutputIntents",
                Object::Array(vec![Object::Dictionary(dictionary! {
                    "Type" => "OutputIntent",
                    "S" => "GTS_PDFX",
                })]),
            );
        }
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
                .unwrap();
        assert!(report.added_output_intent);
        let cat = doc.catalog().unwrap();
        let arr = cat.get(b"OutputIntents").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn xmp_stream_bytes_contain_pdfaid_marker() {
        let mut doc = minimal_doc();
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A3b))
                .unwrap();
        let xmp_id = report.xmp_stream_id.unwrap();
        let stream = doc.get_object(xmp_id).unwrap().as_stream().unwrap();
        let s = std::str::from_utf8(&stream.content).unwrap();
        assert!(s.contains("<pdfaid:part>3</pdfaid:part>"));
        assert!(s.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
    }

    #[test]
    fn xmp_stream_is_not_flate_compressed() {
        // PDF/A spec forbids compression on the metadata stream.
        let mut doc = minimal_doc();
        let report =
            inject_output_intent_and_metadata(&mut doc, &XmpMetadata::new(ConformanceLevel::A2b))
                .unwrap();
        let xmp_id = report.xmp_stream_id.unwrap();
        let stream = doc.get_object(xmp_id).unwrap().as_stream().unwrap();
        assert!(stream.dict.get(b"Filter").is_err());
    }

    #[test]
    fn flate_compress_is_lossless_round_trip() {
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        let original = b"the quick brown fox jumps over the lazy dog";
        let compressed = flate_compress(original);
        assert_ne!(compressed, original);
        let mut z = ZlibDecoder::new(&compressed[..]);
        let mut out = Vec::new();
        z.read_to_end(&mut out).unwrap();
        assert_eq!(out, original);
    }
}
