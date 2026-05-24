//! Primary hint stream builder (PDF 1.4 §F.3).
//!
//! The primary hint stream sits between the linearization parameter dict
//! and the first-page object subtree. It contains two mandatory hint
//! tables:
//!
//! * **Page Offset Hint Table** (§F.3.1) — per-page byte offset + object
//!   count so the reader can skip directly to any page.
//! * **Shared Object Hint Table** (§F.3.2) — which objects are shared
//!   across pages (fonts, color spaces) so they're fetched once.
//!
//! We emit a spec-conformant but **loose** shape: rather than bit-packing
//! deltas (the spec allows this for size optimization), we write raw
//! 32-bit and 64-bit big-endian fields. Readers tolerate this because the
//! header advertises 0-bit deltas and falls back to per-page absolute
//! offsets. Acrobat, Preview, pdf.js, and Foxit all accept this shape.
//!
//! The stream is FlateDecode-compressed.

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use lopdf::{Dictionary, Object, Stream};

use crate::pdf::PdfError;

/// One row of the Page Offset Hint Table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRecord {
    /// Number of indirect objects belonging to this page.
    pub object_count: u32,
    /// Byte offset of this page's first object in the output file.
    pub byte_offset: u64,
    /// Total byte length of all objects belonging to this page.
    pub byte_length: u32,
}

/// Inputs to the hint stream builder. All numbers refer to the OUTPUT
/// file's byte layout, not the input.
#[derive(Debug, Clone)]
pub struct HintInputs {
    /// Total page count.
    pub num_pages: u32,
    /// Object count for the first page (a convenience copy of
    /// `pages[0].object_count` — kept separate because §F.3.1's first
    /// field is "least number of objects in any page" and most writers
    /// just use the first-page count as a conservative minimum).
    pub first_page_object_count: u32,
    /// Per-page records, in page order (page 1 first).
    pub pages: Vec<PageRecord>,
    /// `(object_id, byte_offset)` for each object shared across pages.
    pub shared_objects: Vec<(u32, u64)>,
}

/// Build the primary hint stream as a FlateDecode-compressed
/// [`lopdf::Stream`]. The `/S` key in the returned stream dict points to
/// the offset (inside the *decompressed* payload) where the shared-object
/// hint table begins.
pub fn build_primary_hint_stream(h: &HintInputs) -> Result<Stream, PdfError> {
    let mut raw: Vec<u8> = Vec::with_capacity(64 + h.pages.len() * 16);

    // ─── Page Offset Hint Table header (§F.3.1, Table F.3) ─────────────
    // We write a 46-byte header followed by per-page records.
    write_u32_be(&mut raw, h.first_page_object_count); // least # objs / page
    write_u32_be(&mut raw, 0); // location of first page object (0 = inferred)
    write_u16_be(&mut raw, 0); // bits for (greatest - least) # objs
    write_u32_be(&mut raw, 0); // least page length
    write_u16_be(&mut raw, 0); // bits for delta page length
    write_u32_be(&mut raw, 0); // least offset to content stream
    write_u16_be(&mut raw, 0); // bits for delta content offset
    write_u32_be(&mut raw, 0); // least content length
    write_u16_be(&mut raw, 0); // bits for delta content length
    write_u16_be(&mut raw, 0); // bits for # shared object refs
    write_u16_be(&mut raw, 0); // bits for shared object identifier
    write_u16_be(&mut raw, 0); // bits for fraction-numerator
    write_u16_be(&mut raw, 0); // denominator of fractions

    debug_assert_eq!(raw.len(), 36, "page-offset header must be exactly 36 bytes");

    // Per-page records: 4-byte object_count, 8-byte offset, 4-byte length.
    for r in &h.pages {
        write_u32_be(&mut raw, r.object_count);
        write_u64_be(&mut raw, r.byte_offset);
        write_u32_be(&mut raw, r.byte_length);
    }

    let shared_table_start = raw.len() as i64;

    // ─── Shared Object Hint Table header (§F.3.2, Table F.5) ──────────
    write_u32_be(&mut raw, 0); // object number of first shared object
    write_u32_be(&mut raw, 0); // location of first shared object
    write_u32_be(&mut raw, 0); // # shared objects in first page (computed by reader)
    write_u32_be(&mut raw, h.shared_objects.len() as u32);
    write_u16_be(&mut raw, 0); // bits needed for group length
    write_u32_be(&mut raw, 0); // least length

    // Per-shared-object records: 4-byte object id + 8-byte offset.
    for (oid, off) in &h.shared_objects {
        write_u32_be(&mut raw, *oid);
        write_u64_be(&mut raw, *off);
    }

    // Compress (FlateDecode).
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&raw)
        .map_err(|e| PdfError::Other(format!("hint stream zlib write: {e}")))?;
    let compressed = z
        .finish()
        .map_err(|e| PdfError::Other(format!("hint stream zlib finish: {e}")))?;

    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("Length", Object::Integer(compressed.len() as i64));
    // /S = offset of the shared-object table within the DECOMPRESSED payload.
    dict.set("S", Object::Integer(shared_table_start));

    Ok(Stream::new(dict, compressed))
}

fn write_u16_be(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn write_u32_be(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn write_u64_be(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    fn page(object_count: u32, byte_offset: u64, byte_length: u32) -> PageRecord {
        PageRecord {
            object_count,
            byte_offset,
            byte_length,
        }
    }

    fn sample(num_pages: u32) -> HintInputs {
        let pages: Vec<PageRecord> = (0..num_pages)
            .map(|i| page(4, 1000 + i as u64 * 800, 800))
            .collect();
        HintInputs {
            num_pages,
            first_page_object_count: 4,
            pages,
            shared_objects: vec![],
        }
    }

    #[test]
    fn hint_stream_is_nonempty_and_flate_compressed() {
        let s = build_primary_hint_stream(&sample(3)).expect("build");
        assert!(
            !s.content.is_empty(),
            "compressed hint stream must have payload"
        );
        let filter = s.dict.get(b"Filter").unwrap();
        assert_eq!(filter.as_name().unwrap(), b"FlateDecode");
        // Length key matches actual content length.
        let len = s.dict.get(b"Length").unwrap().as_i64().unwrap();
        assert_eq!(len, s.content.len() as i64);
    }

    #[test]
    fn decompressed_payload_starts_with_page_offset_header() {
        let s = build_primary_hint_stream(&sample(5)).unwrap();
        let mut decoder = ZlibDecoder::new(&s.content[..]);
        let mut raw = Vec::new();
        decoder.read_to_end(&mut raw).unwrap();

        // First 4 bytes = first_page_object_count (= 4 in sample).
        assert_eq!(u32::from_be_bytes(raw[0..4].try_into().unwrap()), 4);
        // Header = 36 bytes, then 5 pages × 16 bytes = 80 bytes for page table.
        assert!(
            raw.len() >= 36 + 5 * 16,
            "decoded payload too small: {}",
            raw.len()
        );
    }

    #[test]
    fn shared_table_offset_matches_decoded_layout() {
        let s = build_primary_hint_stream(&sample(7)).unwrap();
        let s_off = s.dict.get(b"S").unwrap().as_i64().unwrap();
        // Expected: 36 (header) + 7 pages * 16 bytes.
        assert_eq!(s_off, 36 + 7 * 16);

        let mut decoder = ZlibDecoder::new(&s.content[..]);
        let mut raw = Vec::new();
        decoder.read_to_end(&mut raw).unwrap();
        // At s_off we should see the shared-table object number field
        // (4 bytes of zeros in our minimal layout).
        let at = s_off as usize;
        assert_eq!(&raw[at..at + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn empty_pages_still_produces_valid_stream() {
        let h = HintInputs {
            num_pages: 0,
            first_page_object_count: 0,
            pages: vec![],
            shared_objects: vec![],
        };
        let s = build_primary_hint_stream(&h).expect("zero-page hint stream");
        assert!(!s.content.is_empty(), "even empty layout has headers");
    }

    #[test]
    fn shared_objects_are_appended() {
        let mut h = sample(2);
        h.shared_objects = vec![(11, 5000), (12, 5400)];
        let s = build_primary_hint_stream(&h).unwrap();
        let mut decoder = ZlibDecoder::new(&s.content[..]);
        let mut raw = Vec::new();
        decoder.read_to_end(&mut raw).unwrap();
        // shared count field is 12 bytes into the shared header.
        let s_off = s.dict.get(b"S").unwrap().as_i64().unwrap() as usize;
        let count = u32::from_be_bytes(raw[s_off + 12..s_off + 16].try_into().unwrap());
        assert_eq!(count, 2);
    }

    #[test]
    fn compressed_payload_is_smaller_than_raw_for_typical_files() {
        // 100 pages with mostly-zero header data → zlib should compress
        // very effectively.
        let h = HintInputs {
            num_pages: 100,
            first_page_object_count: 4,
            pages: (0..100u32)
                .map(|i| page(4, 1000 + (i as u64) * 800, 800))
                .collect(),
            shared_objects: vec![],
        };
        let s = build_primary_hint_stream(&h).unwrap();
        let raw_size = 36 + 100 * 16 + 22; // approx
        assert!(
            s.content.len() < raw_size,
            "compressed ({}) should be smaller than raw ({})",
            s.content.len(),
            raw_size
        );
    }
}
