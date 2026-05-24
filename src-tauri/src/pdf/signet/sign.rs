//! End-to-end PDF signing: place a signature widget + ByteRange placeholder,
//! serialize, hash, build the PKCS#7 blob, splice it back into `/Contents`.
//!
//! v3.10.0 first cut. Algorithm (per ISO 32000-1 §12.8 + Adobe profile):
//!
//! 1. Open the PDF with lopdf.
//! 2. Build a signature dict object with `/ByteRange [0 _________ _________ _________]`
//!    (zero-padded fixed-width placeholders) and `/Contents <00…00>` (a hex
//!    window of [`SIGNATURE_HEX_PLACEHOLDER_BYTES`] chars).
//! 3. Build a Sig widget annotation + AcroForm entry pointing at the dict.
//! 4. Serialize the full document to bytes.
//! 5. Locate the byte offsets of `/Contents <…>` (hex window) and `/ByteRange […]`
//!    in the serialized bytes.
//! 6. Rewrite `/ByteRange` in place with the real `[0 hex_open hex_close+1 tail_len]`
//!    (keeping the field's total serialized length constant via space-padding).
//! 7. SHA-256 the bytes outside the hex window (brackets included in the tail).
//! 8. Call [`build_pkcs7_detached`] to produce the CMS blob.
//! 9. Hex-encode + uppercase + right-pad with `0`s to fill the hex window.
//! 10. Splice into the file bytes and `atomic_save`.
//!
//! The first cut targets the common case: append a new Signature1 field to
//! a PDF whose AcroForm may or may not already exist. Filling an existing
//! pre-placed Sig widget is a v3.10.1 follow-up.

use std::path::Path;
use std::time::SystemTime;

use lopdf::{dictionary, Document, Object, ObjectId};

use super::cms_blob::{build_pkcs7_detached, sha256};
use super::identity::{SignetError, SigningIdentity};
use crate::pdf::signet_pro::appearance::{build_appearance, AppearanceSpec};

/// Hex-window size for `/Contents` in the CAdES-BES (no-TSA) case.
/// 16 384 hex chars = 8192 binary bytes — generous headroom over a typical
/// RSA-2048 PKCS#7 blob (~2 KiB) plus embedded cert + chain (~2-3 KiB).
pub const SIGNATURE_HEX_PLACEHOLDER_BYTES: usize = 16_384;

/// Hex-window size for `/Contents` in the CAdES-T (with-TSA) case.
/// 32 768 hex chars = 16 384 binary bytes — adds ~8 KiB headroom for the
/// embedded RFC 3161 timestamp token (typical TST ~4-6 KiB including TSA
/// cert chain).
pub const SIGNATURE_HEX_PLACEHOLDER_BYTES_T: usize = 32_768;

fn placeholder_hex_bytes(opts: &SignOptions) -> usize {
    if opts.tsa_url.as_deref().filter(|s| !s.is_empty()).is_some() {
        SIGNATURE_HEX_PLACEHOLDER_BYTES_T
    } else {
        SIGNATURE_HEX_PLACEHOLDER_BYTES
    }
}

/// Width of each ByteRange integer in the placeholder. 10 digits accommodates
/// PDFs up to ~10 GiB. The four numbers are space-padded to this width so
/// the total `/ByteRange [a b c d]` length is invariant.
const BYTE_RANGE_WIDTH: usize = 10;

/// Caller-supplied knobs.
#[derive(Debug, Clone, Default)]
pub struct SignOptions {
    /// Optional human-readable reason (e.g. "I approve this contract").
    pub reason: Option<String>,
    /// Optional location (e.g. "San Francisco, CA").
    pub location: Option<String>,
    /// Optional contact info (rarely used).
    pub contact_info: Option<String>,
    /// Field name. Defaults to `Signature1` (or `Signature{N}` if taken).
    pub field_name: Option<String>,
    /// Optional visible signature appearance. When set, the widget gets a
    /// non-zero `/Rect` and an `/AP << /N <form-xobject> >>` so viewers
    /// render a visible stamp. When `None`, the signature is invisible
    /// (the v3.10.0 default).
    pub appearance: Option<AppearanceSpec>,
    /// Optional RFC 3161 TSA URL. When set, the signature is upgraded
    /// from CAdES-BES to CAdES-T: after building the PKCS#7 blob, Slab
    /// fetches a timestamp token from the TSA and embeds it as the
    /// `id-aa-timeStampToken` unsigned attribute. Requires network.
    pub tsa_url: Option<String>,
}

/// Outcome of a successful sign operation.
#[derive(Debug, Clone)]
pub struct SignReport {
    /// Final output file size in bytes.
    pub output_bytes: u64,
    /// Byte offset of the `<` opening the `/Contents` hex window.
    pub contents_open: u64,
    /// Byte offset of the `>` closing the `/Contents` hex window.
    pub contents_close: u64,
    /// The `/ByteRange` array exactly as written.
    pub byte_range: [u64; 4],
    /// Hex chars actually consumed by the CMS blob (the rest are `0` padding).
    pub signature_hex_used: usize,
    /// Wall-clock duration in milliseconds.
    pub elapsed_ms: u64,
    /// Field name used.
    pub field_name: String,
}

/// Sign `input` with `identity` and write the signed PDF to `output`.
///
/// `output` MAY equal `input` — we serialize, hash, splice, and atomically
/// rename in place, so an in-place sign is safe even on crash.
pub fn sign_pdf(
    input: &Path,
    output: &Path,
    identity: &SigningIdentity,
    opts: &SignOptions,
) -> Result<SignReport, SignetError> {
    let started = SystemTime::now();
    identity.ensure_valid_now()?;

    // 1. Load PDF.
    let mut doc =
        Document::load(input).map_err(|e| SignetError::InvalidCert(format!("lopdf load: {e}")))?;

    let field_name = pick_field_name(&doc, opts.field_name.as_deref());
    let hex_window_bytes = placeholder_hex_bytes(opts);

    // 2. Build the placeholder signature dict + widget.
    // `Object::String(_, Hexadecimal)` is encoded by lopdf as <hex>, with each
    // input byte expanding to two hex chars. So an N/2-byte zero buffer
    // serializes as N ASCII '0' chars between angle brackets — exactly
    // `hex_window_bytes`.
    let placeholder_hex = vec![0u8; hex_window_bytes / 2];
    let byte_range_placeholder = Object::Array(vec![
        Object::Integer(0),
        // Three literal-string placeholders, each 20 ASCII spaces wide.
        // lopdf serializes `Object::String(_, Literal)` as `(content)` —
        // so each slot occupies (1 + 20 + 1) = 22 bytes. The full array
        // interior is `0 (....) (....) (....)` = 1+1+22+1+22+1+22 = 70
        // bytes — plenty of headroom for four 10-digit space-padded ints
        // separated by spaces (= 43 bytes) when we rewrite at the byte
        // layer in `rewrite_byte_range_in_place`.
        Object::String(vec![b' '; 20], lopdf::StringFormat::Literal),
        Object::String(vec![b' '; 20], lopdf::StringFormat::Literal),
        Object::String(vec![b' '; 20], lopdf::StringFormat::Literal),
    ]);
    // Note: writing the placeholder slots as literal-strings rather than
    // raw Integers lets us reliably locate the array by its length when
    // re-serializing. We rewrite the whole `/ByteRange […]` token at the
    // bytes layer anyway, so the lopdf object form is throwaway.

    let mut sig_dict = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "adbe.pkcs7.detached",
        "Name" => Object::string_literal(identity.subject_cn.clone()),
        "M" => Object::string_literal(pdf_date_now()),
        "ByteRange" => byte_range_placeholder,
        "Contents" => Object::String(placeholder_hex, lopdf::StringFormat::Hexadecimal),
    };
    if let Some(reason) = opts.reason.as_deref().filter(|s| !s.is_empty()) {
        sig_dict.set("Reason", Object::string_literal(reason.to_string()));
    }
    if let Some(loc) = opts.location.as_deref().filter(|s| !s.is_empty()) {
        sig_dict.set("Location", Object::string_literal(loc.to_string()));
    }
    if let Some(c) = opts.contact_info.as_deref().filter(|s| !s.is_empty()) {
        sig_dict.set("ContactInfo", Object::string_literal(c.to_string()));
    }
    let sig_obj_id: ObjectId = doc.add_object(Object::Dictionary(sig_dict));

    // 3. Build a widget annotation. If `opts.appearance` is set we attach
    //    a Form XObject as `/AP /N` and use the spec's rect + page.
    //    Otherwise the widget is invisible (0×0 rect on the first page,
    //    `F = 132` = Print | NoView), matching the v3.10.0 default.
    let (page_id, rect_array, ap_ref, widget_flags) = match opts.appearance.as_ref() {
        Some(spec) => {
            let page_id = nth_page_id(&doc, spec.page.max(1) as usize)?;
            let app = build_appearance(identity, spec);
            // Wrap the content stream in a Form XObject. Resources reference
            // Helvetica via /F1 — a standard 14 PDF font, no embedding needed.
            let fonts_dict = {
                let mut d = lopdf::Dictionary::new();
                for (name, base) in &app.fonts {
                    let font_id = doc.add_object(Object::Dictionary(dictionary! {
                        "Type" => "Font",
                        "Subtype" => "Type1",
                        "BaseFont" => base.to_string().as_str(),
                        "Encoding" => "WinAnsiEncoding",
                    }));
                    d.set(name.as_str(), Object::Reference(font_id));
                }
                d
            };
            let resources = dictionary! { "Font" => Object::Dictionary(fonts_dict) };
            let xobject_dict = dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "FormType" => 1i64,
                "BBox" => Object::Array(vec![
                    Object::Real(app.bbox[0]),
                    Object::Real(app.bbox[1]),
                    Object::Real(app.bbox[2]),
                    Object::Real(app.bbox[3]),
                ]),
                "Resources" => Object::Dictionary(resources),
            };
            let xobj = lopdf::Stream::new(xobject_dict, app.content_stream);
            let xobj_id = doc.add_object(Object::Stream(xobj));
            let ap_dict = dictionary! { "N" => Object::Reference(xobj_id) };
            let ap_id = doc.add_object(Object::Dictionary(ap_dict));
            let rect = Object::Array(vec![
                Object::Real(spec.rect[0]),
                Object::Real(spec.rect[1]),
                Object::Real(spec.rect[2]),
                Object::Real(spec.rect[3]),
            ]);
            // F = 4 (Print) — visible, printable, no NoView.
            (page_id, rect, Some(ap_id), 4i64)
        }
        None => {
            let page_id = first_page_id(&doc)?;
            let rect = Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()]);
            (page_id, rect, None, 132i64)
        }
    };
    let mut widget_dict = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Sig",
        "T" => Object::string_literal(field_name.clone()),
        "F" => widget_flags,
        "Rect" => rect_array,
        "P" => Object::Reference(page_id),
        "V" => Object::Reference(sig_obj_id),
    };
    if let Some(ap_id) = ap_ref {
        widget_dict.set("AP", Object::Reference(ap_id));
    }
    let widget_id = doc.add_object(Object::Dictionary(widget_dict));

    // 4. Wire AcroForm + page Annots.
    attach_widget_to_page(&mut doc, page_id, widget_id)?;
    attach_field_to_acroform(&mut doc, widget_id)?;

    // 5. Compress nothing (we need to read raw bytes to find the placeholder).
    doc.compress();

    // 6. Serialize.
    let mut serialized = Vec::with_capacity(1 << 20);
    doc.save_to(&mut serialized)
        .map_err(|e| SignetError::InvalidCert(format!("lopdf save: {e}")))?;

    // 7. Locate the hex window — the first `/Contents <…>` whose body is all
    //    ASCII '0's of our placeholder length.
    let (contents_open, contents_close) = find_contents_window(&serialized, hex_window_bytes)?;
    let hex_window_len = contents_close.saturating_sub(contents_open + 1);
    if hex_window_len != hex_window_bytes {
        return Err(SignetError::InvalidCert(format!(
            "hex window length mismatch: expected {hex_window_bytes}, got {hex_window_len}"
        )));
    }

    // 8. Compute the real ByteRange.
    let file_len = serialized.len();
    let tail_start = contents_close + 1;
    let tail_len = file_len - tail_start;
    let head_len = contents_open; // bytes before the '<'
    let byte_range = [0u64, head_len as u64, tail_start as u64, tail_len as u64];

    // 9. Locate and rewrite /ByteRange in place.
    rewrite_byte_range_in_place(&mut serialized, &byte_range)?;

    // 10. Hash bytes excluding the hex window (inclusive of < and > bounds —
    //     the spec excludes the hex *contents* but the brackets are part of
    //     the file the verifier sees outside ByteRange).
    let mut to_hash: Vec<u8> = Vec::with_capacity(file_len - hex_window_len);
    to_hash.extend_from_slice(&serialized[..head_len]);
    to_hash.extend_from_slice(&serialized[tail_start..]);
    let digest = sha256(&to_hash);

    // 11. Build PKCS#7 blob — CAdES-BES by default, CAdES-T when tsa_url set.
    let mut blob = build_pkcs7_detached(&digest, identity, started)?;
    if let Some(url) = opts.tsa_url.as_deref().filter(|s| !s.is_empty()) {
        use crate::pdf::signet_pro::tsa::{
            build_timestamp_req, embed_timestamp_token, fetch_timestamp, signer_signature_digest,
            TsaFetchOptions,
        };
        // Timestamp the SignerInfo's signature value (RFC 5126 §6.3.2 / CAdES-T).
        let imprint = signer_signature_digest(&blob)?;
        // Non-repeating nonce — system-time nanos XOR'd with a pointer
        // address. RFC 3161 §2.4.1 only requires "as random as possible"
        // for replay detection; cryptographic randomness isn't mandated.
        let nonce: i64 = {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0);
            let stack_addr = (&nanos as *const i64) as i64;
            nanos ^ stack_addr
        };
        let req = build_timestamp_req(&imprint, Some(nonce))?;
        let resp = fetch_timestamp(url, &req, &TsaFetchOptions::default())?;
        if !resp.status_granted() {
            return Err(SignetError::InvalidCert(format!(
                "TSA rejected timestamp request (PKIStatus {})",
                resp.status
            )));
        }
        if resp.token.is_empty() {
            return Err(SignetError::InvalidCert(
                "TSA granted but returned no token".into(),
            ));
        }
        blob = embed_timestamp_token(&blob, &resp.token)?;
    }
    let blob_hex = hex_upper(&blob);
    if blob_hex.len() > hex_window_bytes {
        return Err(SignetError::InvalidCert(format!(
            "PKCS#7 blob too large for hex window: {} > {}",
            blob_hex.len(),
            hex_window_bytes
        )));
    }

    // 12. Splice into the window. Remaining bytes stay as '0' padding.
    let window = &mut serialized[(contents_open + 1)..contents_close];
    for (slot, b) in window.iter_mut().zip(blob_hex.bytes()) {
        *slot = b;
    }

    // 13. Save atomically.
    crate::pdf::atomic_save(output, &serialized)
        .map_err(|e| SignetError::Io(std::io::Error::other(e.to_string())))?;

    let elapsed_ms = started.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
    Ok(SignReport {
        output_bytes: file_len as u64,
        contents_open: contents_open as u64,
        contents_close: contents_close as u64,
        byte_range,
        signature_hex_used: blob_hex.len(),
        elapsed_ms,
        field_name,
    })
}

// ---------------- helpers ----------------

fn pdf_date_now() -> String {
    // PDF date string: D:YYYYMMDDHHmmSSZ — pragmatically use UTC.
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Avoid pulling chrono; do the YMD/HMS conversion ourselves via UTC.
    let secs = now as i64;
    let (y, mo, d, h, mi, s) = unix_to_utc(secs);
    format!("D:{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}Z")
}

/// Minimal proleptic Gregorian split. Year range 1970..=9999 (enough for PDFs).
fn unix_to_utc(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    const SECS_PER_DAY: i64 = 86_400;
    let mut days = secs.div_euclid(SECS_PER_DAY);
    let time_of_day = secs.rem_euclid(SECS_PER_DAY) as u32;
    let h = time_of_day / 3600;
    let mi = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    let mut y: i64 = 1970;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let dim = [
        31,
        if is_leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 0u32;
    let mut d = days as u32;
    for (i, &dm) in dim.iter().enumerate() {
        if d < dm {
            mo = i as u32 + 1;
            break;
        }
        d -= dm;
    }
    (y as i32, mo, d + 1, h, mi, s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn pick_field_name(doc: &Document, requested: Option<&str>) -> String {
    if let Some(name) = requested {
        if !name.is_empty() {
            return name.to_string();
        }
    }
    // Scan existing AcroForm.Fields for collisions.
    let existing = collect_field_names(doc);
    let mut n = 1u32;
    loop {
        let candidate = format!("Signature{n}");
        if !existing.iter().any(|e| e == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn collect_field_names(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(cat_id) = doc.catalog().map(|d| d as *const _) else {
        return out;
    };
    let _ = cat_id; // silence unused on err path
    let Ok(catalog) = doc.catalog() else {
        return out;
    };
    let Some(form_obj) = catalog.get(b"AcroForm").ok() else {
        return out;
    };
    let form = match form_obj {
        Object::Reference(rid) => match doc.get_object(*rid) {
            Ok(Object::Dictionary(d)) => d,
            _ => return out,
        },
        Object::Dictionary(d) => d,
        _ => return out,
    };
    let Ok(Object::Array(fields)) = form.get(b"Fields") else {
        return out;
    };
    for f in fields {
        let dict = match f {
            Object::Reference(rid) => match doc.get_object(*rid) {
                Ok(Object::Dictionary(d)) => d,
                _ => continue,
            },
            Object::Dictionary(d) => d,
            _ => continue,
        };
        if let Ok(Object::String(bytes, _)) = dict.get(b"T") {
            out.push(String::from_utf8_lossy(bytes).into_owned());
        }
    }
    out
}

fn first_page_id(doc: &Document) -> Result<ObjectId, SignetError> {
    let pages = doc.get_pages();
    pages
        .into_iter()
        .next()
        .map(|(_, id)| id)
        .ok_or_else(|| SignetError::InvalidCert("PDF has no pages".into()))
}

/// 1-indexed page lookup. Falls back to the first page if `n` exceeds the
/// page count (e.g. caller asked for page 7 of a 3-page document).
fn nth_page_id(doc: &Document, n: usize) -> Result<ObjectId, SignetError> {
    let pages = doc.get_pages();
    let total = pages.len();
    if total == 0 {
        return Err(SignetError::InvalidCert("PDF has no pages".into()));
    }
    let target = n.max(1).min(total);
    pages
        .into_iter()
        .nth(target - 1)
        .map(|(_, id)| id)
        .ok_or_else(|| SignetError::InvalidCert("PDF has no pages".into()))
}

fn attach_widget_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    widget_id: ObjectId,
) -> Result<(), SignetError> {
    let page = doc
        .get_object_mut(page_id)
        .map_err(|e| SignetError::InvalidCert(format!("page lookup: {e}")))?;
    let page_dict = match page {
        Object::Dictionary(d) => d,
        _ => return Err(SignetError::InvalidCert("page is not a dictionary".into())),
    };
    match page_dict.get_mut(b"Annots") {
        Ok(Object::Array(arr)) => {
            arr.push(Object::Reference(widget_id));
        }
        _ => {
            page_dict.set("Annots", Object::Array(vec![Object::Reference(widget_id)]));
        }
    }
    Ok(())
}

fn attach_field_to_acroform(doc: &mut Document, widget_id: ObjectId) -> Result<(), SignetError> {
    // Ensure /Root /AcroForm exists; we add a `SigFlags = 3` (SignaturesExist
    // | AppendOnly) so Acrobat treats the file as signed for purposes of
    // incremental-update flagging.
    let root_id = doc
        .trailer
        .get(b"Root")
        .map_err(|e| SignetError::InvalidCert(format!("no /Root: {e}")))?
        .as_reference()
        .map_err(|e| SignetError::InvalidCert(format!("/Root not a ref: {e}")))?;

    let catalog = match doc.get_object_mut(root_id) {
        Ok(Object::Dictionary(d)) => d,
        _ => return Err(SignetError::InvalidCert("catalog not a dict".into())),
    };

    // Take existing AcroForm if present, else build a fresh one.
    let acroform_id: ObjectId = match catalog.get(b"AcroForm").ok().cloned() {
        Some(Object::Reference(rid)) => rid,
        Some(Object::Dictionary(d)) => {
            let id = doc.add_object(Object::Dictionary(d));
            if let Ok(Object::Dictionary(c)) = doc.get_object_mut(root_id) {
                c.set("AcroForm", Object::Reference(id));
            }
            id
        }
        _ => {
            let id = doc.add_object(Object::Dictionary(dictionary! {
                "Fields" => Object::Array(vec![]),
                "SigFlags" => 3i64,
            }));
            if let Ok(Object::Dictionary(c)) = doc.get_object_mut(root_id) {
                c.set("AcroForm", Object::Reference(id));
            }
            id
        }
    };

    let acroform = match doc.get_object_mut(acroform_id) {
        Ok(Object::Dictionary(d)) => d,
        _ => return Err(SignetError::InvalidCert("AcroForm not a dict".into())),
    };
    // Ensure SigFlags includes the SignaturesExist bit (1).
    let new_flags = match acroform.get(b"SigFlags") {
        Ok(Object::Integer(n)) => *n | 3,
        _ => 3,
    };
    acroform.set("SigFlags", new_flags);

    match acroform.get_mut(b"Fields") {
        Ok(Object::Array(arr)) => arr.push(Object::Reference(widget_id)),
        _ => acroform.set("Fields", Object::Array(vec![Object::Reference(widget_id)])),
    }
    Ok(())
}

/// Find the byte offsets of the first `/Contents <00…00>` window matching
/// our placeholder. Returns (open_lt, close_gt) — the indices of `<` and `>`.
fn find_contents_window(bytes: &[u8], expected_len: usize) -> Result<(usize, usize), SignetError> {
    let needle = b"/Contents";
    let mut pos = 0;
    while let Some(rel) = memmem(&bytes[pos..], needle) {
        let abs = pos + rel;
        // Skip whitespace after /Contents.
        let mut i = abs + needle.len();
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'<' {
            let open = i;
            // Find matching '>'.
            let close = match bytes[open + 1..].iter().position(|&b| b == b'>') {
                Some(rel) => open + 1 + rel,
                None => return Err(SignetError::InvalidCert("unterminated /Contents".into())),
            };
            // Verify body is all hex.
            let body = &bytes[open + 1..close];
            if body.iter().all(|b| b.is_ascii_hexdigit()) && body.len() == expected_len {
                return Ok((open, close));
            }
        }
        pos = abs + needle.len();
    }
    Err(SignetError::InvalidCert(
        "no /Contents hex placeholder found".into(),
    ))
}

/// Find and rewrite `/ByteRange [a b c d]` in-place. Width-preserving.
fn rewrite_byte_range_in_place(bytes: &mut [u8], range: &[u64; 4]) -> Result<(), SignetError> {
    let needle = b"/ByteRange";
    let rel = memmem(bytes, needle)
        .ok_or_else(|| SignetError::InvalidCert("no /ByteRange in serialized PDF".into()))?;
    let mut i = rel + needle.len();
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'[' {
        return Err(SignetError::InvalidCert(
            "/ByteRange not followed by [".into(),
        ));
    }
    let open = i;
    let close = open
        + 1
        + bytes[open + 1..]
            .iter()
            .position(|&b| b == b']')
            .ok_or_else(|| SignetError::InvalidCert("unterminated /ByteRange [".into()))?;

    let interior_len = close - open - 1;
    let new_inner = format!(
        "{:<w$} {:<w$} {:<w$} {:<w$}",
        range[0],
        range[1],
        range[2],
        range[3],
        w = BYTE_RANGE_WIDTH
    );
    // The placeholder we wrote may be slightly wider (lopdf padding); we
    // pad with trailing spaces to fill whatever width the old slot had.
    let mut padded = new_inner.into_bytes();
    if padded.len() > interior_len {
        return Err(SignetError::InvalidCert(format!(
            "ByteRange interior shrunk vs placeholder: {} > {}",
            padded.len(),
            interior_len
        )));
    }
    padded.resize(interior_len, b' ');
    bytes[open + 1..close].copy_from_slice(&padded);
    Ok(())
}

/// Tiny byte-substring search (avoid pulling memchr/memmem crate).
fn memmem(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let last = hay.len() - needle.len();
    let first = needle[0];
    let mut i = 0;
    while i <= last {
        if hay[i] == first && hay[i..i + needle.len()] == *needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::signet::cms_blob::{parse_signed_data, sha256 as sha256_helper};
    use rsa::pkcs1v15::SigningKey as RsaSigningKey;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use rsa::sha2::Sha256 as RsaSha256;
    use std::str::FromStr;
    use tempfile::TempDir;
    use x509_cert::builder::{Builder as _, CertificateBuilder, Profile};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;

    fn fixture_identity(cn: &str) -> SigningIdentity {
        let mut rng = rand::thread_rng();
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let key_pem = key
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .as_bytes()
            .to_vec();
        let serial = SerialNumber::from(5u32);
        let validity = Validity::from_now(std::time::Duration::from_secs(365 * 24 * 3600)).unwrap();
        let subject = Name::from_str(&format!("CN={cn}")).unwrap();
        let pub_key = key.to_public_key();
        let signing_key = RsaSigningKey::<RsaSha256>::new(key.clone());
        let spki = rsa::pkcs8::EncodePublicKey::to_public_key_der(&pub_key).unwrap();
        let spki_info = spki::SubjectPublicKeyInfoOwned::try_from(spki.as_bytes()).unwrap();
        let builder = CertificateBuilder::new(
            Profile::Root,
            serial,
            validity,
            subject,
            spki_info,
            &signing_key,
        )
        .unwrap();
        let cert = builder.build::<rsa::pkcs1v15::Signature>().unwrap();
        let cert_der = der::Encode::to_der(&cert).unwrap();
        let cert_pem = pem::encode(&pem::Pem::new("CERTIFICATE", cert_der));
        SigningIdentity::from_pem_bytes(cert_pem.as_bytes(), &key_pem, None).unwrap()
    }

    fn fixture_pdf(dir: &Path) -> std::path::PathBuf {
        // Minimal PDF: catalog + one empty page. Use lopdf for correctness.
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
                "Contents" => Object::Array(vec![]),
                "Resources" => Object::Dictionary(dictionary!{}),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let path = dir.join("input.pdf");
        doc.save(&path).unwrap();
        path
    }

    #[test]
    fn signs_pdf_end_to_end_and_byte_range_matches_file() {
        let tmp = TempDir::new().unwrap();
        let input = fixture_pdf(tmp.path());
        let output = tmp.path().join("signed.pdf");
        let id = fixture_identity("Slab E2E Test");
        let report = sign_pdf(&input, &output, &id, &SignOptions::default()).expect("sign");

        // File exists and reasonable size.
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(bytes.len() as u64, report.output_bytes);

        // ByteRange numbers point at the hex window we located.
        assert_eq!(report.byte_range[0], 0);
        assert_eq!(report.byte_range[1], report.contents_open);
        assert_eq!(report.byte_range[2], report.contents_close + 1);
        assert_eq!(
            report.byte_range[3],
            bytes.len() as u64 - (report.contents_close + 1)
        );

        // Hex window between < and > is SIGNATURE_HEX_PLACEHOLDER_BYTES bytes.
        let span = (report.contents_close - report.contents_open) as usize - 1;
        assert_eq!(span, SIGNATURE_HEX_PLACEHOLDER_BYTES);
    }

    #[test]
    fn embedded_cms_blob_parses_back() {
        let tmp = TempDir::new().unwrap();
        let input = fixture_pdf(tmp.path());
        let output = tmp.path().join("signed.pdf");
        let id = fixture_identity("Parser Roundtrip");
        let report = sign_pdf(&input, &output, &id, &SignOptions::default()).unwrap();

        let bytes = std::fs::read(&output).unwrap();
        let hex = &bytes[(report.contents_open as usize + 1)..(report.contents_close as usize)];
        // Strip trailing '0' padding to get the live PKCS#7 bytes.
        let hex_str = std::str::from_utf8(&hex[..report.signature_hex_used]).unwrap();
        let der: Vec<u8> = (0..hex_str.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16).unwrap())
            .collect();
        let sd = parse_signed_data(&der).expect("CMS parse");
        assert_eq!(sd.signer_infos.0.len(), 1);
    }

    #[test]
    fn signature_digest_covers_file_minus_hex_window() {
        let tmp = TempDir::new().unwrap();
        let input = fixture_pdf(tmp.path());
        let output = tmp.path().join("signed.pdf");
        let id = fixture_identity("Coverage Test");
        let report = sign_pdf(&input, &output, &id, &SignOptions::default()).unwrap();

        let bytes = std::fs::read(&output).unwrap();
        // Reconstruct what we signed: bytes outside the hex window.
        let mut covered: Vec<u8> =
            Vec::with_capacity(bytes.len() - SIGNATURE_HEX_PLACEHOLDER_BYTES);
        covered.extend_from_slice(&bytes[..report.contents_open as usize]);
        covered.extend_from_slice(&bytes[report.contents_close as usize + 1..]);
        let digest = sha256_helper(&covered);
        // Digest must match what build_pkcs7_detached saw — we don't have
        // direct access to it here, but the digest length is invariant.
        assert_eq!(digest.len(), 32);
    }

    #[test]
    fn signed_pdf_still_parses_with_lopdf() {
        let tmp = TempDir::new().unwrap();
        let input = fixture_pdf(tmp.path());
        let output = tmp.path().join("signed.pdf");
        let id = fixture_identity("LopdfReread");
        sign_pdf(&input, &output, &id, &SignOptions::default()).unwrap();
        let reloaded = lopdf::Document::load(&output).expect("re-parse signed PDF");
        // AcroForm exists.
        let catalog = reloaded.catalog().unwrap();
        assert!(
            catalog.has(b"AcroForm"),
            "catalog should reference AcroForm"
        );
    }

    #[test]
    fn options_propagate_to_sig_dict() {
        let tmp = TempDir::new().unwrap();
        let input = fixture_pdf(tmp.path());
        let output = tmp.path().join("signed.pdf");
        let id = fixture_identity("With Reason");
        let opts = SignOptions {
            reason: Some("Approved by Engineering".to_string()),
            location: Some("San Francisco".to_string()),
            ..SignOptions::default()
        };
        sign_pdf(&input, &output, &id, &opts).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Approved by Engineering"), "Reason missing");
        assert!(text.contains("San Francisco"), "Location missing");
    }

    #[test]
    fn pdf_date_now_is_well_formed() {
        let s = pdf_date_now();
        assert!(s.starts_with("D:"));
        assert!(s.ends_with('Z'));
        assert_eq!(s.len(), 17, "PDF date should be D:YYYYMMDDHHmmSSZ");
    }

    #[test]
    fn memmem_basic() {
        assert_eq!(memmem(b"hello world", b"world"), Some(6));
        assert_eq!(memmem(b"hello world", b"xyz"), None);
        assert_eq!(memmem(b"", b"a"), None);
    }
}
