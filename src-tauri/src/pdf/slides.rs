// Slides detection and presenter-mode helpers.
//
// `analyze()` examines a PDF and returns a `SlideReport` that the UI can use to:
//   - decide whether to default to Slides view (auto-detect),
//   - render the per-page thumbnail grid with aspect-correct boxes,
//   - show whether the deck has speaker notes (any /Annots Text annot on a page),
//   - surface the dominant aspect ratio / orientation / producer string.
//
// Heuristics for "this PDF is a slide deck":
//
//   1. At least 80% of pages must be landscape (width > height * 1.1) OR
//      square-ish (within 10%). Portrait-heavy PDFs are documents, not decks.
//
//   2. The dominant page size (rounded to 1 pt) must cover ≥ 90% of pages
//      — slide decks have consistent page geometry; mixed-size PDFs are docs.
//
//   3. Page count is between 1 and 500 (decks longer than 500 slides are
//      almost certainly a print export of a long doc).
//
//   4. Producer/Creator metadata bonus: if the Info dict mentions
//      "PowerPoint", "Keynote", "Google Slides", "Beamer", or "Marp",
//      we set `producer_hint = true` and bump confidence.
//
// Confidence is a 0..100 score combining the rules. The UI auto-enables
// Slides view at ≥ 65, otherwise falls back to Reader and shows a
// "Looks like slides — switch to Slides view?" banner.
//
// Speaker-notes extraction: each PDF page can carry `/Annots` with
// `/Subtype /Text` annotations (the standard "sticky note" annot). Many
// PowerPoint/Keynote exports use these for speaker notes. We pull
// `/Contents` from each text-subtype annotation and concatenate per page.

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Standard PowerPoint / Keynote slide sizes in PDF points (1pt = 1/72").
/// Matched fuzzily within ±2pt to allow rounding.
const KNOWN_SIZES: &[(f32, f32, &str)] = &[
    (720.0, 540.0, "PowerPoint 4:3 (10×7.5 in)"),
    (960.0, 540.0, "PowerPoint 16:9 (13.3×7.5 in)"),
    (1024.0, 768.0, "Keynote 4:3 (14.2×10.7 in)"),
    (1024.0, 576.0, "Keynote 16:9 (14.2×8 in)"),
    (1280.0, 720.0, "HD 16:9 (17.8×10 in)"),
    (1920.0, 1080.0, "Full-HD 16:9"),
];

/// Per-page geometry as the UI needs it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SlidePage {
    /// 1-indexed page number.
    pub page: u32,
    /// MediaBox width in PDF points.
    pub width_pt: f32,
    /// MediaBox height in PDF points.
    pub height_pt: f32,
    /// width / height (clamped to 4 decimals).
    pub aspect: f32,
    /// "landscape", "portrait", or "square".
    pub orientation: String,
    /// Concatenated `/Contents` of any `/Annots` with `/Subtype /Text` on
    /// the page. Empty string when there are no speaker-note annotations.
    pub notes: String,
}

/// Document-level slide report.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SlideReport {
    pub page_count: u32,
    pub pages: Vec<SlidePage>,
    /// Dominant page size as `"WIDTHxHEIGHT"` (e.g. `"960x540"`).
    pub dominant_size: String,
    /// Friendly label like "PowerPoint 16:9" if dominant_size matches a known
    /// preset, otherwise just the dominant size in inches at 72 dpi.
    pub dominant_label: String,
    /// Fraction (0..1) of pages whose size matches dominant_size within ±2pt.
    pub consistency: f32,
    /// Fraction (0..1) of pages that are landscape.
    pub landscape_fraction: f32,
    /// Total pages with at least one speaker-note annotation.
    pub pages_with_notes: u32,
    /// `Producer` string from Info dict, lowercased and trimmed.
    pub producer: Option<String>,
    /// True when producer/creator metadata mentions a known slide tool.
    pub producer_hint: bool,
    /// Heuristic 0..100. ≥ 65 → auto-enable Slides view.
    pub confidence: u8,
    /// Whether the heuristic recommends Slides view.
    pub is_slides: bool,
}

/// Public entry: analyze a PDF on disk.
pub fn analyze(input: &Path) -> Result<SlideReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    analyze_doc(&doc)
}

/// Same as `analyze` but takes an already-loaded `Document`. Exposed so
/// callers that already have one (e.g. the Reader) don't pay to re-parse.
pub fn analyze_doc(doc: &Document) -> Result<SlideReport, PdfError> {
    let producer = read_producer(doc);
    let producer_hint = producer
        .as_deref()
        .map(producer_matches_slides_tool)
        .unwrap_or(false);

    let mut pages: Vec<SlidePage> = Vec::new();
    for (idx, (_, page_id)) in doc.get_pages().into_iter().enumerate() {
        let (w, h) = page_dimensions(doc, page_id).unwrap_or((612.0, 792.0));
        let aspect = if h > 0.0 {
            (w / h * 10_000.0).round() / 10_000.0
        } else {
            0.0
        };
        let orientation = orientation_for(w, h);
        let notes = read_text_annot_notes(doc, page_id);
        pages.push(SlidePage {
            page: (idx as u32) + 1,
            width_pt: round1(w),
            height_pt: round1(h),
            aspect,
            orientation,
            notes,
        });
    }

    let page_count = pages.len() as u32;
    let landscape_fraction = if pages.is_empty() {
        0.0
    } else {
        pages
            .iter()
            .filter(|p| p.orientation == "landscape")
            .count() as f32
            / pages.len() as f32
    };

    // Bucket pages by rounded size (1pt) to find dominant geometry.
    let mut buckets: HashMap<(i32, i32), u32> = HashMap::new();
    for p in &pages {
        let key = (p.width_pt.round() as i32, p.height_pt.round() as i32);
        *buckets.entry(key).or_insert(0) += 1;
    }
    let ((dom_w, dom_h), dom_count) = buckets
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, c)| (*k, *c))
        .unwrap_or(((0, 0), 0));
    let consistency = if page_count == 0 {
        0.0
    } else {
        dom_count as f32 / page_count as f32
    };
    let dominant_size = format!("{}x{}", dom_w, dom_h);
    let dominant_label = friendly_size_label(dom_w as f32, dom_h as f32);

    let pages_with_notes = pages.iter().filter(|p| !p.notes.is_empty()).count() as u32;

    let confidence = score_confidence(
        page_count,
        landscape_fraction,
        consistency,
        producer_hint,
        dom_w as f32,
        dom_h as f32,
    );
    let is_slides = confidence >= 65;

    Ok(SlideReport {
        page_count,
        pages,
        dominant_size,
        dominant_label,
        consistency: round4(consistency),
        landscape_fraction: round4(landscape_fraction),
        pages_with_notes,
        producer,
        producer_hint,
        confidence,
        is_slides,
    })
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

fn round4(v: f32) -> f32 {
    (v * 10_000.0).round() / 10_000.0
}

fn orientation_for(w: f32, h: f32) -> String {
    if h == 0.0 {
        return "landscape".to_string();
    }
    let ratio = w / h;
    if (ratio - 1.0).abs() < 0.10 {
        "square".to_string()
    } else if ratio > 1.0 {
        "landscape".to_string()
    } else {
        "portrait".to_string()
    }
}

fn read_producer(doc: &Document) -> Option<String> {
    let info_ref = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
    let info_obj = doc.get_object(info_ref).ok()?;
    let dict = info_obj.as_dict().ok()?;
    let mut hits: Vec<String> = Vec::new();
    for k in [b"Producer".as_ref(), b"Creator".as_ref()] {
        if let Ok(Object::String(bytes, _)) = dict.get(k) {
            let s = String::from_utf8_lossy(bytes).into_owned();
            let s = s.trim().to_string();
            if !s.is_empty() {
                hits.push(s);
            }
        }
    }
    if hits.is_empty() {
        None
    } else {
        Some(hits.join(" / ").to_lowercase())
    }
}

fn producer_matches_slides_tool(prod: &str) -> bool {
    let needles = [
        "powerpoint",
        "keynote",
        "google slides",
        "google docs",
        "beamer",
        "marp",
        "slidy",
        "reveal.js",
        "deckset",
    ];
    let lower = prod.to_lowercase();
    needles.iter().any(|n| lower.contains(n))
}

fn page_dimensions(doc: &Document, page_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let dict = doc.get_object(page_id).ok()?.as_dict().ok()?;
    // Walk up the /Parent chain looking for MediaBox (lopdf does inherit).
    if let Some(dims) = read_media_box(dict) {
        return Some(dims);
    }
    if let Ok(parent_ref) = dict.get(b"Parent") {
        if let Ok(parent_id) = parent_ref.as_reference() {
            if let Ok(parent) = doc.get_object(parent_id) {
                if let Ok(parent_dict) = parent.as_dict() {
                    if let Some(dims) = read_media_box(parent_dict) {
                        return Some(dims);
                    }
                }
            }
        }
    }
    None
}

fn read_media_box(dict: &lopdf::Dictionary) -> Option<(f32, f32)> {
    let mb = dict.get(b"MediaBox").ok()?.as_array().ok()?;
    if mb.len() < 4 {
        return None;
    }
    let nums: Vec<f32> = mb
        .iter()
        .filter_map(|o| match o {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        })
        .collect();
    if nums.len() < 4 {
        return None;
    }
    Some(((nums[2] - nums[0]).abs(), (nums[3] - nums[1]).abs()))
}

fn read_text_annot_notes(doc: &Document, page_id: lopdf::ObjectId) -> String {
    let dict = match doc.get_object(page_id).and_then(|o| o.as_dict()) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let annots = match dict.get(b"Annots") {
        Ok(a) => a,
        Err(_) => return String::new(),
    };
    let arr = match annots {
        Object::Array(arr) => arr.clone(),
        Object::Reference(r) => match doc
            .get_object(*r)
            .ok()
            .and_then(|o| o.as_array().ok().cloned())
        {
            Some(a) => a,
            None => return String::new(),
        },
        _ => return String::new(),
    };

    let mut notes: Vec<String> = Vec::new();
    for item in arr {
        let annot_dict = match item {
            Object::Reference(r) => doc
                .get_object(r)
                .ok()
                .and_then(|o| o.as_dict().ok().cloned()),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        let Some(d) = annot_dict else { continue };
        // Only /Subtype /Text — that's the spec name for sticky notes /
        // speaker-note annotations. Skip /Link, /Highlight, /Square, etc.
        let is_text_subtype = d
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| n == b"Text")
            .unwrap_or(false);
        if !is_text_subtype {
            continue;
        }
        if let Ok(Object::String(bytes, _)) = d.get(b"Contents") {
            let s = String::from_utf8_lossy(bytes).trim().to_string();
            if !s.is_empty() {
                notes.push(s);
            }
        }
    }
    notes.join("\n\n")
}

fn friendly_size_label(w: f32, h: f32) -> String {
    for (kw, kh, label) in KNOWN_SIZES {
        if (w - kw).abs() <= 2.0 && (h - kh).abs() <= 2.0 {
            return (*label).to_string();
        }
    }
    // Fallback: convert pt → inches and round to 1 dp.
    let win = w / 72.0;
    let hin = h / 72.0;
    format!("{:.1}×{:.1} in ({:.0}×{:.0} pt)", win, hin, w, h)
}

fn score_confidence(
    page_count: u32,
    landscape_fraction: f32,
    consistency: f32,
    producer_hint: bool,
    dom_w: f32,
    dom_h: f32,
) -> u8 {
    if page_count == 0 {
        return 0;
    }
    let mut score: f32 = 0.0;

    // Rule 1: landscape ratio — strongest signal. 0..40.
    if landscape_fraction >= 0.80 {
        score += 40.0;
    } else if landscape_fraction >= 0.60 {
        score += 25.0;
    } else if landscape_fraction >= 0.40 {
        score += 10.0;
    }

    // Rule 2: page-size consistency — 0..25.
    if consistency >= 0.95 {
        score += 25.0;
    } else if consistency >= 0.80 {
        score += 15.0;
    } else if consistency >= 0.50 {
        score += 5.0;
    }

    // Rule 3: page count plausibility — 0..15. Penalize huge decks.
    if (1..=200).contains(&page_count) {
        score += 15.0;
    } else if (201..=500).contains(&page_count) {
        score += 7.0;
    } else if page_count > 500 {
        // Long PDFs are documents, not decks. Subtract.
        score -= 10.0;
    }

    // Rule 4: known slide-deck preset size — 0..15.
    if matches_known_size(dom_w, dom_h) {
        score += 15.0;
    }

    // Rule 5: producer/creator hint — 0..15.
    if producer_hint {
        score += 15.0;
    }

    score.clamp(0.0, 100.0).round() as u8
}

fn matches_known_size(w: f32, h: f32) -> bool {
    KNOWN_SIZES
        .iter()
        .any(|(kw, kh, _)| (w - kw).abs() <= 2.0 && (h - kh).abs() <= 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};
    use std::path::Path;

    /// Build a minimal slide-style PDF at `path`: `n` pages with the
    /// requested (w, h) MediaBox + producer string.
    fn make_slide_pdf(path: &Path, n: u32, w: i32, h: i32, producer: &str) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut kids: Vec<Object> = Vec::new();
        for i in 1..=n {
            let content = lopdf::content::Content {
                operations: vec![
                    lopdf::content::Operation::new("BT", vec![]),
                    lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    lopdf::content::Operation::new("Td", vec![50.into(), 50.into()]),
                    lopdf::content::Operation::new(
                        "Tj",
                        vec![Object::string_literal(format!("Slide {}", i))],
                    ),
                    lopdf::content::Operation::new("ET", vec![]),
                ],
            };
            let cid = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let pid = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => cid,
                "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                "Resources" => resources_id,
            });
            kids.push(pid.into());
        }
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => n as i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        if !producer.is_empty() {
            let info_id = doc.add_object(dictionary! {
                "Producer" => Object::string_literal(producer.to_string()),
            });
            doc.trailer.set("Info", info_id);
        }
        doc.save(path).unwrap();
    }

    /// As `make_slide_pdf` but every page also carries a /Text annotation
    /// with the given speaker note in /Contents.
    fn make_slide_pdf_with_notes(path: &Path, n: u32, w: i32, h: i32, note: &str) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut kids: Vec<Object> = Vec::new();
        for i in 1..=n {
            let content = lopdf::content::Content {
                operations: vec![
                    lopdf::content::Operation::new("BT", vec![]),
                    lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    lopdf::content::Operation::new("Td", vec![50.into(), 50.into()]),
                    lopdf::content::Operation::new(
                        "Tj",
                        vec![Object::string_literal(format!("Slide {}", i))],
                    ),
                    lopdf::content::Operation::new("ET", vec![]),
                ],
            };
            let cid = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let annot_id = doc.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Text",
                "Rect" => vec![10.into(), 10.into(), 20.into(), 20.into()],
                "Contents" => Object::string_literal(format!("{} (page {})", note, i)),
            });
            let pid = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => cid,
                "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                "Resources" => resources_id,
                "Annots" => vec![Object::Reference(annot_id)],
            });
            kids.push(pid.into());
        }
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => n as i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.save(path).unwrap();
    }

    #[test]
    fn classic_16x9_powerpoint_is_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("deck.pdf");
        make_slide_pdf(&p, 12, 960, 540, "Microsoft PowerPoint 16.0");
        let r = analyze(&p).unwrap();
        assert!(r.is_slides, "16x9 deck should classify as slides");
        assert!(r.confidence >= 80, "confidence={}", r.confidence);
        assert_eq!(r.page_count, 12);
        assert_eq!(r.pages_with_notes, 0);
        assert_eq!(r.dominant_size, "960x540");
        assert_eq!(r.dominant_label, "PowerPoint 16:9 (13.3×7.5 in)");
        assert!(r.producer_hint);
        assert_eq!(r.consistency, 1.0);
        assert_eq!(r.landscape_fraction, 1.0);
    }

    #[test]
    fn keynote_4x3_is_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("keynote.pdf");
        make_slide_pdf(&p, 30, 1024, 768, "Apple Keynote 13.0");
        let r = analyze(&p).unwrap();
        assert!(r.is_slides);
        assert!(r.confidence >= 80);
        assert_eq!(r.dominant_label, "Keynote 4:3 (14.2×10.7 in)");
        assert!(r.producer_hint);
    }

    #[test]
    fn portrait_document_is_not_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("doc.pdf");
        // Letter portrait, no slide-producer signal.
        make_slide_pdf(&p, 20, 612, 792, "LaTeX with pdfTeX");
        let r = analyze(&p).unwrap();
        assert!(!r.is_slides, "portrait letter doc must not auto-classify");
        // No landscape, no preset size, no producer hint → 25 from
        // consistency (≥0.95 = 25) + 15 from page count = 40.
        assert!(r.confidence < 65, "confidence={}", r.confidence);
        assert_eq!(r.pages[0].orientation, "portrait");
    }

    #[test]
    fn very_long_pdf_is_penalized() {
        // 600 pages landscape, no producer hint, no known size. Long PDFs
        // are almost certainly not decks.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("long.pdf");
        // Unusual landscape size that doesn't match any preset.
        make_slide_pdf(&p, 600, 800, 400, "");
        let r = analyze(&p).unwrap();
        // Landscape (40) + consistency (25) + page-count penalty (-10) = 55.
        assert!(!r.is_slides, "600-page landscape PDF must not classify");
        assert!(r.confidence < 65, "confidence={}", r.confidence);
    }

    #[test]
    fn speaker_notes_are_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("with-notes.pdf");
        make_slide_pdf_with_notes(&p, 5, 960, 540, "Remember to thank the audience");
        let r = analyze(&p).unwrap();
        assert_eq!(r.pages_with_notes, 5);
        assert!(r.pages[0].notes.contains("thank the audience"));
        assert!(r.pages[0].notes.contains("page 1"));
        assert!(r.pages[4].notes.contains("page 5"));
    }

    #[test]
    fn aspect_and_orientation_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("aspects.pdf");
        make_slide_pdf(&p, 1, 960, 540, "");
        let r = analyze(&p).unwrap();
        assert_eq!(r.pages.len(), 1);
        let page = &r.pages[0];
        assert_eq!(page.width_pt, 960.0);
        assert_eq!(page.height_pt, 540.0);
        assert_eq!(page.orientation, "landscape");
        assert!((page.aspect - 1.7778).abs() < 0.001);
    }

    #[test]
    fn square_pages_classify_as_square() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("square.pdf");
        make_slide_pdf(&p, 4, 600, 600, "");
        let r = analyze(&p).unwrap();
        assert_eq!(r.pages[0].orientation, "square");
    }

    #[test]
    fn known_size_helper_recognizes_presets() {
        assert!(matches_known_size(960.0, 540.0));
        assert!(matches_known_size(1024.0, 768.0));
        assert!(matches_known_size(1280.0, 720.0));
        // 1-pt fuzz tolerance.
        assert!(matches_known_size(961.5, 540.5));
        assert!(!matches_known_size(800.0, 400.0));
    }

    #[test]
    fn producer_match_is_case_insensitive() {
        assert!(producer_matches_slides_tool("microsoft powerpoint 16.0"));
        assert!(producer_matches_slides_tool("Apple Keynote 14.1"));
        assert!(producer_matches_slides_tool("Marp v3.4"));
        assert!(!producer_matches_slides_tool("LibreOffice Writer"));
        assert!(!producer_matches_slides_tool("LaTeX / pdfTeX"));
    }

    #[test]
    fn confidence_score_is_bounded() {
        // Synthetic best case.
        let c = score_confidence(20, 1.0, 1.0, true, 960.0, 540.0);
        assert!(c <= 100);
        // Synthetic worst case.
        let c2 = score_confidence(2000, 0.0, 0.1, false, 100.0, 100.0);
        assert!(c2 < 65);
    }

    #[test]
    fn missing_input_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("nope.pdf");
        let r = analyze(&p);
        assert!(matches!(r, Err(PdfError::InputMissing(_))));
    }

    #[test]
    fn analyze_doc_works_on_loaded_document() {
        // Ensure callers can re-use a Document without re-parsing.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("inline.pdf");
        make_slide_pdf(&p, 3, 960, 540, "PowerPoint");
        let doc = Document::load(&p).unwrap();
        let r = analyze_doc(&doc).unwrap();
        assert!(r.is_slides);
        assert_eq!(r.page_count, 3);
    }

    #[test]
    fn dominant_size_handles_mixed_geometry() {
        // Two pages 960x540, one 612x792 — dominant is 960x540 (2/3 ≈ 0.67).
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mixed.pdf");
        // Easiest way to get mixed geometry is to build manually.
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let mut kids: Vec<Object> = Vec::new();
        for (w, h) in [(960, 540), (960, 540), (612, 792)] {
            let content = lopdf::content::Content {
                operations: vec![
                    lopdf::content::Operation::new("BT", vec![]),
                    lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    lopdf::content::Operation::new("ET", vec![]),
                ],
            };
            let cid = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let pid = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => cid,
                "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                "Resources" => resources_id,
            });
            kids.push(pid.into());
        }
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => 3_i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let cat = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", cat);
        doc.save(&p).unwrap();

        let r = analyze(&p).unwrap();
        assert_eq!(r.dominant_size, "960x540");
        assert!((r.consistency - 2.0 / 3.0).abs() < 0.001);
        // Landscape fraction is 2/3.
        assert!((r.landscape_fraction - 2.0 / 3.0).abs() < 0.001);
        // 25 (landscape ≥0.60) + 5 (consistency ≥0.50) + 15 (1..=200)
        // + 15 (matches known size 960x540) = 60 — under threshold.
        assert!(!r.is_slides, "mixed deck shouldn't auto-classify");
    }

    #[test]
    fn round_helpers_are_stable() {
        assert_eq!(round1(540.45), 540.5);
        assert_eq!(round1(540.04), 540.0);
        assert_eq!(round4(0.66666), 0.6667);
        assert_eq!(round4(0.99999), 1.0);
    }

    #[test]
    fn known_sizes_list_is_non_empty() {
        // Catches accidental deletion of the preset table.
        assert!(KNOWN_SIZES.len() >= 4);
    }
}
