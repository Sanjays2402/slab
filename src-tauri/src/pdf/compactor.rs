//! v3.6.0 Compactor — real PDF size reduction.
//!
//! Distinct from `pdf::compress` (which only re-flates uncompressed streams
//! and saves <2% on a scanned legal PDF). The Compactor's job is the actual
//! "Reduce File Size" feature Adobe Acrobat Pro charges $239/yr for:
//!
//! * Walk every image XObject (`/Subtype /Image`).
//! * Downsample to the preset's target DPI (Screen 72 / eBook 150 /
//!   Printer 300 / Prepress 300, with quality and mono-DPI variants).
//! * Re-encode color/grayscale as JPEG; keep monochrome as-is.
//! * Optionally drop `/Thumb` entries, `/Metadata` (XMP), embedded files,
//!   and JS / `/AA` actions.
//! * Run lopdf's stream re-flate at the end so dropped content actually
//!   shrinks the output.
//!
//! This file currently implements the foundation: a read-only image
//! enumerator (`list_image_xobjects`) and the data types every later slice
//! will need. Slice 3 adds presets + an estimate (dry-run); Slice 4 wires
//! the actual decode/resize/re-encode loop.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

/// One image XObject we may want to recompress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub object_id_num: u32,
    pub object_id_gen: u16,
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    /// `"DCTDecode"`, `"FlateDecode"`, `"CCITTFaxDecode"`, `"JBIG2Decode"`,
    /// `"JPXDecode"`, etc. `None` means the stream had no `/Filter` (rare).
    pub filter: Option<String>,
    /// `"DeviceRGB"`, `"DeviceGray"`, `"DeviceCMYK"`, `"Indexed"`,
    /// `"ICCBased"`, etc. `None` if not a Name / first array element.
    pub color_space: Option<String>,
    /// On-disk byte length of the (encoded) stream.
    pub byte_size: usize,
}

fn first_name(obj: &Object) -> Option<String> {
    match obj {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::Array(a) => a.first().and_then(|x| {
            x.as_name()
                .ok()
                .map(|n| String::from_utf8_lossy(n).into_owned())
        }),
        _ => None,
    }
}

/// Enumerate every image XObject in `doc`. Pure / read-only.
///
/// Excludes Form XObjects, soft-mask alphas referenced via `/SMask`
/// (those are still images, but they piggyback on the parent and we'll
/// rewrite them together in Slice 4).
pub fn list_image_xobjects(doc: &Document) -> Vec<ImageRef> {
    let mut out = Vec::new();
    for (id, obj) in &doc.objects {
        let Ok(stream) = obj.as_stream() else {
            continue;
        };
        let dict = &stream.dict;
        let subtype = dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok());
        if subtype != Some(b"Image".as_ref()) {
            continue;
        }
        let width = dict
            .get(b"Width")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0) as u32;
        let height = dict
            .get(b"Height")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0) as u32;
        let bpc = dict
            .get(b"BitsPerComponent")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(8) as u8;
        let filter = dict.get(b"Filter").ok().and_then(first_name);
        let color_space = dict.get(b"ColorSpace").ok().and_then(first_name);
        out.push(ImageRef {
            object_id_num: id.0,
            object_id_gen: id.1,
            width,
            height,
            bits_per_component: bpc,
            filter,
            color_space,
            byte_size: stream.content.len(),
        });
    }
    out
}

/// Sum of encoded image bytes — what we'll be working with as the
/// "compactable surface". A doc with 0 here can't meaningfully shrink
/// via the image path (it might still shrink via metadata/font drops).
pub fn total_image_bytes(refs: &[ImageRef]) -> u64 {
    refs.iter().map(|r| r.byte_size as u64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::{make_n_page_pdf, make_pdf_with_image};

    #[test]
    fn list_image_xobjects_finds_one() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("i.pdf");
        make_pdf_with_image(&p, 600, 400);
        let doc = Document::load(&p).unwrap();
        let images = list_image_xobjects(&doc);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width, 600);
        assert_eq!(images[0].height, 400);
        assert_eq!(images[0].filter.as_deref(), Some("DCTDecode"));
        assert_eq!(images[0].color_space.as_deref(), Some("DeviceRGB"));
        assert_eq!(images[0].bits_per_component, 8);
        assert!(images[0].byte_size > 100);
    }

    #[test]
    fn list_image_xobjects_empty_for_text_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_n_page_pdf(&p, 2);
        let doc = Document::load(&p).unwrap();
        assert!(list_image_xobjects(&doc).is_empty());
    }

    #[test]
    fn total_image_bytes_sums_all_streams() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("i.pdf");
        make_pdf_with_image(&p, 800, 800);
        let doc = Document::load(&p).unwrap();
        let imgs = list_image_xobjects(&doc);
        let total = total_image_bytes(&imgs);
        assert!(total > 1000, "expected >1KB of image data, got {}", total);
        assert_eq!(total, imgs[0].byte_size as u64);
    }
}
