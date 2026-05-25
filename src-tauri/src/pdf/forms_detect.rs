//! v3.27.0 "Quill Auto-Detect" — propose AcroForm fields on flat PDFs.
//!
//! Reads a flat PDF (one with no `/AcroForm`), walks each page's content
//! stream for graphics primitives (horizontal rules, small stroked rectangles)
//! and text runs, and emits a [`DetectionReport`] of [`FieldCandidate`]s the
//! frontend can review before forwarding to v3.26.0 Designer's
//! `slab_forms_design_add` to commit them as real interactive fields.
//!
//! Heuristics implemented in v1:
//!   - **HorizontalRule** → Text field with writing room above the line
//!   - **EmptyBox** (8–22pt square stroked) → Checkbox
//!   - **LabeledBlank** — label text ending in `:` left of a rule → Text +
//!     boosted confidence
//!   - **SignatureLine** — label matches `signature|sign here|signed by|x` → Signature
//!
//! Out of scope for v1: XFA, multi-CTM transforms, complex graphics state
//! tracking, OCR-only scans (file v0.13.0 Lens for that), and field type
//! disambiguation between checkbox and radio. The detector errs on the
//! conservative side and returns confidence scores so the UI can sort
//! low-confidence picks to the top for review.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::pdf::PdfError;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// What sort of AcroForm field the detector thinks this candidate should
/// become. Tagged enum that serializes as `{"kind":"text",...}` etc. so the
/// TS side can pattern-match on the `kind` discriminant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CandidateKind {
    Text { multiline: bool },
    Checkbox,
    Signature,
}

/// Where the detector got this candidate from. Surfaced in the UI as a
/// little "why?" tooltip so the reviewer knows whether to trust it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Evidence {
    /// A thin horizontal line with no text on it (an underline-blank).
    HorizontalRule { line_width_pt: f32 },
    /// A small square stroked rectangle (printed checkbox glyph).
    EmptyBox { side_pt: f32 },
    /// A short text run ending with a colon followed by a rule to the right.
    LabeledBlank,
    /// A "Signature:" or "Sign here" label followed by a long rule.
    SignatureLine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldCandidate {
    pub suggested_name: String,
    #[serde(flatten)]
    pub kind: CandidateKind,
    pub page: u32,
    pub rect: [f32; 4],
    pub label: Option<String>,
    pub evidence: Evidence,
    /// 0.0..1.0 — calibrated against the test corpus, surfaced in the UI so
    /// the reviewer can sort low-confidence picks to the top.
    pub confidence: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectionReport {
    pub candidates: Vec<FieldCandidate>,
    pub pages_scanned: u32,
    /// True if the PDF already has an `/AcroForm` — the UI warns before
    /// committing (we'd merge, not replace, but the user should know).
    pub already_has_acroform: bool,
    /// Soft warnings (e.g. could not decode a page's content stream).
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Run the detector against a PDF file on disk. Pure-CPU, no network.
pub fn detect(input: &Path) -> Result<DetectionReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = lopdf::Document::load(input)?;
    detect_doc(&doc)
}

/// Lower-level entry point operating on an already-loaded document. Handy
/// for tests that build the PDF in memory via `lopdf::Document::new`.
pub fn detect_doc(doc: &lopdf::Document) -> Result<DetectionReport, PdfError> {
    let pages = doc.get_pages();
    let already_has_acroform = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .is_some();
    let mut report = DetectionReport {
        candidates: Vec::new(),
        pages_scanned: pages.len() as u32,
        already_has_acroform,
        warnings: Vec::new(),
    };
    let mut page_nums: Vec<u32> = pages.keys().copied().collect();
    page_nums.sort_unstable();
    for page_no in page_nums {
        match detect_page(doc, page_no) {
            Ok(mut cs) => report.candidates.append(&mut cs),
            Err(e) => report.warnings.push(format!("page {page_no}: {e}")),
        }
    }
    dedupe_names(&mut report.candidates);
    Ok(report)
}

pub fn detect_page(doc: &lopdf::Document, page: u32) -> Result<Vec<FieldCandidate>, PdfError> {
    let prims = geom::walk_page(doc, page)?;
    let runs = text::walk_page(doc, page)?;
    let mut out = Vec::new();

    // (1) Small stroked rects → Checkbox candidates.
    for r in &prims.rects {
        let w = r.x1 - r.x0;
        let h = r.y1 - r.y0;
        if (8.0..=22.0).contains(&w) && (8.0..=22.0).contains(&h) && (w - h).abs() < 2.0 {
            let cy = (r.y0 + r.y1) / 2.0;
            let label = nearest_label_to_the_right(&runs, r.x1, cy, 120.0)
                .or_else(|| nearest_label_to_the_left(&runs, r.x0, cy, 120.0));
            out.push(FieldCandidate {
                suggested_name: slugify(label.as_deref().unwrap_or("checkbox")),
                kind: CandidateKind::Checkbox,
                page,
                rect: [r.x0, r.y0, r.x1, r.y1],
                label,
                evidence: Evidence::EmptyBox { side_pt: w },
                confidence: 0.70,
            });
        }
    }

    // (2) Thin horizontal rules → Text / Signature / LabeledBlank.
    for line in &prims.lines {
        let dy = (line.y1 - line.y0).abs();
        let dx = (line.x1 - line.x0).abs();
        if dy < 1.0 && dx > 50.0 && line.width < 2.0 {
            // The rule's baseline ≈ y0. The label, if any, sits to the left
            // of the rule at roughly the same baseline (or slightly above).
            let label_raw = label_left_of_rule(&runs, line.x0.min(line.x1), line.y0, 4.0, 40.0);
            let is_sig = label_raw
                .as_deref()
                .map(is_signature_label)
                .unwrap_or(false);
            let kind = if is_sig {
                CandidateKind::Signature
            } else {
                CandidateKind::Text { multiline: false }
            };
            let evidence = if is_sig {
                Evidence::SignatureLine
            } else if label_raw.is_some() {
                Evidence::LabeledBlank
            } else {
                Evidence::HorizontalRule {
                    line_width_pt: line.width,
                }
            };
            let mut confidence = 0.55_f32;
            if dx > 80.0 {
                confidence += 0.15;
            }
            if label_raw.is_some() {
                confidence += 0.20;
            }
            let label_clean = label_raw
                .as_deref()
                .map(strip_trailing_colon)
                .map(str::to_string);
            let (x0, x1) = (line.x0.min(line.x1), line.x0.max(line.x1));
            out.push(FieldCandidate {
                suggested_name: slugify(label_clean.as_deref().unwrap_or("text")),
                kind,
                page,
                rect: [x0, line.y0 - 2.0, x1, line.y0 + 14.0],
                label: label_clean,
                evidence,
                confidence: confidence.min(1.0),
            });
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_us = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("field");
    }
    out
}

fn strip_trailing_colon(s: &str) -> &str {
    s.trim().trim_end_matches(':').trim()
}

fn is_signature_label(s: &str) -> bool {
    let s = s.to_lowercase();
    let s_trim = s.trim();
    s.contains("signature")
        || s.contains("sign here")
        || s.contains("signed by")
        || s_trim == "x"
        || s_trim == "x:"
}

fn approx_width(s: &str, size: f32) -> f32 {
    (s.chars().count() as f32) * size * 0.5
}

fn nearest_label_to_the_right(
    runs: &[text::TextRun],
    x: f32,
    y: f32,
    max_dx: f32,
) -> Option<String> {
    runs.iter()
        .filter(|r| (r.y - y).abs() < 6.0 && r.x > x && (r.x - x) < max_dx)
        .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        .map(|r| strip_trailing_colon(&r.text).to_string())
}

fn nearest_label_to_the_left(
    runs: &[text::TextRun],
    x: f32,
    y: f32,
    max_dx: f32,
) -> Option<String> {
    runs.iter()
        .filter(|r| (r.y - y).abs() < 6.0 && r.x < x && (x - r.x) < max_dx)
        .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        .map(|r| strip_trailing_colon(&r.text).to_string())
}

fn label_left_of_rule(
    runs: &[text::TextRun],
    rule_x0: f32,
    rule_y: f32,
    max_above: f32,
    max_right_gap: f32,
) -> Option<String> {
    runs.iter()
        .filter(|r| {
            // Label baseline must be within `max_above` pt of the rule's
            // baseline (typically text sits *just above* the line at the
            // same y when both originate from "Name: ____" patterns; PDFs
            // also draw lines exactly at the baseline so |dy| < 2 is most
            // common). We accept up to 14pt above for safety.
            (r.y - rule_y).abs() <= max_above.max(14.0)
                && r.x < rule_x0
                && (rule_x0 - r.x - approx_width(&r.text, r.size)) < max_right_gap
        })
        .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        .map(|r| r.text.clone())
}

fn dedupe_names(cands: &mut [FieldCandidate]) {
    use std::collections::HashMap;
    let mut counts: HashMap<String, u32> = HashMap::new();
    for c in cands.iter_mut() {
        let n = counts.entry(c.suggested_name.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            c.suggested_name = format!("{}_{}", c.suggested_name, n);
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry walker
// ---------------------------------------------------------------------------

pub(crate) mod geom {
    use crate::pdf::PdfError;
    use lopdf::{content::Content, Document};

    #[derive(Debug, Clone, Copy)]
    pub struct Line {
        pub x0: f32,
        pub y0: f32,
        pub x1: f32,
        pub y1: f32,
        pub width: f32,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct Rect {
        pub x0: f32,
        pub y0: f32,
        pub x1: f32,
        pub y1: f32,
    }

    #[derive(Debug, Default, Clone)]
    pub struct PagePrims {
        pub lines: Vec<Line>,
        pub rects: Vec<Rect>,
    }

    /// Walk a single page's content stream and emit recognised primitives.
    ///
    /// v1 keeps the graphics state intentionally tiny — current point,
    /// pending rect, and line width — and ignores CTM transforms. The
    /// detector still produces useful results on the vast majority of
    /// printed forms (which use identity CTM and absolute coords).
    pub fn walk_page(doc: &Document, page_1based: u32) -> Result<PagePrims, PdfError> {
        let page_id = *doc
            .get_pages()
            .get(&page_1based)
            .ok_or_else(|| PdfError::Other(format!("page {page_1based} missing")))?;
        let data = doc
            .get_page_content(page_id)
            .map_err(|e| PdfError::Other(e.to_string()))?;
        let content = Content::decode(&data).map_err(|e| PdfError::Other(e.to_string()))?;
        let mut out = PagePrims::default();
        let mut lw: f32 = 1.0;
        let mut path_cur: Option<(f32, f32)> = None;
        let mut pending_lines: Vec<Line> = Vec::new();
        let mut pending_rect: Option<(f32, f32, f32, f32)> = None;

        for op in content.operations {
            match op.operator.as_str() {
                "w" => {
                    if let Some(v) = op.operands.first().and_then(num) {
                        lw = v;
                    }
                }
                "m" => {
                    if let (Some(x), Some(y)) = (
                        op.operands.first().and_then(num),
                        op.operands.get(1).and_then(num),
                    ) {
                        path_cur = Some((x, y));
                    }
                }
                "l" => {
                    if let (Some(x), Some(y), Some((sx, sy))) = (
                        op.operands.first().and_then(num),
                        op.operands.get(1).and_then(num),
                        path_cur,
                    ) {
                        pending_lines.push(Line {
                            x0: sx,
                            y0: sy,
                            x1: x,
                            y1: y,
                            width: lw,
                        });
                        path_cur = Some((x, y));
                    }
                }
                "re" => {
                    if let (Some(x), Some(y), Some(w), Some(h)) = (
                        op.operands.first().and_then(num),
                        op.operands.get(1).and_then(num),
                        op.operands.get(2).and_then(num),
                        op.operands.get(3).and_then(num),
                    ) {
                        let (x0, y0, x1, y1) =
                            (x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h));
                        pending_rect = Some((x0, y0, x1, y1));
                    }
                }
                "S" | "s" | "B" | "b" | "B*" | "b*" => {
                    out.lines.append(&mut pending_lines);
                    if let Some((x0, y0, x1, y1)) = pending_rect.take() {
                        out.rects.push(Rect { x0, y0, x1, y1 });
                    }
                    path_cur = None;
                }
                "f" | "F" | "f*" | "n" => {
                    // Filled-only or no-paint — drop any pending primitives.
                    pending_lines.clear();
                    pending_rect = None;
                    path_cur = None;
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub(super) fn num(o: &lopdf::Object) -> Option<f32> {
        match o {
            lopdf::Object::Integer(i) => Some(*i as f32),
            lopdf::Object::Real(r) => Some(*r),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Text walker
// ---------------------------------------------------------------------------

pub(crate) mod text {
    use crate::pdf::PdfError;
    use lopdf::{content::Content, Document};

    #[derive(Debug, Clone)]
    pub struct TextRun {
        pub text: String,
        pub x: f32,
        pub y: f32,
        pub size: f32,
    }

    pub fn walk_page(doc: &Document, page_1based: u32) -> Result<Vec<TextRun>, PdfError> {
        let page_id = *doc
            .get_pages()
            .get(&page_1based)
            .ok_or_else(|| PdfError::Other(format!("page {page_1based} missing")))?;
        let data = doc
            .get_page_content(page_id)
            .map_err(|e| PdfError::Other(e.to_string()))?;
        let content = Content::decode(&data).map_err(|e| PdfError::Other(e.to_string()))?;
        let mut out = Vec::new();
        let mut tx: f32 = 0.0;
        let mut ty: f32 = 0.0;
        let mut tf: f32 = 12.0;
        let mut in_text = false;

        for op in content.operations {
            match op.operator.as_str() {
                "BT" => {
                    in_text = true;
                    tx = 0.0;
                    ty = 0.0;
                }
                "ET" => {
                    in_text = false;
                }
                "Tf" => {
                    if let Some(s) = op.operands.get(1).and_then(super::geom::num) {
                        tf = s;
                    }
                }
                "Td" | "TD" => {
                    if let (Some(x), Some(y)) = (
                        op.operands.first().and_then(super::geom::num),
                        op.operands.get(1).and_then(super::geom::num),
                    ) {
                        tx += x;
                        ty += y;
                    }
                }
                "Tm" => {
                    // Text matrix set: 6 operands a b c d e f → tx=e, ty=f
                    if let (Some(e), Some(f)) = (
                        op.operands.get(4).and_then(super::geom::num),
                        op.operands.get(5).and_then(super::geom::num),
                    ) {
                        tx = e;
                        ty = f;
                    }
                }
                "T*" => {
                    ty -= tf * 1.2;
                }
                "Tj" | "'" if in_text => {
                    if let Some(lopdf::Object::String(bytes, _)) = op.operands.first() {
                        let s = String::from_utf8_lossy(bytes).into_owned();
                        if !s.is_empty() {
                            out.push(TextRun {
                                text: s,
                                x: tx,
                                y: ty,
                                size: tf,
                            });
                        }
                    }
                }
                "TJ" if in_text => {
                    if let Some(lopdf::Object::Array(arr)) = op.operands.first() {
                        let mut s = String::new();
                        for el in arr {
                            if let lopdf::Object::String(b, _) = el {
                                s.push_str(&String::from_utf8_lossy(b));
                            }
                        }
                        if !s.is_empty() {
                            out.push(TextRun {
                                text: s,
                                x: tx,
                                y: ty,
                                size: tf,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test_helpers {
    use lopdf::{dictionary, Document, Object, Stream};

    /// Build a minimal 1-page 612x792 PDF wrapping the given content stream.
    pub fn pdf_with_content(stream: &[u8]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let content_id = doc.add_object(Stream::new(dictionary! {}, stream.to_vec()));
        let resources_id = doc.add_object(dictionary! {});
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![page_id.into()],
        });
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
            d.set("Parent", pages_id);
        }
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_serializes_with_kind_tag() {
        let c = FieldCandidate {
            suggested_name: "applicant_name".into(),
            kind: CandidateKind::Text { multiline: false },
            page: 1,
            rect: [100.0, 700.0, 400.0, 720.0],
            label: Some("Applicant Name".into()),
            evidence: Evidence::HorizontalRule { line_width_pt: 0.5 },
            confidence: 0.82,
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"kind\":\"text\""), "got {j}");
        assert!(j.contains("\"confidence\":0.82"), "got {j}");
        assert!(
            j.contains("\"evidence\":{\"type\":\"horizontal_rule\""),
            "got {j}"
        );
    }

    #[test]
    fn slugify_handles_spaces_and_punctuation() {
        assert_eq!(slugify("Applicant Name:"), "applicant_name");
        assert_eq!(slugify("  Phone (cell) "), "phone_cell");
        assert_eq!(slugify("---"), "field");
        assert_eq!(slugify("ZIP/Postal"), "zip_postal");
    }

    #[test]
    fn strip_trailing_colon_strips_and_trims() {
        assert_eq!(strip_trailing_colon("Name:"), "Name");
        assert_eq!(strip_trailing_colon("Name :  "), "Name");
        assert_eq!(strip_trailing_colon("Name"), "Name");
    }

    #[test]
    fn is_signature_label_matches_common_phrasings() {
        assert!(is_signature_label("Signature:"));
        assert!(is_signature_label("Sign here"));
        assert!(is_signature_label("signed by"));
        assert!(is_signature_label("X"));
        assert!(!is_signature_label("Name:"));
    }

    #[test]
    fn dedupe_names_appends_suffix_to_duplicates() {
        let mut cs = vec![
            FieldCandidate {
                suggested_name: "name".into(),
                kind: CandidateKind::Text { multiline: false },
                page: 1,
                rect: [0.0; 4],
                label: None,
                evidence: Evidence::HorizontalRule { line_width_pt: 0.5 },
                confidence: 0.5,
            },
            FieldCandidate {
                suggested_name: "name".into(),
                kind: CandidateKind::Text { multiline: false },
                page: 1,
                rect: [0.0; 4],
                label: None,
                evidence: Evidence::HorizontalRule { line_width_pt: 0.5 },
                confidence: 0.5,
            },
        ];
        dedupe_names(&mut cs);
        assert_eq!(cs[0].suggested_name, "name");
        assert_eq!(cs[1].suggested_name, "name_2");
    }

    #[test]
    fn geom_walker_finds_horizontal_line_and_box() {
        let pdf =
            test_helpers::pdf_with_content(b"q 0.5 w 100 720 m 300 720 l S 100 650 12 12 re s Q");
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let prims = geom::walk_page(&doc, 1).unwrap();
        assert_eq!(prims.lines.len(), 1, "lines: {:?}", prims.lines);
        assert_eq!(prims.rects.len(), 1, "rects: {:?}", prims.rects);
        let l = &prims.lines[0];
        assert!((l.x0 - 100.0).abs() < 0.01);
        assert!((l.x1 - 300.0).abs() < 0.01);
        assert!((l.y0 - 720.0).abs() < 0.01);
        let r = &prims.rects[0];
        assert!((r.x0 - 100.0).abs() < 0.01);
        assert!((r.y0 - 650.0).abs() < 0.01);
        assert!((r.x1 - 112.0).abs() < 0.01);
        assert!((r.y1 - 662.0).abs() < 0.01);
    }

    #[test]
    fn geom_walker_drops_unstroked_subpath() {
        // `n` op = no paint; rect should NOT show up.
        let pdf = test_helpers::pdf_with_content(b"100 650 12 12 re n");
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let prims = geom::walk_page(&doc, 1).unwrap();
        assert!(prims.rects.is_empty());
    }

    #[test]
    fn text_walker_finds_labeled_runs() {
        let pdf = test_helpers::pdf_with_content(b"BT /F1 12 Tf 100 720 Td (Name:) Tj ET");
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let runs = text::walk_page(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Name:");
        assert!((runs[0].x - 100.0).abs() < 0.01);
        assert!((runs[0].y - 720.0).abs() < 0.01);
        assert!((runs[0].size - 12.0).abs() < 0.01);
    }

    #[test]
    fn text_walker_supports_tj_array_and_tm() {
        let pdf = test_helpers::pdf_with_content(
            b"BT /F1 10 Tf 1 0 0 1 200 600 Tm [(Hel) -50 (lo)] TJ ET",
        );
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let runs = text::walk_page(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello");
        assert!((runs[0].x - 200.0).abs() < 0.01);
        assert!((runs[0].y - 600.0).abs() < 0.01);
    }

    #[test]
    fn detects_horizontal_rule_as_text() {
        let pdf = test_helpers::pdf_with_content(b"q 0.5 w 100 600 m 400 600 l S Q");
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let cands = detect_page(&doc, 1).unwrap();
        assert_eq!(cands.len(), 1);
        assert!(matches!(cands[0].kind, CandidateKind::Text { .. }));
        assert!(cands[0].label.is_none());
        assert!(cands[0].confidence >= 0.55 && cands[0].confidence < 0.85);
    }

    #[test]
    fn detects_labeled_blank_with_label() {
        // "Name:" sits at (100,720); rule at y=720 from x=140 to x=400.
        let pdf = test_helpers::pdf_with_content(
            b"BT /F1 12 Tf 100 720 Td (Name:) Tj ET q 0.5 w 140 720 m 400 720 l S Q",
        );
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let cands = detect_page(&doc, 1).unwrap();
        assert_eq!(cands.len(), 1, "candidates: {cands:?}");
        let c = &cands[0];
        assert!(matches!(c.kind, CandidateKind::Text { .. }));
        assert_eq!(c.label.as_deref(), Some("Name"));
        assert_eq!(c.suggested_name, "name");
        assert!(c.confidence >= 0.85, "confidence was {}", c.confidence);
    }

    #[test]
    fn detects_checkbox_glyph() {
        let pdf = test_helpers::pdf_with_content(
            b"q 0.5 w 100 650 12 12 re s Q BT /F1 12 Tf 120 654 Td (I agree) Tj ET",
        );
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let cands = detect_page(&doc, 1).unwrap();
        let checks: Vec<_> = cands
            .iter()
            .filter(|c| matches!(c.kind, CandidateKind::Checkbox))
            .collect();
        assert_eq!(checks.len(), 1, "candidates: {cands:?}");
        assert_eq!(checks[0].label.as_deref(), Some("I agree"));
    }

    #[test]
    fn detects_signature_line() {
        let pdf = test_helpers::pdf_with_content(
            b"BT /F1 12 Tf 100 500 Td (Signature:) Tj ET q 0.5 w 160 500 m 500 500 l S Q",
        );
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let cands = detect_page(&doc, 1).unwrap();
        assert_eq!(cands.len(), 1, "candidates: {cands:?}");
        assert!(matches!(cands[0].kind, CandidateKind::Signature));
        assert!(matches!(cands[0].evidence, Evidence::SignatureLine));
        assert!(cands[0].confidence >= 0.85);
    }

    /// Integration: a hand-built page mimicking a real flat form.
    /// 4 labeled blanks + 2 checkboxes + 1 signature line = 7 intended fields.
    #[test]
    fn integration_realistic_form() {
        let stream = realistic_form_stream();
        let pdf = test_helpers::pdf_with_content(&stream);
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let report = detect_doc(&doc).unwrap();
        assert!(!report.already_has_acroform);
        assert_eq!(report.pages_scanned, 1);
        assert!(
            report.candidates.len() >= 6,
            "expected >= 6 candidates, got {} ({:?})",
            report.candidates.len(),
            report.candidates
        );
        assert!(report
            .candidates
            .iter()
            .any(|c| matches!(c.kind, CandidateKind::Signature)));
        let n_checkboxes = report
            .candidates
            .iter()
            .filter(|c| matches!(c.kind, CandidateKind::Checkbox))
            .count();
        assert_eq!(n_checkboxes, 2, "expected 2 checkboxes, got {n_checkboxes}");
        let high_conf = report
            .candidates
            .iter()
            .filter(|c| c.confidence >= 0.7)
            .count();
        assert!(high_conf >= 5, "high-conf={high_conf}");
        // Unique slug names after dedupe
        let mut names: Vec<_> = report
            .candidates
            .iter()
            .map(|c| c.suggested_name.clone())
            .collect();
        names.sort();
        let n_before = names.len();
        names.dedup();
        assert_eq!(names.len(), n_before, "duplicate names remain");
    }

    fn realistic_form_stream() -> Vec<u8> {
        let mut s = String::new();
        // Labeled blanks
        for (label, lx, rx0, rx1, y) in [
            ("Name:", 60, 110, 400, 720),
            ("Email:", 60, 110, 400, 690),
            ("Address:", 60, 130, 500, 660),
            ("Phone:", 60, 110, 300, 630),
        ] {
            s.push_str(&format!(
                "BT /F1 12 Tf {lx} {y} Td ({label}) Tj ET q 0.5 w {rx0} {y} m {rx1} {y} l S Q "
            ));
        }
        // Two checkboxes with labels
        s.push_str("q 0.5 w 60 580 12 12 re s Q BT /F1 12 Tf 80 584 Td (I agree to terms) Tj ET ");
        s.push_str(
            "q 0.5 w 60 555 12 12 re s Q BT /F1 12 Tf 80 559 Td (Subscribe to newsletter) Tj ET ",
        );
        // Signature line
        s.push_str("BT /F1 12 Tf 60 500 Td (Signature:) Tj ET q 0.5 w 140 500 m 500 500 l S Q ");
        s.into_bytes()
    }

    #[test]
    fn detect_returns_input_missing_for_nonexistent_path() {
        let err = detect(Path::new("/nonexistent/no.pdf")).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
