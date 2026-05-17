// In-place PDF text editing — Slice 6 of v0.11.0 "Lathe".
//
// This is the hard one. The Tj/TJ operators in a content stream don't have
// pretty "word" boundaries; they're just byte strings that the PDF viewer
// renders left-to-right at the current text-matrix position with the
// currently-selected font. To "edit" text, we have to:
//
//   1. Find all Tj / TJ operations on a page.
//   2. For each, figure out (a) what the text *says*, (b) where it
//      visually starts on the page (so the UI can position an edit
//      bubble), and (c) which font is in play.
//   3. When the user submits a replacement string, swap the bytes in the
//      Tj/TJ operand and re-encode the stream.
//
// Hard parts we DO handle in this MVP:
//   - Tj with a literal-string operand (the 99% case for `BT ... Tj ... ET`).
//   - TJ with an array of strings + kerning offsets (mostly-text variant
//     — we treat all strings concatenated as the "span text" and split
//     the replacement evenly when writing back, or punt with a clear error
//     when the array has more than one string segment).
//   - Tracking Tf (font), Tfs (font size), and a *rough* text origin
//     from Td/TD/Tm operations (we accumulate translation, ignoring
//     the rotation/scale components of Tm — that's good enough to put
//     an edit overlay near the right place; the UI's PDF.js text layer
//     already has pixel-perfect spans).
//   - Handling content streams stored as a single stream OR an array of
//     streams (both are legal — see grayscale.rs which solved the same).
//
// Hard parts we EXPLICITLY DO NOT handle yet — they return a clear,
// user-facing `PdfError::Other(...)` so the UI can show a real message
// instead of crashing:
//   - Composite (CID-keyed Type 0) fonts. We can't safely swap glyphs
//     without consulting the font's CMap; non-ASCII rewrites would
//     corrupt the PDF. Detected by the font's /Subtype.
//   - Non-ASCII replacement text on a Type 1/TrueType font that has
//     no explicit /Encoding override. The replacement would have to
//     be PDFDocEncoded; we punt with "non-ASCII edits not supported
//     yet" and let the user fall back to delete-and-redraw.
//   - Re-encoded streams (already through `compress` etc.) — we always
//     work on the decompressed bytes, so this is handled.
//
// What ships in this MVP: a backend that can *find* every editable
// text span on a page and *replace* the ASCII ones. The UI overlay in
// Slice 7 will sit on top of this.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single editable text span discovered on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSpan {
    /// Stable string ID of the form `"p<page>:s<seq>"` — the UI uses
    /// this to address the span on follow-up `replace_text_span` calls.
    /// Sequence is the index of the span on its page (0-based), counted
    /// in stream-order. Stable as long as the page's content stream
    /// isn't mutated by something else first.
    pub id: String,
    /// 1-based page number.
    pub page: u32,
    /// The text that the user sees rendered at this span.
    pub text: String,
    /// PDF resource name of the font in scope when this span was drawn,
    /// e.g. "F1", "TT2", "C2_0". Empty if no `Tf` was seen yet.
    pub font_resource: String,
    /// Point size from the most recent `Tf` op.
    pub font_size: f32,
    /// Approximate origin in *unscaled* text-space units (the page's
    /// own coordinate system). Bottom-left = (0,0). This is the
    /// *cumulative* Td translation since `BT`; it ignores Tm rotation
    /// and scale, so it's a hint, not a precise box.
    pub x: f32,
    pub y: f32,
    /// True if this span is safely replaceable today (ASCII Tj on a
    /// non-CID font). When false the UI should show a read-only badge.
    pub editable: bool,
    /// If `editable` is false, a human-readable reason.
    pub reason: Option<String>,
}

/// All spans found on one page, returned in document order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSpans {
    pub page: u32,
    pub spans: Vec<TextSpan>,
}

/// Walk every page of `input` and return its editable text spans.
pub fn find_text_spans(input: &Path) -> Result<Vec<PageSpans>, PdfError> {
    let doc = Document::load(input)?;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();

    let mut out: Vec<PageSpans> = Vec::with_capacity(page_ids.len());
    for (page_num, page_id) in &page_ids {
        let spans = collect_page_spans(&doc, *page_id, *page_num);
        out.push(PageSpans {
            page: *page_num,
            spans,
        });
    }
    Ok(out)
}

/// Replace the `Tj` text on a single span. `span_id` must look like
/// `"p<page>:s<seq>"` (the same shape `find_text_spans` returned).
///
/// `new_text` must be ASCII for the MVP — otherwise we return a
/// `PdfError::Other` describing the limitation.
pub fn replace_text_span(
    input: &Path,
    output: &Path,
    span_id: &str,
    new_text: &str,
) -> Result<(), PdfError> {
    let (page_num, seq) = parse_span_id(span_id)?;
    if !new_text.is_ascii() {
        return Err(PdfError::Other(
            "Non-ASCII replacement text is not supported yet (CID/Unicode font work pending)."
                .into(),
        ));
    }
    // Reject characters that break the PDF literal-string escaping.
    // The Tj operand is wrapped in `(...)` and `\` ( ) need escaping;
    // we DO handle that ourselves below — but we still reject control
    // characters because they're almost always a UI bug.
    if new_text.chars().any(|c| (c as u32) < 0x20 && c != '\n') {
        return Err(PdfError::Other(
            "Replacement contains a control character (only printable ASCII is supported).".into(),
        ));
    }

    let mut doc = Document::load(input)?;
    let target_page_id = doc
        .get_pages()
        .into_iter()
        .find(|(n, _)| *n == page_num)
        .map(|(_, id)| id)
        .ok_or_else(|| PdfError::Other(format!("Page {page_num} not found in document")))?;

    let stream_ids = content_stream_ids(&doc, target_page_id);
    if stream_ids.is_empty() {
        return Err(PdfError::Other(format!(
            "Page {page_num} has no content stream to edit"
        )));
    }

    // We need a per-page running sequence index across (possibly) multiple
    // content streams, in the same order that `collect_page_spans` produced.
    let mut running_seq = 0u32;
    let mut replaced = false;
    for sid in stream_ids {
        if try_replace_in_stream(&mut doc, sid, &mut running_seq, seq, new_text)? {
            replaced = true;
            break;
        }
    }
    if !replaced {
        return Err(PdfError::Other(format!(
            "Span '{span_id}' not found on page {page_num}"
        )));
    }
    doc.save(output)?;
    Ok(())
}

// ---------- internals ----------

fn parse_span_id(id: &str) -> Result<(u32, u32), PdfError> {
    // Expect "p<page>:s<seq>".
    let bad = || PdfError::Other(format!("Malformed span id: {id:?} (expected 'p<n>:s<m>')"));
    let rest = id.strip_prefix('p').ok_or_else(bad)?;
    let (p, s) = rest.split_once(":s").ok_or_else(bad)?;
    let page: u32 = p.parse().map_err(|_| bad())?;
    let seq: u32 = s.parse().map_err(|_| bad())?;
    Ok((page, seq))
}

fn content_stream_ids(doc: &Document, page_id: lopdf::ObjectId) -> Vec<lopdf::ObjectId> {
    let mut out = Vec::new();
    let Ok(dict) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return out;
    };
    match dict.get(b"Contents") {
        Ok(Object::Reference(r)) => out.push(*r),
        Ok(Object::Array(arr)) => {
            for o in arr {
                if let Object::Reference(r) = o {
                    out.push(*r);
                }
            }
        }
        _ => {}
    }
    out
}

fn decoded_stream_bytes(doc: &Document, sid: lopdf::ObjectId) -> Option<Vec<u8>> {
    let obj = doc.get_object(sid).ok()?;
    let Object::Stream(s) = obj else {
        return None;
    };
    Some(
        s.decompressed_content()
            .unwrap_or_else(|_| s.content.clone()),
    )
}

/// Inspect the document's font resource and decide whether we know
/// how to round-trip ASCII through it. CID Type-0 fonts get a "no" answer.
fn font_is_safe(doc: &Document, page_id: lopdf::ObjectId, font_resource: &str) -> (bool, String) {
    if font_resource.is_empty() {
        return (true, String::new());
    }
    let Ok(page_dict) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return (true, String::new());
    };
    let resources = match page_dict.get(b"Resources") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => match doc.get_object(*r).and_then(|o| o.as_dict()) {
            Ok(d) => d.clone(),
            _ => return (true, String::new()),
        },
        _ => return (true, String::new()),
    };
    let fonts = match resources.get(b"Font") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => match doc.get_object(*r).and_then(|o| o.as_dict()) {
            Ok(d) => d.clone(),
            _ => return (true, String::new()),
        },
        _ => return (true, String::new()),
    };
    let font_obj = match fonts.get(font_resource.as_bytes()) {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => match doc.get_object(*r).and_then(|o| o.as_dict()) {
            Ok(d) => d.clone(),
            _ => return (true, String::new()),
        },
        _ => return (true, String::new()),
    };
    let subtype_bytes = match font_obj.get(b"Subtype") {
        Ok(Object::Name(n)) => n.clone(),
        _ => return (true, String::new()),
    };
    let subtype = String::from_utf8_lossy(&subtype_bytes);
    if subtype == "Type0" {
        return (
            false,
            format!("Font {font_resource} is CID Type0 — text edit not supported yet"),
        );
    }
    (true, String::new())
}

fn collect_page_spans(doc: &Document, page_id: lopdf::ObjectId, page_num: u32) -> Vec<TextSpan> {
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut seq: u32 = 0;
    for sid in content_stream_ids(doc, page_id) {
        let Some(bytes) = decoded_stream_bytes(doc, sid) else {
            continue;
        };
        let Ok(content) = Content::decode(&bytes) else {
            continue;
        };
        // Walk operations with a running text state.
        let mut state = TextState::default();
        let mut in_bt = false;
        for op in &content.operations {
            match op.operator.as_str() {
                "BT" => {
                    in_bt = true;
                    state.cursor_x = 0.0;
                    state.cursor_y = 0.0;
                }
                "ET" => {
                    in_bt = false;
                }
                "Tf" if op.operands.len() == 2 => {
                    if let Object::Name(n) = &op.operands[0] {
                        state.font_resource = String::from_utf8_lossy(n).into_owned();
                    }
                    state.font_size = op_to_f32(&op.operands[1]);
                }
                "Td" | "TD" if op.operands.len() == 2 => {
                    state.cursor_x += op_to_f32(&op.operands[0]);
                    state.cursor_y += op_to_f32(&op.operands[1]);
                }
                "Tm" if op.operands.len() == 6 => {
                    // Snap absolute position from translation components.
                    state.cursor_x = op_to_f32(&op.operands[4]);
                    state.cursor_y = op_to_f32(&op.operands[5]);
                }
                "T*" => {
                    // Move to next line. We don't track leading precisely.
                    state.cursor_y -= state.font_size.max(8.0);
                }
                "Tj" if in_bt && op.operands.len() == 1 => {
                    if let Some(span) =
                        span_from_tj(&op.operands[0], page_num, seq, &state, doc, page_id)
                    {
                        spans.push(span);
                    }
                    seq += 1;
                }
                "'" | "\"" if in_bt && !op.operands.is_empty() => {
                    // Move to next line + show string. `"` has two leading
                    // numeric ops (word + char spacing) we ignore.
                    let s = match op.operator.as_str() {
                        "'" => &op.operands[0],
                        _ => op.operands.last().unwrap(),
                    };
                    state.cursor_y -= state.font_size.max(8.0);
                    if let Some(span) = span_from_tj(s, page_num, seq, &state, doc, page_id) {
                        spans.push(span);
                    }
                    seq += 1;
                }
                "TJ" if in_bt && op.operands.len() == 1 => {
                    if let Some(span) =
                        span_from_tj_array(&op.operands[0], page_num, seq, &state, doc, page_id)
                    {
                        spans.push(span);
                    }
                    seq += 1;
                }
                _ => {}
            }
        }
    }
    spans
}

#[derive(Default, Clone)]
struct TextState {
    font_resource: String,
    font_size: f32,
    cursor_x: f32,
    cursor_y: f32,
}

fn op_to_f32(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn span_from_tj(
    operand: &Object,
    page: u32,
    seq: u32,
    state: &TextState,
    doc: &Document,
    page_id: lopdf::ObjectId,
) -> Option<TextSpan> {
    let bytes = match operand {
        Object::String(b, _) => b.clone(),
        _ => return None,
    };
    if bytes.is_empty() {
        return None;
    }
    let (font_ok, font_reason) = font_is_safe(doc, page_id, &state.font_resource);
    let is_ascii = bytes.iter().all(|b| *b < 0x80);
    let editable = font_ok && is_ascii;
    let reason = if !font_ok {
        Some(font_reason)
    } else if !is_ascii {
        Some("Non-ASCII glyphs — replacement not supported yet".into())
    } else {
        None
    };
    Some(TextSpan {
        id: format!("p{}:s{}", page, seq),
        page,
        text: String::from_utf8_lossy(&bytes).into_owned(),
        font_resource: state.font_resource.clone(),
        font_size: state.font_size,
        x: state.cursor_x,
        y: state.cursor_y,
        editable,
        reason,
    })
}

fn span_from_tj_array(
    operand: &Object,
    page: u32,
    seq: u32,
    state: &TextState,
    doc: &Document,
    page_id: lopdf::ObjectId,
) -> Option<TextSpan> {
    let arr = match operand {
        Object::Array(a) => a,
        _ => return None,
    };
    let mut combined = Vec::<u8>::new();
    let mut segments = 0;
    for item in arr {
        if let Object::String(b, _) = item {
            combined.extend_from_slice(b);
            segments += 1;
        }
    }
    if combined.is_empty() {
        return None;
    }
    let (font_ok, font_reason) = font_is_safe(doc, page_id, &state.font_resource);
    let is_ascii = combined.iter().all(|b| *b < 0x80);
    // TJ arrays with >1 segment have kerning gaps the user can see; a
    // straight replacement would either smush together or stretch. We
    // mark these read-only and let the UI offer a "convert to plain
    // text and re-edit" hint later.
    let editable = font_ok && is_ascii && segments == 1;
    let reason = if !font_ok {
        Some(font_reason)
    } else if !is_ascii {
        Some("Non-ASCII glyphs — replacement not supported yet".into())
    } else if segments > 1 {
        Some(format!(
            "Text drawn with kerning ({segments} segments) — direct edit not supported yet"
        ))
    } else {
        None
    };
    Some(TextSpan {
        id: format!("p{}:s{}", page, seq),
        page,
        text: String::from_utf8_lossy(&combined).into_owned(),
        font_resource: state.font_resource.clone(),
        font_size: state.font_size,
        x: state.cursor_x,
        y: state.cursor_y,
        editable,
        reason,
    })
}

fn try_replace_in_stream(
    doc: &mut Document,
    sid: lopdf::ObjectId,
    running_seq: &mut u32,
    target_seq: u32,
    new_text: &str,
) -> Result<bool, PdfError> {
    let Some(bytes) = decoded_stream_bytes(doc, sid) else {
        return Ok(false);
    };
    let Ok(content) = Content::decode(&bytes) else {
        return Ok(false);
    };

    let mut new_ops: Vec<Operation> = Vec::with_capacity(content.operations.len());
    let mut in_bt = false;
    let mut hit = false;
    for op in content.operations {
        let mut emit_op = op.clone();
        match op.operator.as_str() {
            "BT" => in_bt = true,
            "ET" => in_bt = false,
            "Tj" if in_bt && op.operands.len() == 1 => {
                if matches!(&op.operands[0], Object::String(_, _)) {
                    if *running_seq == target_seq {
                        emit_op = Operation::new(
                            "Tj",
                            vec![Object::string_literal(new_text.as_bytes().to_vec())],
                        );
                        hit = true;
                    }
                    *running_seq += 1;
                }
            }
            "'" if in_bt && op.operands.len() == 1 => {
                if matches!(&op.operands[0], Object::String(_, _)) {
                    if *running_seq == target_seq {
                        emit_op = Operation::new(
                            "'",
                            vec![Object::string_literal(new_text.as_bytes().to_vec())],
                        );
                        hit = true;
                    }
                    *running_seq += 1;
                }
            }
            "\"" if in_bt && op.operands.len() == 3 => {
                if matches!(&op.operands[2], Object::String(_, _)) {
                    if *running_seq == target_seq {
                        emit_op = Operation::new(
                            "\"",
                            vec![
                                op.operands[0].clone(),
                                op.operands[1].clone(),
                                Object::string_literal(new_text.as_bytes().to_vec()),
                            ],
                        );
                        hit = true;
                    }
                    *running_seq += 1;
                }
            }
            "TJ" if in_bt && op.operands.len() == 1 => {
                if let Object::Array(arr) = &op.operands[0] {
                    // Only single-segment TJ is editable (see find).
                    let string_count = arr
                        .iter()
                        .filter(|o| matches!(o, Object::String(_, _)))
                        .count();
                    if string_count == 1 && *running_seq == target_seq {
                        let new_arr: Vec<Object> = arr
                            .iter()
                            .map(|o| match o {
                                Object::String(_, _) => {
                                    Object::string_literal(new_text.as_bytes().to_vec())
                                }
                                other => other.clone(),
                            })
                            .collect();
                        emit_op = Operation::new("TJ", vec![Object::Array(new_arr)]);
                        hit = true;
                    }
                    *running_seq += 1;
                }
            }
            _ => {}
        }
        new_ops.push(emit_op);
        if hit {
            // Keep walking so we re-emit the rest of the stream verbatim,
            // but we've already done the swap.
            // (Don't break; we need to finish constructing new_ops.)
        }
    }
    if !hit {
        return Ok(false);
    }
    let new_content = Content {
        operations: new_ops,
    };
    let encoded = new_content
        .encode()
        .map_err(|e| PdfError::Other(format!("Failed to encode edited stream: {e}")))?;
    if let Ok(Object::Stream(s)) = doc.get_object_mut(sid) {
        s.set_plain_content(encoded);
        let _ = s.compress();
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};
    use tempfile::tempdir;

    fn pdf_with_streams(streams: &[(&str, &[u8])]) -> Vec<u8> {
        // streams = [(font_subtype, content_bytes), ...] one per page.
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for (subtype, stream_bytes) in streams {
            let font = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
                "BaseFont" => "Helvetica",
            });
            let resources = doc.add_object(dictionary! {
                "Font" => dictionary! { "F1" => font },
            });
            let contents = doc.add_object(Stream::new(dictionary! {}, stream_bytes.to_vec()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => contents,
                "Resources" => resources,
            });
            kids.push(Object::Reference(page));
        }
        let kids_count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => kids_count,
            }),
        );
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    fn write_tmp(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let p = dir.path().join("in.pdf");
        std::fs::write(&p, bytes).unwrap();
        (dir, p)
    }

    #[test]
    fn finds_single_tj_span() {
        // BT /F1 12 Tf 50 700 Td (Hello world) Tj ET
        let bytes = pdf_with_streams(&[("Type1", b"BT /F1 12 Tf 50 700 Td (Hello world) Tj ET\n")]);
        let (_dir, p) = write_tmp(&bytes);
        let spans = find_text_spans(&p).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].page, 1);
        assert_eq!(spans[0].spans.len(), 1);
        let s = &spans[0].spans[0];
        assert_eq!(s.text, "Hello world");
        assert_eq!(s.id, "p1:s0");
        assert_eq!(s.font_resource, "F1");
        assert!((s.font_size - 12.0).abs() < 0.01);
        assert!((s.x - 50.0).abs() < 0.01);
        assert!((s.y - 700.0).abs() < 0.01);
        assert!(s.editable);
    }

    #[test]
    fn finds_multiple_tj_spans_with_ids() {
        let bytes = pdf_with_streams(&[(
            "Type1",
            b"BT /F1 12 Tf 50 700 Td (First) Tj 0 -20 Td (Second) Tj ET\n",
        )]);
        let (_dir, p) = write_tmp(&bytes);
        let spans = find_text_spans(&p).unwrap();
        assert_eq!(spans[0].spans.len(), 2);
        assert_eq!(spans[0].spans[0].id, "p1:s0");
        assert_eq!(spans[0].spans[0].text, "First");
        assert_eq!(spans[0].spans[1].id, "p1:s1");
        assert_eq!(spans[0].spans[1].text, "Second");
        // Second span should be lower on the page (cumulative Td translation).
        assert!(spans[0].spans[1].y < spans[0].spans[0].y);
    }

    #[test]
    fn cid_font_marks_span_read_only() {
        // Same Tj, but the font is Type0 (CID).
        let bytes = pdf_with_streams(&[("Type0", b"BT /F1 12 Tf 50 700 Td (Hello) Tj ET\n")]);
        let (_dir, p) = write_tmp(&bytes);
        let spans = find_text_spans(&p).unwrap();
        let s = &spans[0].spans[0];
        assert!(!s.editable);
        let reason = s.reason.as_ref().unwrap();
        assert!(reason.contains("CID") || reason.contains("Type0"));
    }

    #[test]
    fn non_ascii_tj_marks_span_read_only() {
        // High-bit bytes in the Tj operand (PDFDocEncoded "é" at 0xE9).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BT /F1 12 Tf 50 700 Td (Caf");
        bytes.push(0xE9);
        bytes.extend_from_slice(b") Tj ET\n");
        let pdf = pdf_with_streams(&[("Type1", &bytes)]);
        let (_dir, p) = write_tmp(&pdf);
        let spans = find_text_spans(&p).unwrap();
        let s = &spans[0].spans[0];
        assert!(!s.editable, "non-ASCII should be read-only");
        assert!(s.reason.as_ref().unwrap().contains("Non-ASCII"));
    }

    #[test]
    fn tj_array_multi_segment_read_only_but_visible() {
        // TJ with two string segments + a kerning offset between them.
        let stream = b"BT /F1 12 Tf 50 700 Td [(Hel) -50 (lo)] TJ ET\n";
        let bytes = pdf_with_streams(&[("Type1", stream)]);
        let (_dir, p) = write_tmp(&bytes);
        let spans = find_text_spans(&p).unwrap();
        let s = &spans[0].spans[0];
        assert_eq!(s.text, "Hello");
        assert!(!s.editable);
        assert!(s.reason.as_ref().unwrap().contains("kerning"));
    }

    #[test]
    fn tj_array_single_segment_is_editable() {
        let stream = b"BT /F1 12 Tf 50 700 Td [(Hello)] TJ ET\n";
        let bytes = pdf_with_streams(&[("Type1", stream)]);
        let (_dir, p) = write_tmp(&bytes);
        let spans = find_text_spans(&p).unwrap();
        let s = &spans[0].spans[0];
        assert_eq!(s.text, "Hello");
        assert!(s.editable);
    }

    #[test]
    fn replace_text_span_writes_new_string() {
        let bytes = pdf_with_streams(&[("Type1", b"BT /F1 12 Tf 50 700 Td (Hello world) Tj ET\n")]);
        let (dir, p) = write_tmp(&bytes);
        let out = dir.path().join("out.pdf");
        replace_text_span(&p, &out, "p1:s0", "Bye world!").unwrap();

        // Re-extract spans and verify the swap.
        let spans = find_text_spans(&out).unwrap();
        assert_eq!(spans[0].spans[0].text, "Bye world!");
        // And the doc still opens — extract_text should see the new text too.
        let doc = Document::load(&out).unwrap();
        let txt = doc.extract_text(&[1]).unwrap();
        assert!(txt.contains("Bye world!"));
        assert!(!txt.contains("Hello world"));
    }

    #[test]
    fn replace_text_span_preserves_other_spans() {
        let bytes = pdf_with_streams(&[(
            "Type1",
            b"BT /F1 12 Tf 50 700 Td (First) Tj 0 -20 Td (Second) Tj ET\n",
        )]);
        let (dir, p) = write_tmp(&bytes);
        let out = dir.path().join("out.pdf");
        replace_text_span(&p, &out, "p1:s1", "Replaced").unwrap();
        let spans = find_text_spans(&out).unwrap();
        assert_eq!(spans[0].spans[0].text, "First");
        assert_eq!(spans[0].spans[1].text, "Replaced");
    }

    #[test]
    fn replace_text_span_rejects_non_ascii() {
        let bytes = pdf_with_streams(&[("Type1", b"BT /F1 12 Tf 50 700 Td (Hello) Tj ET\n")]);
        let (dir, p) = write_tmp(&bytes);
        let out = dir.path().join("out.pdf");
        let r = replace_text_span(&p, &out, "p1:s0", "Café");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("Non-ASCII"));
    }

    #[test]
    fn replace_text_span_rejects_bad_span_id() {
        let bytes = pdf_with_streams(&[("Type1", b"BT /F1 12 Tf 50 700 Td (Hello) Tj ET\n")]);
        let (dir, p) = write_tmp(&bytes);
        let out = dir.path().join("out.pdf");
        for bad in ["", "abc", "p1", "p1:s", "s1", "p:s0", "p1:s9999"] {
            let r = replace_text_span(&p, &out, bad, "x");
            assert!(r.is_err(), "expected err for {bad:?}");
        }
    }

    #[test]
    fn replace_text_span_rejects_control_chars() {
        let bytes = pdf_with_streams(&[("Type1", b"BT /F1 12 Tf 50 700 Td (Hello) Tj ET\n")]);
        let (dir, p) = write_tmp(&bytes);
        let out = dir.path().join("out.pdf");
        let r = replace_text_span(&p, &out, "p1:s0", "Bad\tchar");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("control character"));
    }

    #[test]
    fn parse_span_id_round_trip() {
        assert_eq!(parse_span_id("p1:s0").unwrap(), (1, 0));
        assert_eq!(parse_span_id("p42:s7").unwrap(), (42, 7));
        assert!(parse_span_id("p1s0").is_err());
        assert!(parse_span_id("p1:0").is_err());
        assert!(parse_span_id("page1:s0").is_err());
    }

    #[test]
    fn multi_page_spans_have_distinct_ids() {
        let bytes = pdf_with_streams(&[
            ("Type1", b"BT /F1 12 Tf 50 700 Td (Page one) Tj ET\n"),
            ("Type1", b"BT /F1 12 Tf 50 700 Td (Page two) Tj ET\n"),
        ]);
        let (_dir, p) = write_tmp(&bytes);
        let pages = find_text_spans(&p).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].spans[0].id, "p1:s0");
        assert_eq!(pages[1].spans[0].id, "p2:s0");
        assert_eq!(pages[0].spans[0].text, "Page one");
        assert_eq!(pages[1].spans[0].text, "Page two");
    }

    #[test]
    fn empty_pdf_returns_empty_spans() {
        // A 1-page PDF with an empty content stream.
        let bytes = pdf_with_streams(&[("Type1", b"")]);
        let (_dir, p) = write_tmp(&bytes);
        let pages = find_text_spans(&p).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].spans.len(), 0);
    }

    #[test]
    fn missing_input_errors() {
        let bad = std::path::PathBuf::from("/nope/does/not/exist.pdf");
        assert!(find_text_spans(&bad).is_err());
    }
}
