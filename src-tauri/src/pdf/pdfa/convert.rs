//! `convert_to_pdfa` — the v3.0.0 Bedrock orchestrator.
//!
//! Composes every pass we ship in this release into one end-to-end
//! conversion:
//!
//!   1. Load the input PDF (`lopdf::Document::load`). Reject encrypted.
//!   2. Run the read-only [`font_audit`](super::font_audit) — if any font
//!      is missing its embedded program AND the caller didn't pass
//!      [`ConvertOpts::skip_font_check`], bail with a clear, actionable
//!      error that names the offending fonts. Standard-14 fonts (Helvetica,
//!      Times, Courier, Symbol, ZapfDingbats) are flagged because PDF/A
//!      forbids the spec-default substitution behaviour.
//!   3. [`sanitize_for_pdfa`](super::sanitize::sanitize_for_pdfa) — strip
//!      ISO 19005-2 §6.6 forbidden interactive features.
//!   4. [`inject_output_intent_and_metadata`](super::output_intent::inject_output_intent_and_metadata)
//!      — splice sRGB v4 ICC OutputIntent + XMP metadata into the catalog.
//!   5. Serialize the document to a buffer with `doc.save_to(&mut buf)`.
//!   6. Re-load that buffer and run [`validate_doc`](super::validate::validate_doc)
//!      so we ship a report that reflects what's on disk, not what's
//!      hopefully on disk.
//!   7. Atomic write the buffer to `output` via [`crate::pdf::atomic_save`].
//!
//! The orchestrator is **non-destructive to fonts** — the mutating
//! embedding pass is deferred to v3.0.1 (Slice 4). For now we refuse
//! the conversion when fonts are missing rather than emit invalid
//! PDF/A; the alternative is silently producing a non-conformant file
//! and that's exactly what Adobe / Ghostscript do today (and what we
//! refuse to do).

use super::font_audit::audit_fonts;
use super::output_intent::inject_output_intent_and_metadata;
use super::sanitize::sanitize_for_pdfa;
use super::validate::{validate_doc, ValidationReport};
use super::xmp::XmpMetadata;
use super::ConformanceLevel;
use crate::pdf::{atomic_save, PdfError};
use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Knobs the UI / CLI may pass through to the orchestrator.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConvertOpts {
    /// Target PDF/A level (2b default, 3b allows file attachments).
    pub level: ConformanceLevel,
    /// Optional `dc:title` to write into XMP. Falls back to the
    /// existing `Info` dictionary title if unset.
    pub title: Option<String>,
    /// Optional `dc:creator` to write into XMP.
    pub author: Option<String>,
    /// Optional `dc:description` for the XMP packet.
    pub subject: Option<String>,
    /// Skip the font-audit gate. Use only when the caller has already
    /// shown the user the audit and they explicitly accepted that
    /// missing-font glyphs will fall back to the viewer's substitute.
    /// The resulting file will not pass a strict ISO 19005-2 validator
    /// but it WILL render correctly in every modern reader.
    pub skip_font_check: bool,
}

/// What the orchestrator did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertReport {
    pub level: ConformanceLevel,
    /// True when sanitize stripped at least one entry.
    pub sanitized_entries: Vec<String>,
    /// True when an OutputIntent dictionary was newly added.
    pub added_output_intent: bool,
    /// True when the catalog's `/Metadata` reference was (re)written.
    pub added_xmp_metadata: bool,
    /// Total number of fonts discovered by the audit.
    pub font_count: usize,
    /// Fonts the audit flagged as missing embedded data. Empty on a
    /// successful conversion (unless `skip_font_check` was set).
    pub fonts_missing_embed: Vec<String>,
    /// Bytes written to disk.
    pub output_bytes: usize,
    /// Validation report computed from the re-loaded output file. The
    /// `valid` field on this is the canonical answer to "did the
    /// conversion succeed?".
    pub validation: ValidationReport,
}

/// Top-level entry point. Reads `input`, writes `output`, returns the report.
pub fn convert_to_pdfa(
    input: &Path,
    output: &Path,
    opts: ConvertOpts,
) -> Result<ConvertReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if output.as_os_str().is_empty() {
        return Err(PdfError::EmptyOutput);
    }

    let mut doc = Document::load(input).map_err(PdfError::from)?;

    // 1) Font audit — read-only gate.
    let audit = audit_fonts(&doc);
    let mut fonts_missing: Vec<String> = audit
        .missing_embed()
        .iter()
        .map(|f| f.name.clone())
        .collect();
    fonts_missing.sort();
    fonts_missing.dedup();
    if !fonts_missing.is_empty() && !opts.skip_font_check {
        return Err(PdfError::Other(format!(
            "PDF/A conversion blocked: {} font(s) are not embedded — {}. \
             Re-export from the source application with 'embed all fonts' \
             enabled, or pass skip_font_check to convert anyway (the output \
             will render correctly but won't pass strict PDF/A validators).",
            fonts_missing.len(),
            fonts_missing.join(", ")
        )));
    }

    // 2) Sanitize — strip §6.6 forbidden entries.
    let sanitize_report =
        sanitize_for_pdfa(&mut doc).map_err(|e| PdfError::Other(format!("sanitize: {e}")))?;

    // 3) XMP + OutputIntent injection.
    let mut meta = XmpMetadata::new(opts.level);
    meta.title = opts.title.clone();
    meta.author = opts.author.clone();
    meta.subject = opts.subject.clone();
    let inject_report = inject_output_intent_and_metadata(&mut doc, &meta)
        .map_err(|e| PdfError::Other(format!("inject: {e}")))?;

    // 4) Serialize to a buffer.
    let mut buf: Vec<u8> = Vec::with_capacity(1 << 16);
    doc.save_to(&mut buf).map_err(PdfError::from)?;

    // 5) Validate the SERIALIZED output (round-trip safety).
    let mut reloaded = Document::load_mem(&buf).map_err(PdfError::from)?;
    // Re-injection on the reloaded doc is unnecessary; we just validate.
    let _ = &mut reloaded; // silence unused mut on some lopdf versions
    let validation = validate_doc(&reloaded, opts.level);

    // 6) Atomic write to disk.
    let out_len = buf.len();
    atomic_save(output, &buf)?;

    Ok(ConvertReport {
        level: opts.level,
        sanitized_entries: sanitize_report.removed,
        added_output_intent: inject_report.added_output_intent,
        added_xmp_metadata: inject_report.added_xmp_metadata,
        font_count: audit.count(),
        fonts_missing_embed: fonts_missing,
        output_bytes: out_len,
        validation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};
    use std::fs;
    use tempfile::tempdir;

    fn write_minimal_pdf_no_fonts(path: &Path) {
        // A 1-page PDF with NO font resources — passes our audit
        // trivially and lets us exercise the happy path of the
        // orchestrator without bundling a real binary fixture.
        let mut doc = Document::with_version("1.7");
        // Empty content stream so the page has SOMETHING.
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b" ".to_vec())));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.compress();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        fs::write(path, &buf).unwrap();
    }

    fn write_minimal_pdf_with_unembedded_helvetica(path: &Path) {
        // Standard-14 Helvetica with no FontDescriptor — flagged as
        // not embedded by audit_fonts. We use it to exercise the gate.
        let mut doc = Document::with_version("1.7");
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b" ".to_vec())));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => Object::Reference(font_id),
                },
            },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.compress();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        fs::write(path, &buf).unwrap();
    }

    #[test]
    fn happy_path_no_fonts_produces_valid_pdfa() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        write_minimal_pdf_no_fonts(&input);

        let report = convert_to_pdfa(&input, &output, ConvertOpts::default()).unwrap();

        assert!(output.exists());
        assert!(report.output_bytes > 0);
        assert_eq!(report.font_count, 0);
        assert!(report.fonts_missing_embed.is_empty());
        assert!(report.added_output_intent);
        assert!(report.added_xmp_metadata);
        // The output, re-loaded, should validate.
        assert!(
            report.validation.passed(),
            "expected output to validate; findings: {:?}",
            report.validation.findings
        );
    }

    #[test]
    fn blocks_when_fonts_not_embedded() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        write_minimal_pdf_with_unembedded_helvetica(&input);

        let err = convert_to_pdfa(&input, &output, ConvertOpts::default()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Helvetica"),
            "expected error to name Helvetica, got: {msg}"
        );
        assert!(!output.exists(), "output must not be written on font-block");
    }

    #[test]
    fn skip_font_check_writes_output_anyway() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        write_minimal_pdf_with_unembedded_helvetica(&input);

        let opts = ConvertOpts {
            skip_font_check: true,
            ..Default::default()
        };
        let report = convert_to_pdfa(&input, &output, opts).unwrap();
        assert!(output.exists());
        assert_eq!(report.fonts_missing_embed, vec!["Helvetica".to_string()]);
        // Output exists and was injected — even if a strict validator
        // would still flag the unembedded font.
        assert!(report.added_output_intent);
    }

    #[test]
    fn rejects_missing_input() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("nope.pdf");
        let output = dir.path().join("out.pdf");
        let err = convert_to_pdfa(&input, &output, ConvertOpts::default()).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }

    #[test]
    fn rejects_empty_output_path() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        write_minimal_pdf_no_fonts(&input);
        let err = convert_to_pdfa(&input, Path::new(""), ConvertOpts::default()).unwrap_err();
        assert!(matches!(err, PdfError::EmptyOutput));
    }

    #[test]
    fn idempotent_double_convert() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let mid = dir.path().join("mid.pdf");
        let out = dir.path().join("out.pdf");
        write_minimal_pdf_no_fonts(&input);
        convert_to_pdfa(&input, &mid, ConvertOpts::default()).unwrap();
        let r2 = convert_to_pdfa(&mid, &out, ConvertOpts::default()).unwrap();
        // Second pass should still validate; OutputIntent already
        // present from the first pass so it's NOT added again.
        assert!(r2.validation.passed());
        assert!(!r2.added_output_intent, "should be idempotent");
        assert!(r2.added_xmp_metadata, "XMP is always (re)written");
    }

    #[test]
    fn writes_xmp_title_when_provided() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        write_minimal_pdf_no_fonts(&input);

        let opts = ConvertOpts {
            title: Some("Quarterly Report Q4 2026".into()),
            author: Some("Cake".into()),
            ..Default::default()
        };
        convert_to_pdfa(&input, &output, opts).unwrap();

        // Re-load and grep the XMP packet for the title we set.
        let doc = Document::load(&output).unwrap();
        let cat = doc.catalog().unwrap();
        let meta_id = match cat.get(b"Metadata").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("Metadata is not a reference"),
        };
        let stream = doc.get_object(meta_id).unwrap().as_stream().unwrap();
        let xmp = std::str::from_utf8(&stream.content).unwrap();
        assert!(xmp.contains("Quarterly Report Q4 2026"));
        assert!(xmp.contains("Cake"));
    }
}
