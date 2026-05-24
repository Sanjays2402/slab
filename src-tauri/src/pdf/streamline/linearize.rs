//! End-to-end PDF linearizer (Task 6 of the v3.13.0 Streamline plan).
//!
//! Given an input PDF, produce a byte-equivalent output that follows the
//! Fast Web View layout from PDF 1.4 §F:
//!
//! ```text
//! %PDF-1.7
//! <binary mark>
//! N+1 0 obj          <- linearization parameter dict (well-padded for in-place rewrite)
//!   << /Linearized 1 /L … /H […] /O … /E … /N … /T … >>
//! endobj
//! <first-page reachable objects>
//! N+2 0 obj          <- primary hint stream
//!   << /Length … /S … >> stream … endstream
//! endobj
//! <remaining objects>
//! xref
//! …
//! trailer << … >>
//! startxref <offset>
//! %%EOF
//! ```
//!
//! The implementation does two passes:
//!  1. Layout pass — write a fully-padded placeholder lin-dict, then objects,
//!     learn every byte offset.
//!  2. Rewrite pass — replace the placeholder with the real lin-dict (same
//!     serialized length because the placeholder used worst-case-width values).
//!
//! Caveats accepted by v3.13.0:
//!  - Encrypted PDFs are rejected with a clear error.
//!  - The hint stream uses the loose 0-bit-delta shape from
//!    `hint_stream.rs` (Acrobat/Preview/pdf.js all accept it).
//!  - We do not (yet) emit a cross-reference stream — we use the classic
//!    `xref` table to keep the writer simple and reader-compatible.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::pdf::atomic_save;
use crate::pdf::PdfError;

use super::depgraph::first_page_reachable;
use super::dto::{LinearizationStatus, LinearizeReport, LinearizeStats};
use super::hint_stream::{build_primary_hint_stream, HintInputs, PageRecord};
use super::param_dict::{build_param_dict, LinearizationParams};

/// Produce a linearized copy of `input` at `output`. Returns a populated
/// [`LinearizeReport`] including before/after stats.
pub fn linearize_pdf(input: &Path, output: &Path) -> Result<LinearizeReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let before_bytes = fs::read(input)?;
    let before_total = before_bytes.len() as u64;

    let doc = Document::load_mem(&before_bytes)
        .map_err(|e| PdfError::Other(format!("damaged or unsupported PDF (parse failed): {e}")))?;

    if doc.is_encrypted() {
        return Err(PdfError::Other(
            "encrypted PDFs are not supported by the linearizer in v3.13.0 \
             — decrypt the file first via the unlock panel"
                .into(),
        ));
    }

    let page_ids: Vec<ObjectId> = doc.page_iter().collect();
    let num_pages = page_ids.len();
    if num_pages == 0 {
        return Err(PdfError::Other(
            "PDF has no pages — nothing to linearize".into(),
        ));
    }
    let first_page_id = page_ids[0];

    // First-page reachable set (catalog, page-tree, page-1, resources, …).
    let first_set = first_page_reachable(&doc, 1);

    // Partition object IDs: first_chunk = first_set, rest_chunk = everything else.
    let mut first_chunk: Vec<ObjectId> = first_set.iter().copied().collect();
    first_chunk.sort_by_key(|id| (id.0, id.1));
    let mut rest_chunk: Vec<ObjectId> = doc
        .objects
        .keys()
        .copied()
        .filter(|id| !first_set.contains(id))
        .collect();
    rest_chunk.sort_by_key(|id| (id.0, id.1));

    // Allocate IDs for our two synthetic objects.
    let max_existing = doc.objects.keys().map(|id| id.0).max().unwrap_or(0);
    let lin_dict_id = (max_existing + 1, 0);
    let hint_stream_id = (max_existing + 2, 0);

    // --- Layout pass ------------------------------------------------------
    // Use worst-case-width values so the placeholder is the same serialized
    // length as the real dict.
    let pad = LinearizationParams {
        file_length: 9_999_999_999,
        hint_stream_offset: 9_999_999_999,
        hint_stream_length: 9_999_999_999,
        first_page_obj_id: max_existing + 2, // fits any plausible value
        end_of_first_page_offset: 9_999_999_999,
        num_pages: 9_999_999,
        main_xref_offset: 9_999_999_999,
    };
    let _ = build_param_dict(&pad); // (keep import path live; manual writer used below)

    let mut buf: Vec<u8> = Vec::with_capacity(before_bytes.len() + 8192);
    // Header: PDF-1.7 + 4-byte binary marker (per §7.5.2).
    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    // Track byte offsets per object id for the xref table.
    let mut offsets: BTreeMap<u32, usize> = BTreeMap::new();

    // 1. Linearization parameter dictionary FIRST (per §F.2.1).
    let lin_offset = buf.len();
    write_lin_dict_manually(&mut buf, lin_dict_id, &pad);
    let lin_placeholder_end = buf.len();
    let lin_placeholder_len = lin_placeholder_end - lin_offset;
    offsets.insert(lin_dict_id.0, lin_offset);

    // 2. First-page-reachable objects, sorted by ID for determinism.
    let mut first_page_offsets: Vec<(u32, u64, u64)> = Vec::with_capacity(first_chunk.len());
    for id in &first_chunk {
        let obj_owned = doc.get_object(*id).cloned();
        if let Ok(obj) = obj_owned {
            let off = buf.len();
            let wrote = write_indirect_object(&mut buf, *id, &obj)?;
            if !wrote {
                continue;
            }
            let end = buf.len();
            offsets.insert(id.0, off);
            first_page_offsets.push((id.0, off as u64, (end - off) as u64));
        }
    }
    let end_of_first_page = buf.len();

    // 3. Primary hint stream.
    let page_records: Vec<PageRecord> = (0..num_pages as u32)
        .map(|i| {
            if i == 0 {
                let bytes: u64 = first_page_offsets.iter().map(|(_, _, l)| *l).sum();
                PageRecord {
                    object_count: first_page_offsets.len() as u32,
                    byte_offset: lin_offset as u64,
                    byte_length: bytes as u32,
                }
            } else {
                PageRecord {
                    object_count: 0,
                    byte_offset: 0,
                    byte_length: 0,
                }
            }
        })
        .collect();

    let hint_inputs = HintInputs {
        num_pages: num_pages as u32,
        first_page_object_count: first_page_offsets.len() as u32,
        pages: page_records,
        shared_objects: vec![],
    };
    let hint_stream: Stream = build_primary_hint_stream(&hint_inputs)?;

    let hint_offset = buf.len();
    let _ = write_indirect_object(&mut buf, hint_stream_id, &Object::Stream(hint_stream))?;
    let hint_end = buf.len();
    offsets.insert(hint_stream_id.0, hint_offset);
    let hint_length = (hint_end - hint_offset) as i64;

    // 4. Remaining objects.
    for id in &rest_chunk {
        let obj_owned = doc.get_object(*id).cloned();
        if let Ok(obj) = obj_owned {
            let off = buf.len();
            let wrote = write_indirect_object(&mut buf, *id, &obj)?;
            if wrote {
                offsets.insert(id.0, off);
            }
        }
    }

    // 5. Main xref + trailer.
    let xref_offset = buf.len();
    let max_id_total = (max_existing + 2).max(offsets.keys().copied().max().unwrap_or(0));
    write_xref_table(&mut buf, &offsets, max_id_total);
    write_trailer(&mut buf, &doc.trailer, max_id_total + 1)?;
    writeln!(buf, "\nstartxref\n{xref_offset}\n%%EOF").map_err(io_err)?;

    // --- Rewrite pass: replace the placeholder lin dict ------------------
    let real_params = LinearizationParams {
        file_length: buf.len() as i64,
        hint_stream_offset: hint_offset as i64,
        hint_stream_length: hint_length,
        first_page_obj_id: first_page_id.0,
        end_of_first_page_offset: end_of_first_page as i64,
        num_pages: num_pages as i64,
        main_xref_offset: xref_offset as i64,
    };
    let real_dict = build_param_dict(&real_params);
    let _ = real_dict;
    rewrite_lin_dict_manual(
        &mut buf,
        lin_offset,
        lin_placeholder_len,
        lin_dict_id,
        &real_params,
    )?;

    // Persist atomically (so a crash mid-write doesn't replace the original).
    atomic_save(output, &buf)?;

    let after_total = buf.len() as u64;
    Ok(LinearizeReport {
        input_path: input.display().to_string(),
        output_path: Some(output.display().to_string()),
        before: LinearizeStats {
            first_page_prefix_bytes: before_total,
            total_bytes: before_total,
            hint_stream_bytes: 0,
            page_count: num_pages as u32,
        },
        after: Some(LinearizeStats {
            first_page_prefix_bytes: end_of_first_page as u64,
            total_bytes: after_total,
            hint_stream_bytes: hint_length as u64,
            page_count: num_pages as u32,
        }),
        status: LinearizationStatus::Linearized,
        warnings: vec![],
    })
}

fn io_err(e: std::io::Error) -> PdfError {
    PdfError::Io(e)
}

/// Serialize a single indirect object using lopdf as the source of truth.
///
/// We build a tiny throwaway `Document`, insert the object with the same
/// id, save the whole thing to a buffer, and then extract just the
/// `N G obj … endobj` slice. Indirect references inside the object are
/// preserved as `n g R` text — they remain valid in the final document
/// because we did not renumber anything.
///
/// Returns `Ok(true)` when the object was written, `Ok(false)` when lopdf
/// intentionally skipped it (object streams, XRef streams, legacy
/// linearization dicts — all of which we don't want in our classic-xref
/// linearized output anyway).
fn write_indirect_object(out: &mut Vec<u8>, id: ObjectId, obj: &Object) -> Result<bool, PdfError> {
    // Skip objects that lopdf's `save_internal` filters out — they would
    // never appear in a classic-xref output.
    if let Ok(name) = obj.type_name() {
        if name == b"ObjStm" || name == b"XRef" || name == b"Linearized" {
            return Ok(false);
        }
    }

    let mut tmp = Document::with_version("1.7");
    tmp.objects.insert(id, obj.clone());
    tmp.max_id = id.0;
    tmp.trailer.set("Size", Object::Integer((id.0 + 1) as i64));

    let mut scratch: Vec<u8> = Vec::with_capacity(1024);
    tmp.save_to(&mut scratch)
        .map_err(|e| PdfError::Other(format!("inner serialize failed: {e}")))?;

    let needle = format!("{} {} obj", id.0, id.1);
    let start = match find_subslice(&scratch, needle.as_bytes()) {
        Some(s) => s,
        // lopdf decided not to write this object (e.g. unprintable variant);
        // treat as skip-rather-than-error.
        None => return Ok(false),
    };
    let rel_end = find_subslice(&scratch[start..], b"endobj")
        .ok_or_else(|| PdfError::Other("could not locate 'endobj' in inner save".into()))?;
    let end = start + rel_end + b"endobj".len();

    out.extend_from_slice(&scratch[start..end]);
    out.push(b'\n');
    Ok(true)
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Manually serialize the linearization parameter dictionary as an indirect
/// object. We can't use lopdf's writer here because it intentionally skips
/// indirect objects whose dictionary `get_type()` returns `b"Linearized"` —
/// it expects callers to handle the lin-dict separately (which is exactly
/// what we're doing). The dict is trivial (just integers), so we serialize
/// it by hand.
fn write_lin_dict_manually(out: &mut Vec<u8>, id: ObjectId, p: &LinearizationParams) {
    let _ = write!(
        out,
        "{} {} obj\n<< /Linearized 1 /L {} /H [ {} {} ] /O {} /E {} /N {} /T {} >>\nendobj\n",
        id.0,
        id.1,
        p.file_length,
        p.hint_stream_offset,
        p.hint_stream_length,
        p.first_page_obj_id,
        p.end_of_first_page_offset,
        p.num_pages,
        p.main_xref_offset,
    );
}

/// Replace the placeholder lin dict (at `offset`, with on-disk length
/// `placeholder_len`) with the real dict, padding internally so the
/// resulting byte length is unchanged.
#[allow(clippy::ptr_arg)] // we mutate in-place via index slicing
fn rewrite_lin_dict_manual(
    buf: &mut Vec<u8>,
    offset: usize,
    placeholder_len: usize,
    id: ObjectId,
    real: &LinearizationParams,
) -> Result<(), PdfError> {
    let mut scratch: Vec<u8> = Vec::with_capacity(placeholder_len);
    write_lin_dict_manually(&mut scratch, id, real);
    if scratch.len() > placeholder_len {
        return Err(PdfError::Other(format!(
            "linearization dict grew beyond placeholder ({}B real > {}B placeholder)",
            scratch.len(),
            placeholder_len
        )));
    }
    let pad = placeholder_len - scratch.len();
    // Insert spaces immediately after `<<` so syntactic validity is preserved.
    let open = find_subslice(&scratch, b"<<")
        .ok_or_else(|| PdfError::Other("placeholder lin dict missing '<<'".into()))?;
    let insert_at = open + 2;
    let mut padded: Vec<u8> = Vec::with_capacity(placeholder_len);
    padded.extend_from_slice(&scratch[..insert_at]);
    padded.extend(std::iter::repeat_n(b' ', pad));
    padded.extend_from_slice(&scratch[insert_at..]);
    debug_assert_eq!(padded.len(), placeholder_len);
    buf[offset..offset + placeholder_len].copy_from_slice(&padded);
    Ok(())
}

/// Write the classic `xref` table covering ids `0..=max_id`.
fn write_xref_table(out: &mut Vec<u8>, offsets: &BTreeMap<u32, usize>, max_id: u32) {
    let _ = writeln!(out, "xref");
    let _ = writeln!(out, "0 {}", max_id + 1);
    let _ = writeln!(out, "0000000000 65535 f ");
    for n in 1..=max_id {
        if let Some(off) = offsets.get(&n) {
            let _ = writeln!(out, "{:010} 00000 n ", off);
        } else {
            // Holes are legal — mark as free.
            let _ = writeln!(out, "0000000000 65535 f ");
        }
    }
}

/// Write a `trailer << … >>` block. We reuse the input doc's trailer dict,
/// patching `/Size`. The trailer is serialized by hand because we need to
/// emit a classic-xref trailer (not a cross-reference stream), and lopdf's
/// public writer doesn't expose a standalone "serialize this dictionary"
/// entry point.
fn write_trailer(out: &mut Vec<u8>, trailer: &Dictionary, size: u32) -> Result<(), PdfError> {
    out.extend_from_slice(b"trailer\n<<");
    for (key, val) in trailer.iter() {
        // /Size is rewritten — drop any incoming value.
        if key == b"Size" {
            continue;
        }
        // Skip keys that don't make sense in the new file (e.g. /Prev
        // pointing into the old file's xref chain, /XRefStm pointing at
        // a hybrid xref stream we're not emitting, encryption metadata
        // we already rejected up-front).
        if key == b"Prev" || key == b"XRefStm" || key == b"Encrypt" {
            continue;
        }
        let key_str = std::str::from_utf8(key).unwrap_or("Unknown");
        out.extend_from_slice(b" /");
        out.extend_from_slice(key_str.as_bytes());
        out.push(b' ');
        serialize_value(out, val);
    }
    let _ = write!(out, " /Size {}", size);
    out.extend_from_slice(b" >>\n");
    Ok(())
}

/// Minimal value serializer covering everything the trailer can hold
/// (integers, names, arrays, references, hex/literal strings).
fn serialize_value(out: &mut Vec<u8>, val: &Object) {
    match val {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Object::Integer(n) => {
            let _ = write!(out, "{n}");
        }
        Object::Real(r) => {
            let _ = write!(out, "{r}");
        }
        Object::Name(name) => {
            out.push(b'/');
            out.extend_from_slice(name);
        }
        Object::String(bytes, fmt) => {
            // PDF literal-string with backslash-escape for ( ) \.
            match fmt {
                lopdf::StringFormat::Hexadecimal => {
                    out.push(b'<');
                    for b in bytes {
                        let _ = write!(out, "{:02X}", b);
                    }
                    out.push(b'>');
                }
                _ => {
                    out.push(b'(');
                    for &b in bytes {
                        if b == b'(' || b == b')' || b == b'\\' {
                            out.push(b'\\');
                        }
                        out.push(b);
                    }
                    out.push(b')');
                }
            }
        }
        Object::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                serialize_value(out, item);
            }
            out.push(b']');
        }
        Object::Dictionary(d) => {
            out.extend_from_slice(b"<<");
            for (k, v) in d.iter() {
                out.push(b' ');
                out.push(b'/');
                out.extend_from_slice(k);
                out.push(b' ');
                serialize_value(out, v);
            }
            out.extend_from_slice(b" >>");
        }
        Object::Reference(id) => {
            let _ = write!(out, "{} {} R", id.0, id.1);
        }
        Object::Stream(_) => {
            // Streams shouldn't appear inside a trailer — best-effort
            // fallback to skip them quietly.
        }
    }
}

/// Rewrite the placeholder linearization dict at `offset` with `new_dict`,
/// padding with spaces so the on-disk byte length stays exactly
/// `placeholder_len`. Returns an error if the real dict grew beyond the
/// padding budget (shouldn't happen — placeholder used worst-case widths).
#[allow(dead_code, clippy::ptr_arg)]
fn rewrite_lin_dict(
    buf: &mut Vec<u8>,
    offset: usize,
    placeholder_len: usize,
    id: ObjectId,
    new_dict: &Dictionary,
) -> Result<(), PdfError> {
    let mut scratch: Vec<u8> = Vec::with_capacity(placeholder_len);
    write_indirect_object(&mut scratch, id, &Object::Dictionary(new_dict.clone()))?;
    if scratch.len() > placeholder_len {
        return Err(PdfError::Other(format!(
            "linearization dict grew beyond placeholder ({}B real > {}B placeholder)",
            scratch.len(),
            placeholder_len
        )));
    }
    // Build padded version: insert spaces inside the dict (right after `<<`)
    // so syntactic validity is preserved.
    let pad = placeholder_len - scratch.len();
    let mut padded: Vec<u8> = Vec::with_capacity(placeholder_len);
    if let Some(open) = find_subslice(&scratch, b"<<") {
        let insert_at = open + 2;
        padded.extend_from_slice(&scratch[..insert_at]);
        padded.extend(std::iter::repeat_n(b' ', pad));
        padded.extend_from_slice(&scratch[insert_at..]);
    } else {
        // Fallback: pad at the very end (before final newline).
        padded.extend_from_slice(&scratch);
        padded.extend(std::iter::repeat_n(b' ', pad));
    }
    debug_assert_eq!(padded.len(), placeholder_len);
    buf[offset..offset + placeholder_len].copy_from_slice(&padded);
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::streamline::inspect::is_linearized;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use tempfile::tempdir;

    #[test]
    fn linearize_two_page_pdf_produces_output() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 2);
        let output = dir.path().join("out.pdf");

        let report = linearize_pdf(&input, &output).expect("linearize ok");
        assert!(output.exists());
        assert_eq!(report.status, LinearizationStatus::Linearized);
        assert!(report.after.is_some());
        let after = report.after.unwrap();
        assert!(after.first_page_prefix_bytes > 0);
        assert!(
            after.first_page_prefix_bytes < after.total_bytes,
            "first-page prefix ({}) must be smaller than total ({})",
            after.first_page_prefix_bytes,
            after.total_bytes
        );
        assert_eq!(after.page_count, 2);
    }

    #[test]
    fn linearize_output_reloads_via_lopdf() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 3);
        let output = dir.path().join("out.pdf");
        linearize_pdf(&input, &output).unwrap();

        let reloaded = lopdf::Document::load(&output).expect("reloadable");
        assert_eq!(reloaded.page_iter().count(), 3);
    }

    #[test]
    fn linearize_output_is_detected_as_linearized() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 4);
        let output = dir.path().join("out.pdf");
        linearize_pdf(&input, &output).unwrap();
        let (status, stats) = is_linearized(&output).unwrap();
        assert_eq!(
            status,
            LinearizationStatus::Linearized,
            "round-trip: inspector should detect /Linearized at the head"
        );
        assert_eq!(stats.page_count, 4);
        assert!(stats.hint_stream_bytes > 0);
    }

    #[test]
    fn linearize_rejects_missing_input() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope.pdf");
        let out = dir.path().join("out.pdf");
        let err = linearize_pdf(&missing, &out).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }

    #[test]
    fn linearize_rejects_damaged_input() {
        let dir = tempdir().unwrap();
        let bogus = dir.path().join("bogus.pdf");
        fs::write(&bogus, b"not pdf").unwrap();
        let out = dir.path().join("out.pdf");
        let err = linearize_pdf(&bogus, &out).unwrap_err();
        let msg = format!("{err}").to_lowercase();
        assert!(
            msg.contains("damaged") || msg.contains("parse") || msg.contains("unsupported"),
            "expected parse-error message, got: {msg}"
        );
    }

    #[test]
    fn linearize_report_carries_before_and_after_stats() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        make_n_page_pdf(&input, 1);
        let output = dir.path().join("out.pdf");
        let report = linearize_pdf(&input, &output).unwrap();

        assert_eq!(report.before.page_count, 1);
        assert_eq!(
            report.before.first_page_prefix_bytes, report.before.total_bytes,
            "before stats: a non-linearized input has no prefix shortcut"
        );

        let after = report.after.expect("after stats present");
        assert_eq!(after.page_count, 1);
        assert!(after.hint_stream_bytes > 0);
    }

    #[test]
    fn rewrite_lin_dict_preserves_byte_length() {
        let placeholder = LinearizationParams {
            file_length: 9_999_999_999,
            hint_stream_offset: 9_999_999_999,
            hint_stream_length: 9_999_999_999,
            first_page_obj_id: 999_999,
            end_of_first_page_offset: 9_999_999_999,
            num_pages: 9_999_999,
            main_xref_offset: 9_999_999_999,
        };
        let real = LinearizationParams {
            file_length: 42,
            hint_stream_offset: 100,
            hint_stream_length: 50,
            first_page_obj_id: 5,
            end_of_first_page_offset: 30,
            num_pages: 1,
            main_xref_offset: 35,
        };
        let id: ObjectId = (7, 0);

        let mut buf = Vec::new();
        write_lin_dict_manually(&mut buf, id, &placeholder);
        let original_len = buf.len();

        rewrite_lin_dict_manual(&mut buf, 0, original_len, id, &real).unwrap();
        assert_eq!(
            buf.len(),
            original_len,
            "rewrite must not change buffer length"
        );
        // The real values must be discoverable in the rewritten bytes.
        assert!(find_subslice(&buf, b"/L 42").is_some());
        assert!(find_subslice(&buf, b"/N 1 ").is_some());
    }
}
