//! Linearization parameter dictionary builder (PDF 1.4 §F.2.1).
//!
//! Object 1 of a linearized PDF is the linearization parameter dictionary.
//! It tells a streaming reader four things up-front:
//!
//! * `/L` — the total length of the file.
//! * `/H` — `[offset length]` of the primary hint stream.
//! * `/O` — the indirect object ID of the first-page object.
//! * `/E` — the byte offset of the end of the first-page object subtree.
//! * `/N` — total page count.
//! * `/T` — byte offset of the main cross-reference section.
//!
//! Given those, the reader can fetch only the file prefix `[0..E]` to render
//! page 1 and lazily request the rest. This module is pure: no I/O, no
//! parsing — it's the helper Task 6 will call when emitting the final file.

use lopdf::{Dictionary, Object};

/// All the byte-level addresses the writer must know before it can produce
/// the parameter dictionary. Every value is a byte offset or count and is
/// computed by the writer in [`crate::pdf::streamline::linearize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinearizationParams {
    /// Total length of the output file, bytes.
    pub file_length: i64,
    /// Byte offset of the primary hint stream within the output file.
    pub hint_stream_offset: i64,
    /// Length of the primary hint stream, bytes.
    pub hint_stream_length: i64,
    /// Indirect object ID of the first-page object.
    pub first_page_obj_id: u32,
    /// Byte offset at which the first-page object subtree ends (the `/E`
    /// key — the only field a reader needs to know how much of the file to
    /// download before painting page 1).
    pub end_of_first_page_offset: i64,
    /// Page count.
    pub num_pages: i64,
    /// Byte offset of the main cross-reference table.
    pub main_xref_offset: i64,
}

/// Build the linearization parameter dictionary.
///
/// Conforms to PDF 1.4 §F.2.1 Table F.1. The returned [`Dictionary`] is
/// ready to be wrapped in an indirect object (typically `1 0 obj`) and
/// written immediately after the file header.
pub fn build_param_dict(p: &LinearizationParams) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Linearized", Object::Real(1.0));
    d.set("L", Object::Integer(p.file_length));
    d.set(
        "H",
        Object::Array(vec![
            Object::Integer(p.hint_stream_offset),
            Object::Integer(p.hint_stream_length),
        ]),
    );
    d.set("O", Object::Integer(p.first_page_obj_id as i64));
    d.set("E", Object::Integer(p.end_of_first_page_offset));
    d.set("N", Object::Integer(p.num_pages));
    d.set("T", Object::Integer(p.main_xref_offset));
    d
}

/// Convenience helper: the smallest legal-looking sample dictionary, used
/// in fixtures + downstream tests so call-sites don't repeat 7 fields.
#[cfg(test)]
pub fn sample_params() -> LinearizationParams {
    LinearizationParams {
        file_length: 100_000,
        hint_stream_offset: 200,
        hint_stream_length: 300,
        first_page_obj_id: 5,
        end_of_first_page_offset: 50_000,
        num_pages: 10,
        main_xref_offset: 99_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i64_of(d: &Dictionary, key: &[u8]) -> i64 {
        let obj = d.get(key).unwrap();
        match obj {
            Object::Integer(n) => *n,
            Object::Real(r) => *r as i64,
            other => panic!("expected numeric, got {other:?}"),
        }
    }

    #[test]
    fn param_dict_has_all_required_keys() {
        let dict = build_param_dict(&sample_params());
        // Linearized may be encoded as Real 1.0 or Integer 1 — both are
        // spec-legal. We compare numerically.
        assert_eq!(i64_of(&dict, b"Linearized"), 1);
        assert_eq!(i64_of(&dict, b"L"), 100_000);
        let h = dict.get(b"H").unwrap().as_array().unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].as_i64().unwrap(), 200);
        assert_eq!(h[1].as_i64().unwrap(), 300);
        assert_eq!(i64_of(&dict, b"O"), 5);
        assert_eq!(i64_of(&dict, b"E"), 50_000);
        assert_eq!(i64_of(&dict, b"N"), 10);
        assert_eq!(i64_of(&dict, b"T"), 99_000);
    }

    #[test]
    fn param_dict_preserves_extreme_file_lengths() {
        let mut p = sample_params();
        p.file_length = 4_000_000_000; // 4 GB — within i64, beyond i32.
        p.main_xref_offset = 3_999_900_000;
        let dict = build_param_dict(&p);
        assert_eq!(i64_of(&dict, b"L"), 4_000_000_000);
        assert_eq!(i64_of(&dict, b"T"), 3_999_900_000);
    }

    #[test]
    fn param_dict_with_empty_hint_stream_still_emits_h_array() {
        let mut p = sample_params();
        p.hint_stream_offset = 0;
        p.hint_stream_length = 0;
        let dict = build_param_dict(&p);
        // /H must be present even if the hint stream is empty — readers
        // that don't see /H reject the file as malformed.
        let h = dict.get(b"H").unwrap().as_array().unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].as_i64().unwrap(), 0);
        assert_eq!(h[1].as_i64().unwrap(), 0);
    }

    #[test]
    fn param_dict_single_page_file_is_legal() {
        let mut p = sample_params();
        p.num_pages = 1;
        let dict = build_param_dict(&p);
        assert_eq!(i64_of(&dict, b"N"), 1);
    }

    #[test]
    fn dict_keys_round_trip() {
        // Sanity: every key we set comes back out under the same name.
        let dict = build_param_dict(&sample_params());
        for key in [&b"Linearized"[..], b"L", b"H", b"O", b"E", b"N", b"T"] {
            assert!(
                dict.get(key).is_ok(),
                "missing key /{}",
                std::str::from_utf8(key).unwrap()
            );
        }
    }
}
