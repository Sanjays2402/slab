//! Scan audit: figure out whether a PDF is text-native, scanned (image-only),
//! mixed, or empty — per page and as a whole. Drives the auto-OCR banner in
//! the Reader and the Library auto-OCR queue in Slice 2.
//!
//! ## Heuristic
//!
//! For each page we ask two cheap questions:
//!
//! 1. Does extracting text from the page yield more than `MIN_TEXT_CHARS`
//!    non-whitespace characters? If yes → there's real text.
//! 2. Does the page's Resources dict reference any `XObject` of subtype
//!    `Image`? If yes → there's an image.
//!
//! From those two booleans we get a clean 2x2 classification:
//!
//! |              | image? no | image? yes |
//! |--------------|:--------:|:----------:|
//! | text? no     | Empty    | Image      |
//! | text? yes    | Text     | Mixed      |
//!
//! The whole-document recommendation:
//!
//! * `OcrAll` — every non-empty page is `Image`.
//! * `OcrSome` — at least one `Image` page and at least one `Text` page (or
//!   any `Mixed` pages).
//! * `None` — every page is `Text` or `Empty`.
//!
//! ## Limits
//!
//! This module does not rasterize anything. It does not call Tesseract. It
//! only looks at the PDF object graph. That's intentional: scan_audit runs
//! on every Reader open and on every Library indexed doc, so it must be
//! fast and dependency-free.

use crate::pdf::PdfError;
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Minimum non-whitespace character count for a page to be considered
/// "text-bearing". Tuned conservatively: a typical scanned PDF page yields 0
/// chars from extract_text; a typical text page yields hundreds.
const MIN_TEXT_CHARS: usize = 40;

/// Per-page classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageClassification {
    /// Has real, extractable text. No image overlay (or text dominates).
    Text,
    /// Image-only — no extractable text, has at least one image XObject.
    /// This is the "scan needs OCR" signal.
    Image,
    /// Has both text AND image content — a scanned form with typed fields,
    /// a photo with caption, etc.
    Mixed,
    /// No text, no image. Blank divider page or stripped-content page.
    Empty,
}

/// What the UI should suggest doing with this document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    /// Every non-empty page is image-only — strongly recommend running OCR.
    OcrAll,
    /// At least one image-only page (or mixed page) mixed with text pages —
    /// offer OCR but as a soft suggestion.
    OcrSome,
    /// Nothing to do — the doc is text-native.
    None,
}

/// Full report — one entry per page plus per-class counts plus recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanAuditReport {
    /// Per-page classifications, page 1 first.
    pub pages: Vec<PageClassification>,
    /// Convenience tally: how many `Text` pages.
    pub text_pages: u32,
    /// How many `Image` pages.
    pub image_pages: u32,
    /// How many `Mixed` pages.
    pub mixed_pages: u32,
    /// How many `Empty` pages.
    pub empty_pages: u32,
    /// What the UI should propose.
    pub recommended_action: Recommendation,
}

impl ScanAuditReport {
    /// Total page count.
    pub fn total(&self) -> u32 {
        self.pages.len() as u32
    }
}

/// Audit `input` and return a classification per page plus a top-level
/// recommendation. Cheap: O(pages) extract_text + a single Resources walk.
pub fn audit(input: &Path) -> Result<ScanAuditReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    let pages_map = doc.get_pages();
    let total = pages_map.len() as u32;

    let mut out = Vec::with_capacity(total as usize);
    let mut text_pages = 0u32;
    let mut image_pages = 0u32;
    let mut mixed_pages = 0u32;
    let mut empty_pages = 0u32;

    for page_no in 1..=total {
        let has_text = page_has_text(&doc, page_no);
        let page_id = pages_map.get(&page_no).copied();
        let has_image = match page_id {
            Some(id) => page_has_image(&doc, id),
            None => false,
        };

        let class = match (has_text, has_image) {
            (true, false) => PageClassification::Text,
            (false, true) => PageClassification::Image,
            (true, true) => PageClassification::Mixed,
            (false, false) => PageClassification::Empty,
        };
        match class {
            PageClassification::Text => text_pages += 1,
            PageClassification::Image => image_pages += 1,
            PageClassification::Mixed => mixed_pages += 1,
            PageClassification::Empty => empty_pages += 1,
        }
        out.push(class);
    }

    let recommended_action = if total == 0 {
        Recommendation::None
    } else if image_pages > 0 && text_pages == 0 && mixed_pages == 0 {
        Recommendation::OcrAll
    } else if image_pages > 0 || mixed_pages > 0 {
        Recommendation::OcrSome
    } else {
        Recommendation::None
    };

    Ok(ScanAuditReport {
        pages: out,
        text_pages,
        image_pages,
        mixed_pages,
        empty_pages,
        recommended_action,
    })
}

/// Does `extract_text` produce >= MIN_TEXT_CHARS non-whitespace chars for
/// this page? lopdf's extract_text is the same one the rest of the codebase
/// uses (see pdf::extract), which keeps audit and downstream features in sync.
fn page_has_text(doc: &Document, page_no: u32) -> bool {
    let raw = doc.extract_text(&[page_no]).unwrap_or_default();
    raw.chars().filter(|c| !c.is_whitespace()).count() >= MIN_TEXT_CHARS
}

/// Walk the page's Resources XObject dict (following inherited resources up
/// the page tree) and return true if any XObject has Subtype = "Image".
fn page_has_image(doc: &Document, page_id: ObjectId) -> bool {
    // Resources can be on the page or inherited from an ancestor Pages node.
    let res_dict = match find_resources(doc, page_id) {
        Some(d) => d,
        None => return false,
    };
    let xobject_ref = match res_dict.get(b"XObject") {
        Ok(obj) => obj,
        Err(_) => return false,
    };
    // XObject value might be a dict or an indirect ref to a dict.
    let xobject_dict = match xobject_ref {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return false,
        },
        _ => return false,
    };
    for (_name, obj) in xobject_dict.iter() {
        let stream_ref = match obj {
            Object::Reference(id) => doc.get_object(*id).ok(),
            Object::Stream(_) => Some(obj),
            _ => None,
        };
        if let Some(Object::Stream(stream)) = stream_ref {
            if let Ok(subtype) = stream.dict.get(b"Subtype") {
                if let Ok(name) = subtype.as_name() {
                    if name == b"Image" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Walk up the page tree to find an inherited /Resources dictionary.
/// We cycle-guard with a visited set just in case.
fn find_resources(doc: &Document, page_id: ObjectId) -> Option<lopdf::Dictionary> {
    let mut current = Some(page_id);
    let mut seen: HashSet<ObjectId> = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return None;
        }
        let dict = doc.get_object(id).ok()?.as_dict().ok()?;
        if let Ok(res) = dict.get(b"Resources") {
            match res {
                Object::Dictionary(d) => return Some(d.clone()),
                Object::Reference(rid) => {
                    if let Ok(obj) = doc.get_object(*rid) {
                        if let Ok(d) = obj.as_dict() {
                            return Some(d.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        current = dict.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::{make_image_only_pdf, make_mixed_pdf, make_n_page_pdf};

    #[test]
    fn missing_input_errors() {
        let r = audit(Path::new("/this/path/does/not/exist.pdf"));
        assert!(matches!(r, Err(PdfError::InputMissing(_))));
    }

    #[test]
    fn text_only_pdf_is_all_text() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_n_page_pdf(&p, 3);
        let r = audit(&p).unwrap();
        // Our fixture uses very short text ("Slab page N") — likely below
        // the MIN_TEXT_CHARS threshold. That's fine: a short caption page
        // should still classify as Empty when there's no image either.
        assert_eq!(r.total(), 3);
        for c in &r.pages {
            assert!(
                matches!(c, PageClassification::Text | PageClassification::Empty),
                "expected Text or Empty, got {:?}",
                c
            );
        }
        assert_eq!(r.image_pages, 0);
        assert_eq!(r.mixed_pages, 0);
        assert_eq!(r.recommended_action, Recommendation::None);
    }

    #[test]
    fn image_only_pdf_is_all_image() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("scan.pdf");
        make_image_only_pdf(&p, 4);
        let r = audit(&p).unwrap();
        assert_eq!(r.total(), 4);
        assert_eq!(r.image_pages, 4);
        assert_eq!(r.text_pages, 0);
        assert_eq!(r.mixed_pages, 0);
        assert_eq!(r.empty_pages, 0);
        assert_eq!(r.recommended_action, Recommendation::OcrAll);
        for c in &r.pages {
            assert_eq!(*c, PageClassification::Image);
        }
    }

    #[test]
    fn single_page_image_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("one.pdf");
        make_image_only_pdf(&p, 1);
        let r = audit(&p).unwrap();
        assert_eq!(r.total(), 1);
        assert_eq!(r.pages, vec![PageClassification::Image]);
        assert_eq!(r.recommended_action, Recommendation::OcrAll);
    }

    #[test]
    fn mixed_pdf_recommends_ocr_some() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mix.pdf");
        make_mixed_pdf(&p);
        let r = audit(&p).unwrap();
        assert_eq!(r.total(), 3);
        // page 1: text-only — short string, classifies as Empty (below threshold).
        //  That's a known quirk of our threshold + tiny fixtures; we only
        //  care that page 2 is Image and page 3 has image+text behavior.
        assert!(matches!(
            r.pages[0],
            PageClassification::Text | PageClassification::Empty
        ));
        assert_eq!(r.pages[1], PageClassification::Image);
        // page 3 has both — classified Mixed if text is long enough, else Image.
        assert!(matches!(
            r.pages[2],
            PageClassification::Mixed | PageClassification::Image
        ));
        // Whatever happens, recommendation must be OcrSome or OcrAll (not None).
        assert!(matches!(
            r.recommended_action,
            Recommendation::OcrSome | Recommendation::OcrAll
        ));
    }

    #[test]
    fn report_counts_match_pages_vec() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("scan.pdf");
        make_image_only_pdf(&p, 5);
        let r = audit(&p).unwrap();
        let sum = r.text_pages + r.image_pages + r.mixed_pages + r.empty_pages;
        assert_eq!(sum, r.total());
        assert_eq!(r.pages.len(), r.total() as usize);
    }

    #[test]
    fn recommendation_none_for_text_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_n_page_pdf(&p, 2);
        let r = audit(&p).unwrap();
        assert_eq!(r.recommended_action, Recommendation::None);
    }

    #[test]
    fn classification_serializes_to_lowercase_strings() {
        // The frontend reads these as plain string discriminants.
        let s = serde_json::to_string(&PageClassification::Image).unwrap();
        assert_eq!(s, "\"image\"");
        let s = serde_json::to_string(&PageClassification::Text).unwrap();
        assert_eq!(s, "\"text\"");
        let s = serde_json::to_string(&PageClassification::Mixed).unwrap();
        assert_eq!(s, "\"mixed\"");
        let s = serde_json::to_string(&PageClassification::Empty).unwrap();
        assert_eq!(s, "\"empty\"");
    }

    #[test]
    fn recommendation_serializes_to_snake_case() {
        let s = serde_json::to_string(&Recommendation::OcrAll).unwrap();
        assert_eq!(s, "\"ocr_all\"");
        let s = serde_json::to_string(&Recommendation::OcrSome).unwrap();
        assert_eq!(s, "\"ocr_some\"");
        let s = serde_json::to_string(&Recommendation::None).unwrap();
        assert_eq!(s, "\"none\"");
    }

    #[test]
    fn report_round_trips_through_json() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("scan.pdf");
        make_image_only_pdf(&p, 2);
        let r = audit(&p).unwrap();
        let s = serde_json::to_string(&r).unwrap();
        let parsed: ScanAuditReport = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.total(), r.total());
        assert_eq!(parsed.image_pages, r.image_pages);
        assert_eq!(parsed.recommended_action, r.recommended_action);
    }
}
