// Legal stamps — diagonal watermark-style overlays for legal/compliance
// document workflows. The four canonical presets that every law firm
// uses on confidential / privileged / draft documents, plus an arbitrary
// custom-text option.
//
// Implementation: same Q/q content-stream pattern as `watermark.rs`, but
// with RGB color (rg operator) instead of grayscale, a `StampPreset`
// enum that maps to colors + default text, and an optional page range.
//
// Buyer story: a paralegal opens 200 PDFs, drags them onto Slab, picks
// "CONFIDENTIAL — Attorney Eyes Only" from a dropdown, hits Apply. Done.
// Adobe Acrobat makes you script this in JavaScript or buy "Action
// Wizard" (paid add-on).

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StampPreset {
    Confidential,
    AttorneyEyesOnly,
    Privileged,
    Draft,
    /// Arbitrary caller-supplied text. The label string becomes the
    /// stamp text exactly as given (uppercased by convention — the
    /// caller decides).
    Custom {
        text: String,
    },
}

impl StampPreset {
    /// Default text for the preset. For `Custom`, returns the caller text.
    pub fn text(&self) -> &str {
        match self {
            StampPreset::Confidential => "CONFIDENTIAL",
            StampPreset::AttorneyEyesOnly => "ATTORNEY EYES ONLY",
            StampPreset::Privileged => "PRIVILEGED & CONFIDENTIAL",
            StampPreset::Draft => "DRAFT",
            StampPreset::Custom { text } => text.as_str(),
        }
    }

    /// Default RGB color tuned to legal conventions:
    /// CONFIDENTIAL/AEO are red, PRIVILEGED is dark blue, DRAFT is gray.
    pub fn default_color(&self) -> (f32, f32, f32) {
        match self {
            StampPreset::Confidential => (0.78, 0.10, 0.10),
            StampPreset::AttorneyEyesOnly => (0.55, 0.05, 0.05),
            StampPreset::Privileged => (0.10, 0.18, 0.55),
            StampPreset::Draft => (0.45, 0.45, 0.45),
            StampPreset::Custom { .. } => (0.55, 0.05, 0.05),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegalStampOpts {
    pub preset: StampPreset,
    /// 0.0..=1.0; default 0.35 — visible but not obscuring.
    pub opacity: f32,
    /// Helvetica point size; default 64.
    pub font_size: f32,
    /// Rotation in degrees, CCW; default 45.
    pub rotation_deg: f32,
    /// RGB triple in 0.0..=1.0. If `None`, the preset's default color is used.
    pub color: Option<(f32, f32, f32)>,
    /// Pages to stamp. Empty = all pages. 1-indexed.
    pub pages: Vec<u32>,
}

impl Default for LegalStampOpts {
    fn default() -> Self {
        LegalStampOpts {
            preset: StampPreset::Confidential,
            opacity: 0.35,
            font_size: 64.0,
            rotation_deg: 45.0,
            color: None,
            pages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LegalStampReport {
    pub pages_stamped: u32,
    pub text: String,
}

pub fn apply_legal_stamp(
    input: &Path,
    output: &Path,
    opts: &LegalStampOpts,
) -> Result<LegalStampReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let text = opts.preset.text().to_string();
    if text.is_empty() {
        return Err(PdfError::Other("stamp text is empty".into()));
    }
    if !(0.0..=1.0).contains(&opts.opacity) {
        return Err(PdfError::Other("opacity must be in 0.0..=1.0".into()));
    }
    if opts.font_size <= 0.0 {
        return Err(PdfError::Other("font_size must be positive".into()));
    }
    let (r_col, g_col, b_col) = opts.color.unwrap_or_else(|| opts.preset.default_color());
    for &c in &[r_col, g_col, b_col] {
        if !(0.0..=1.0).contains(&c) {
            return Err(PdfError::Other(
                "color channels must be in 0.0..=1.0".into(),
            ));
        }
    }

    let mut doc = Document::load(input)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    let targets: BTreeSet<u32> = if opts.pages.is_empty() {
        (1..=total).collect()
    } else {
        for &p in &opts.pages {
            if p == 0 || p > total {
                return Err(PdfError::Other(format!(
                    "page {} out of range (1..={})",
                    p, total
                )));
            }
        }
        opts.pages.iter().copied().collect()
    };

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
    });

    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => opts.opacity as f64,
        "CA" => opts.opacity as f64,
    });

    let mut applied = 0u32;
    let mut page_updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();

    for (page_num, page_id) in &page_map {
        if !targets.contains(page_num) {
            continue;
        }
        let (w, h) = page_size(&doc, *page_id).unwrap_or((612.0, 792.0));
        let cx = w / 2.0;
        let cy = h / 2.0;
        let theta = opts.rotation_deg.to_radians();
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new("gs", vec!["SlabStampGS".into()]),
            Operation::new(
                "rg",
                vec![
                    Object::Real(r_col),
                    Object::Real(g_col),
                    Object::Real(b_col),
                ],
            ),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["SlabStampF".into(), opts.font_size.into()]),
            Operation::new(
                "Tm",
                vec![
                    Object::Real(cos_t),
                    Object::Real(sin_t),
                    Object::Real(-sin_t),
                    Object::Real(cos_t),
                    Object::Real(cx),
                    Object::Real(cy),
                ],
            ),
            Operation::new(
                "Td",
                vec![
                    Object::Real(-(text.len() as f32) * opts.font_size * 0.27),
                    Object::Real(-opts.font_size * 0.3),
                ],
            ),
            Operation::new("Tj", vec![Object::string_literal(text.as_bytes())]),
            Operation::new("ET", vec![]),
            Operation::new("Q", vec![]),
        ];
        let content = Content { operations: ops };
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        page_updates.push((*page_id, stream_id));
        applied += 1;
    }

    for (page_id, stream_id) in page_updates {
        append_stamp(&mut doc, page_id, stream_id, font_id, gs_id)?;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;

    Ok(LegalStampReport {
        pages_stamped: applied,
        text,
    })
}

fn page_size(doc: &Document, page_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let page = doc.get_object(page_id).ok()?;
    let dict = page.as_dict().ok()?;
    let mb = dict.get(b"MediaBox").ok()?;
    let arr = mb.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let nums: Vec<f32> = arr
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

fn append_stamp(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    new_stream_id: lopdf::ObjectId,
    font_id: lopdf::ObjectId,
    gs_id: lopdf::ObjectId,
) -> Result<(), PdfError> {
    let page = doc.get_object_mut(page_id)?;
    if let Object::Dictionary(dict) = page {
        let new_contents = match dict.get(b"Contents") {
            Ok(Object::Reference(r)) => {
                vec![Object::Reference(*r), Object::Reference(new_stream_id)]
            }
            Ok(Object::Array(arr)) => {
                let mut v = arr.clone();
                v.push(Object::Reference(new_stream_id));
                v
            }
            _ => vec![Object::Reference(new_stream_id)],
        };
        dict.set("Contents", new_contents);

        let resources_value = dict.get(b"Resources").ok().cloned();
        let mut resources = match resources_value {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(r)) => doc.get_object(r)?.as_dict()?.clone(),
            _ => lopdf::Dictionary::new(),
        };

        let mut font = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        font.set("SlabStampF", Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(font));

        let mut gs = match resources.get(b"ExtGState") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        gs.set("SlabStampGS", Object::Reference(gs_id));
        resources.set("ExtGState", Object::Dictionary(gs));

        let page = doc.get_object_mut(page_id)?;
        if let Object::Dictionary(dict) = page {
            dict.set("Resources", Object::Dictionary(resources));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn preset_text_matches_legal_convention() {
        assert_eq!(StampPreset::Confidential.text(), "CONFIDENTIAL");
        assert_eq!(StampPreset::AttorneyEyesOnly.text(), "ATTORNEY EYES ONLY");
        assert_eq!(StampPreset::Privileged.text(), "PRIVILEGED & CONFIDENTIAL");
        assert_eq!(StampPreset::Draft.text(), "DRAFT");
        assert_eq!(
            StampPreset::Custom {
                text: "INTERNAL ONLY".into(),
            }
            .text(),
            "INTERNAL ONLY"
        );
    }

    #[test]
    fn preset_colors_are_in_range() {
        for p in [
            StampPreset::Confidential,
            StampPreset::AttorneyEyesOnly,
            StampPreset::Privileged,
            StampPreset::Draft,
        ] {
            let (r, g, b) = p.default_color();
            assert!((0.0..=1.0).contains(&r));
            assert!((0.0..=1.0).contains(&g));
            assert!((0.0..=1.0).contains(&b));
        }
    }

    #[test]
    fn applies_to_all_pages_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let r = apply_legal_stamp(
            &src,
            &dst,
            &LegalStampOpts {
                preset: StampPreset::Confidential,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.pages_stamped, 3);
        assert_eq!(r.text, "CONFIDENTIAL");
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn applies_to_page_range_only() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 5);

        let r = apply_legal_stamp(
            &src,
            &dst,
            &LegalStampOpts {
                preset: StampPreset::Draft,
                pages: vec![2, 3],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.pages_stamped, 2);
        assert_eq!(r.text, "DRAFT");
        // Output PDF still has all 5 pages.
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 5);
    }

    #[test]
    fn custom_text_passes_through() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        let r = apply_legal_stamp(
            &src,
            &dst,
            &LegalStampOpts {
                preset: StampPreset::Custom {
                    text: "INTERNAL ONLY".into(),
                },
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.pages_stamped, 1);
        assert_eq!(r.text, "INTERNAL ONLY");
    }

    #[test]
    fn rejects_out_of_range_page() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);

        let err = apply_legal_stamp(
            &src,
            &dst,
            &LegalStampOpts {
                preset: StampPreset::Confidential,
                pages: vec![99],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn rejects_bad_opacity() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        let err = apply_legal_stamp(
            &src,
            &dst,
            &LegalStampOpts {
                preset: StampPreset::Confidential,
                opacity: 1.5,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("opacity"));
    }

    #[test]
    fn rejects_bad_color_channel() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        let err = apply_legal_stamp(
            &src,
            &dst,
            &LegalStampOpts {
                preset: StampPreset::Confidential,
                color: Some((1.2, 0.0, 0.0)),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("color"));
    }

    #[test]
    fn missing_input_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");
        let err = apply_legal_stamp(&bogus, &dst, &LegalStampOpts::default()).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
