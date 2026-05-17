// Bake presenter annotations (pen + highlighter strokes) into a stamped PDF.
//
// v0.15.0 "Theater" Slice 5 — Export Annotated Deck
//
// The presenter mode captures live ink as `Stroke { tool, color, width,
// points }` where `points` are normalized [0,1] coords with **top-left
// origin** (DOM/canvas convention). This module takes a flat list of those
// strokes plus the original PDF and emits a new PDF with the strokes
// painted as a transparent overlay on each affected page.
//
// Implementation notes (modeled on `pdf::watermark`):
//
//   * We **never** mutate the existing content stream. We add a new
//     content stream per affected page that consists of one `q ... Q`
//     graphics-state block per stroke. Order in the document mirrors the
//     order of the input strokes, so on-screen z-order is preserved.
//   * Highlighter strokes go through a per-page ExtGState dict
//     (`/SlabHL`) with `ca/CA = 0.42`, painted with a round-cap stroked
//     path so they read as a fat translucent swipe — same look as the UI.
//   * Pen strokes are opaque (`ca/CA = 1.0`), round-cap, narrower.
//   * Coordinate flip: PDF user space has origin at bottom-left and units
//     in PDF points; our input is `[0,1]^2` from the top. We convert via
//     `(nx * W, (1 - ny) * H)` where `(W, H)` is the page's MediaBox size.
//   * Line widths are given in PDF points so the overlay looks similar
//     across page sizes (you don't want a 3pt pen on letter and a 3pt pen
//     on A0).
//
// Errors:
//   * `InputMissing` if the source PDF doesn't exist.
//   * `Other` for empty stroke lists, out-of-range `page_index`,
//     out-of-range color components, or paths with < 2 points.
//
// Returns the number of stamped strokes (NOT pages) on success.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Tool variant for a stroke. Highlighter uses a 42%-alpha ExtGState; pen
/// is opaque. Adding a third tool (e.g. "marker") is a matter of adding
/// another arm here and tuning width/alpha — the content-stream builder
/// is otherwise generic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StampTool {
    Pen,
    Highlighter,
}

/// One stroke to bake into the output PDF.
///
/// Coordinates in `points` are normalized [0,1] with the **top-left** as
/// origin — the same space the live overlay uses. The Y axis flip to PDF
/// user space happens inside this module.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct StampStroke {
    /// 1-indexed page number. (Matches the frontend's `currentPage`.)
    pub page: u32,
    pub tool: StampTool,
    /// RGB triple, each component in `0.0..=1.0`.
    pub color: [f32; 3],
    /// Stroke width in PDF points. Typical: 3.0 for pen, 18.0 for highlighter.
    pub width_pt: f32,
    /// Path vertices in normalized [0,1] top-left space.
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StampAnnotationsOpts {
    pub strokes: Vec<StampStroke>,
}

/// Bake `opts.strokes` into a new content overlay on each affected page of
/// `input`, write the result to `output`, and return the number of strokes
/// successfully stamped.
pub fn stamp_annotations(
    input: &Path,
    output: &Path,
    opts: StampAnnotationsOpts,
) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if opts.strokes.is_empty() {
        return Err(PdfError::Other("no strokes to stamp".into()));
    }
    // Validate strokes up front so we don't partially-write the doc.
    for (i, s) in opts.strokes.iter().enumerate() {
        if s.points.len() < 2 {
            return Err(PdfError::Other(format!(
                "stroke {i} has fewer than 2 points"
            )));
        }
        if s.width_pt <= 0.0 || s.width_pt > 200.0 {
            return Err(PdfError::Other(format!(
                "stroke {i} has invalid width_pt {}",
                s.width_pt
            )));
        }
        for c in &s.color {
            if !(0.0..=1.0).contains(c) {
                return Err(PdfError::Other(format!(
                    "stroke {i} has out-of-range color component {c}"
                )));
            }
        }
        if s.page == 0 {
            return Err(PdfError::Other(format!(
                "stroke {i} has invalid page 0 (must be 1-indexed)"
            )));
        }
    }

    let mut doc = Document::load(input)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    // Reject out-of-range pages now that we know the doc's length.
    for (i, s) in opts.strokes.iter().enumerate() {
        if s.page > total {
            return Err(PdfError::Other(format!(
                "stroke {i} targets page {} but doc has {total} pages",
                s.page
            )));
        }
    }

    // Two ExtGState dicts shared across all pages: opaque (pen) + 42%
    // alpha (highlighter). We register both once and reuse references.
    let gs_opaque_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => 1.0_f64,
        "CA" => 1.0_f64,
    });
    let gs_hl_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => 0.42_f64,
        "CA" => 0.42_f64,
    });

    // Bucket strokes by page so we can build one stream per page.
    let mut by_page: BTreeMap<u32, Vec<&StampStroke>> = BTreeMap::new();
    for s in &opts.strokes {
        by_page.entry(s.page).or_default().push(s);
    }

    let mut stamped: u32 = 0;
    let mut page_updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();

    for (page_num, page_strokes) in &by_page {
        let page_id = match page_map.get(page_num) {
            Some(id) => *id,
            None => continue, // should be caught by validation, but be safe
        };
        let (w, h) = page_size(&doc, page_id).unwrap_or((612.0, 792.0));

        let mut ops: Vec<Operation> = Vec::with_capacity(page_strokes.len() * 8 + 4);
        // Outer save-state so we leave the page graphics state untouched.
        ops.push(Operation::new("q", vec![]));

        for stroke in page_strokes {
            ops.push(Operation::new("q", vec![]));
            // Select alpha state.
            let gs_name = match stroke.tool {
                StampTool::Pen => "SlabAnnOp",
                StampTool::Highlighter => "SlabAnnHL",
            };
            ops.push(Operation::new("gs", vec![gs_name.into()]));
            // Stroke color (RG = stroking RGB).
            ops.push(Operation::new(
                "RG",
                vec![
                    Object::Real(stroke.color[0]),
                    Object::Real(stroke.color[1]),
                    Object::Real(stroke.color[2]),
                ],
            ));
            // Width.
            ops.push(Operation::new("w", vec![Object::Real(stroke.width_pt)]));
            // Round line cap (1) + round line join (1).
            ops.push(Operation::new("J", vec![Object::Integer(1)]));
            ops.push(Operation::new("j", vec![Object::Integer(1)]));

            // Build the path. Y axis flip from top-left → bottom-left.
            let p0 = stroke.points[0];
            ops.push(Operation::new(
                "m",
                vec![
                    Object::Real(clamp01(p0[0]) * w),
                    Object::Real((1.0 - clamp01(p0[1])) * h),
                ],
            ));
            for p in &stroke.points[1..] {
                ops.push(Operation::new(
                    "l",
                    vec![
                        Object::Real(clamp01(p[0]) * w),
                        Object::Real((1.0 - clamp01(p[1])) * h),
                    ],
                ));
            }
            // Stroke the path.
            ops.push(Operation::new("S", vec![]));
            ops.push(Operation::new("Q", vec![]));
            stamped += 1;
        }

        ops.push(Operation::new("Q", vec![]));
        let content = Content { operations: ops };
        let bytes = content
            .encode()
            .map_err(|e| PdfError::Other(format!("encode content stream: {e}")))?;
        let stream_id = doc.add_object(Stream::new(dictionary! {}, bytes));
        page_updates.push((page_id, stream_id));
    }

    for (page_id, stream_id) in page_updates {
        append_overlay_stream(&mut doc, page_id, stream_id, gs_opaque_id, gs_hl_id)?;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(stamped)
}

#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
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

fn append_overlay_stream(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    new_stream_id: lopdf::ObjectId,
    gs_opaque_id: lopdf::ObjectId,
    gs_hl_id: lopdf::ObjectId,
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

        // Make sure Resources has /ExtGState /SlabAnnOp + /SlabAnnHL.
        let resources_value = dict.get(b"Resources").ok().cloned();
        let resources_dict = match resources_value {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(r)) => doc.get_object(r)?.as_dict()?.clone(),
            _ => lopdf::Dictionary::new(),
        };
        let mut resources = resources_dict;

        let mut gs = match resources.get(b"ExtGState") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        gs.set("SlabAnnOp", Object::Reference(gs_opaque_id));
        gs.set("SlabAnnHL", Object::Reference(gs_hl_id));
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

    fn pen_stroke(page: u32, n_points: usize) -> StampStroke {
        let points = (0..n_points)
            .map(|i| {
                let t = i as f32 / (n_points.max(2) - 1) as f32;
                [0.1 + t * 0.8, 0.5]
            })
            .collect();
        StampStroke {
            page,
            tool: StampTool::Pen,
            color: [1.0, 0.0, 0.0],
            width_pt: 3.0,
            points,
        }
    }

    fn hl_stroke(page: u32) -> StampStroke {
        StampStroke {
            page,
            tool: StampTool::Highlighter,
            color: [1.0, 0.9, 0.0],
            width_pt: 18.0,
            points: vec![[0.1, 0.3], [0.5, 0.3], [0.9, 0.3]],
        }
    }

    #[test]
    fn stamps_single_pen_stroke_one_page() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);
        let n = stamp_annotations(
            &src,
            &dst,
            StampAnnotationsOpts {
                strokes: vec![pen_stroke(2, 5)],
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
        // Output file is a valid, parseable PDF.
        let _doc = lopdf::Document::load(&dst).unwrap();
    }

    #[test]
    fn stamps_many_strokes_across_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 5);
        let strokes = vec![
            pen_stroke(1, 3),
            pen_stroke(1, 4),
            hl_stroke(2),
            pen_stroke(4, 6),
            hl_stroke(5),
        ];
        let n = stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes }).unwrap();
        assert_eq!(n, 5);
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 5);
    }

    #[test]
    fn empty_strokes_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let err =
            stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![] }).unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("no strokes"));
    }

    #[test]
    fn rejects_stroke_with_one_point() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let s = StampStroke {
            page: 1,
            tool: StampTool::Pen,
            color: [0.0, 0.0, 0.0],
            width_pt: 2.0,
            points: vec![[0.5, 0.5]],
        };
        let err =
            stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![s] }).unwrap_err();
        assert!(format!("{err}").contains("fewer than 2 points"));
    }

    #[test]
    fn rejects_bad_width() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let mut s = pen_stroke(1, 4);
        s.width_pt = -1.0;
        let err =
            stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![s] }).unwrap_err();
        assert!(format!("{err}").contains("invalid width_pt"));
    }

    #[test]
    fn rejects_bad_color() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let mut s = pen_stroke(1, 4);
        s.color = [1.2, 0.0, 0.0];
        let err =
            stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![s] }).unwrap_err();
        assert!(format!("{err}").contains("out-of-range color"));
    }

    #[test]
    fn rejects_page_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let mut s = pen_stroke(1, 4);
        s.page = 0;
        let err =
            stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![s] }).unwrap_err();
        assert!(format!("{err}").contains("page 0"));
    }

    #[test]
    fn rejects_page_out_of_range() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);
        let mut s = pen_stroke(1, 4);
        s.page = 99;
        let err =
            stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![s] }).unwrap_err();
        assert!(format!("{err}").contains("doc has 3 pages"));
    }

    #[test]
    fn missing_input_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");
        let err = stamp_annotations(
            &missing,
            &dst,
            StampAnnotationsOpts {
                strokes: vec![pen_stroke(1, 3)],
            },
        )
        .unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }

    #[test]
    fn output_has_extgstate_resource() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        stamp_annotations(
            &src,
            &dst,
            StampAnnotationsOpts {
                strokes: vec![hl_stroke(1)],
            },
        )
        .unwrap();
        // Crack open the output and assert that page 1 now has a Resources
        // entry referencing our ExtGState names.
        let doc = lopdf::Document::load(&dst).unwrap();
        let pages = doc.get_pages();
        let page_id = *pages.get(&1).unwrap();
        let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
        let resources = page.get(b"Resources").unwrap();
        let res_dict = match resources {
            lopdf::Object::Dictionary(d) => d.clone(),
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            _ => panic!("Resources is not a dict"),
        };
        let gs = res_dict.get(b"ExtGState").unwrap().as_dict().unwrap();
        assert!(gs.has(b"SlabAnnOp"));
        assert!(gs.has(b"SlabAnnHL"));
    }

    #[test]
    fn output_contents_grew() {
        // After stamping, page 1 must reference an additional content
        // stream (proves we didn't accidentally overwrite the original).
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        let src_doc = lopdf::Document::load(&src).unwrap();
        let src_pages = src_doc.get_pages();
        let src_p1 = src_doc
            .get_object(*src_pages.get(&1).unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        let n_before = match src_p1.get(b"Contents") {
            Ok(lopdf::Object::Reference(_)) => 1,
            Ok(lopdf::Object::Array(a)) => a.len(),
            _ => 0,
        };

        stamp_annotations(
            &src,
            &dst,
            StampAnnotationsOpts {
                strokes: vec![pen_stroke(1, 4)],
            },
        )
        .unwrap();

        let out_doc = lopdf::Document::load(&dst).unwrap();
        let out_pages = out_doc.get_pages();
        let out_p1 = out_doc
            .get_object(*out_pages.get(&1).unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        let n_after = match out_p1.get(b"Contents") {
            Ok(lopdf::Object::Reference(_)) => 1,
            Ok(lopdf::Object::Array(a)) => a.len(),
            _ => 0,
        };
        assert!(
            n_after > n_before,
            "expected more content streams after stamp (before={n_before}, after={n_after})"
        );
    }

    #[test]
    fn many_strokes_same_page() {
        // 30 strokes on page 1 — bulk-stamp smoke test.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let strokes: Vec<StampStroke> = (0..30).map(|_| pen_stroke(1, 8)).collect();
        let n = stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes }).unwrap();
        assert_eq!(n, 30);
        let _doc = lopdf::Document::load(&dst).unwrap();
    }

    #[test]
    fn coords_outside_unit_range_are_clamped() {
        // We don't reject; we clamp so a slight stylus overshoot doesn't
        // explode. Just make sure the stamp still succeeds.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let s = StampStroke {
            page: 1,
            tool: StampTool::Pen,
            color: [0.0, 0.0, 1.0],
            width_pt: 2.0,
            points: vec![[-0.1, 0.2], [1.2, 0.8]],
        };
        let n = stamp_annotations(&src, &dst, StampAnnotationsOpts { strokes: vec![s] }).unwrap();
        assert_eq!(n, 1);
    }
}
