//! Geometry pass for PDF/X-4 conversion.
//!
//! ISO 15930-7 §6.3 requires every page to declare a `/TrimBox` (or a
//! `/ArtBox`, but `TrimBox` is what print shops actually look at) so the
//! RIP knows where the final printed area lives within the larger
//! `/MediaBox` sheet. Most PDFs in the wild (Word exports, browser
//! "Save as PDF", scanner outputs) ship with only a `/MediaBox` set, so
//! Press has to synthesise the missing print boxes before the document
//! can claim X-4 conformance.
//!
//! # Rules this pass enforces
//!
//! - Every page MUST have a `/TrimBox`. If absent, synthesise:
//!   1. Copy from `/CropBox` if present (Acrobat's preferred fallback).
//!   2. Otherwise copy from `/MediaBox`.
//! - `/TrimBox` MUST satisfy `TrimBox ⊆ MediaBox`. If the existing
//!   TrimBox extends beyond MediaBox (a malformed input), clamp it.
//! - If `opts.add_bleed` is true, set `/BleedBox = TrimBox` outset by
//!   `bleed_pts` (default 8.504pt ≈ 3mm — the print industry standard
//!   for full-bleed jobs). The result is clamped to `MediaBox` so we
//!   never exceed the physical sheet.
//! - Existing `/TrimBox` entries are preserved (idempotent on a
//!   second run).
//!
//! All four rectangle entries (`MediaBox`, `CropBox`, `BleedBox`,
//! `TrimBox`, `ArtBox`) are PDF arrays of four numbers `[llx lly urx ury]`
//! in default user space units (points, 1/72 inch).

use lopdf::{Document, Object};

/// 3 mm expressed in PDF points (`3 / 25.4 * 72`). Industry standard
/// for full-bleed print jobs.
pub const DEFAULT_BLEED_PTS: f32 = 8.503_937;

/// Options for the geometry pass.
#[derive(Debug, Clone)]
pub struct GeometryOptions {
    /// Add a `/BleedBox` outset by `bleed_pts` on every page.
    pub add_bleed: bool,
    /// Bleed distance in PDF points. Default ~8.504 (3 mm).
    pub bleed_pts: f32,
}

impl Default for GeometryOptions {
    fn default() -> Self {
        Self {
            add_bleed: false,
            bleed_pts: DEFAULT_BLEED_PTS,
        }
    }
}

/// Stats produced by [`ensure_print_boxes`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeometryStats {
    /// Pages this pass mutated in any way.
    pub pages_touched: usize,
    /// Pages where `/TrimBox` was newly created.
    pub trimbox_synthesized: usize,
    /// Pages where `/TrimBox` already existed and was preserved.
    pub trimbox_preserved: usize,
    /// Pages where `/TrimBox` was clamped down to fit inside MediaBox.
    pub trimbox_clamped: usize,
    /// Pages where `/BleedBox` was added.
    pub bleed_added: usize,
}

/// Four-element rectangle helper, kept as `f64` so we don't lose
/// precision on the round-trip through `Object::Integer` / `Object::Real`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    llx: f64,
    lly: f64,
    urx: f64,
    ury: f64,
}

impl Rect {
    fn from_object(obj: &Object) -> Option<Self> {
        let arr = obj.as_array().ok()?;
        if arr.len() != 4 {
            return None;
        }
        let mut v = [0.0_f64; 4];
        for (i, o) in arr.iter().enumerate() {
            v[i] = number_as_f64(o)?;
        }
        // PDF rectangles are unordered — normalize so llx<=urx, lly<=ury.
        let (llx, urx) = if v[0] <= v[2] {
            (v[0], v[2])
        } else {
            (v[2], v[0])
        };
        let (lly, ury) = if v[1] <= v[3] {
            (v[1], v[3])
        } else {
            (v[3], v[1])
        };
        Some(Self { llx, lly, urx, ury })
    }

    fn to_object(self) -> Object {
        Object::Array(vec![
            Object::Real(self.llx as f32),
            Object::Real(self.lly as f32),
            Object::Real(self.urx as f32),
            Object::Real(self.ury as f32),
        ])
    }

    fn clamp_to(self, outer: Rect) -> Self {
        Self {
            llx: self.llx.max(outer.llx),
            lly: self.lly.max(outer.lly),
            urx: self.urx.min(outer.urx),
            ury: self.ury.min(outer.ury),
        }
    }

    fn outset(self, by: f64) -> Self {
        Self {
            llx: self.llx - by,
            lly: self.lly - by,
            urx: self.urx + by,
            ury: self.ury + by,
        }
    }

    fn fits_inside(self, outer: Rect) -> bool {
        self.llx >= outer.llx
            && self.lly >= outer.lly
            && self.urx <= outer.urx
            && self.ury <= outer.ury
    }
}

fn number_as_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// Resolve a `/MediaBox`-style entry on a page, walking the page-tree
/// inheritance chain. ISO 32000-2 §7.7.3.4 — `MediaBox` and `CropBox`
/// inherit from ancestor `/Pages` nodes if absent on the page leaf.
fn inherited_rect(doc: &Document, page_id: lopdf::ObjectId, key: &[u8]) -> Option<Rect> {
    let mut current = page_id;
    let mut depth = 0;
    loop {
        if depth > 32 {
            return None;
        }
        let dict = doc.get_object(current).ok()?.as_dict().ok()?;
        if let Ok(obj) = dict.get(key) {
            if let Some(r) = Rect::from_object(obj) {
                return Some(r);
            }
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(id)) => {
                current = *id;
                depth += 1;
            }
            _ => return None,
        }
    }
}

/// Ensure every page has the print boxes PDF/X-4 requires.
pub fn ensure_print_boxes(
    doc: &mut Document,
    opts: &GeometryOptions,
) -> Result<GeometryStats, String> {
    let mut stats = GeometryStats::default();
    let bleed = opts.bleed_pts.max(0.0) as f64;

    // Snapshot page list first; mutation invalidates the page-tree walker.
    let pages: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();

    for (_page_num, page_id) in pages {
        let media = match inherited_rect(doc, page_id, b"MediaBox") {
            Some(r) => r,
            // No MediaBox at all — the document is structurally broken;
            // skip the page rather than fail the whole conversion.
            None => continue,
        };
        let crop = inherited_rect(doc, page_id, b"CropBox");
        let existing_trim = inherited_rect(doc, page_id, b"TrimBox");

        let page_dict = doc
            .get_object_mut(page_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| format!("page {page_id:?}: {e}"))?;

        // ── 1. TrimBox ───────────────────────────────────────────────
        let mut touched = false;
        let trim_final = match existing_trim {
            Some(t) if t.fits_inside(media) => {
                stats.trimbox_preserved += 1;
                t
            }
            Some(t) => {
                // Existing but invalid (extends beyond MediaBox). Clamp.
                let clamped = t.clamp_to(media);
                page_dict.set("TrimBox", clamped.to_object());
                stats.trimbox_clamped += 1;
                touched = true;
                clamped
            }
            None => {
                let synth = crop.unwrap_or(media).clamp_to(media);
                page_dict.set("TrimBox", synth.to_object());
                stats.trimbox_synthesized += 1;
                touched = true;
                synth
            }
        };

        // ── 2. BleedBox (optional) ──────────────────────────────────
        if opts.add_bleed {
            // Idempotent — only add if not present.
            let has_bleed = page_dict.get(b"BleedBox").is_ok();
            if !has_bleed {
                let bleed_rect = trim_final.outset(bleed).clamp_to(media);
                page_dict.set("BleedBox", bleed_rect.to_object());
                stats.bleed_added += 1;
                touched = true;
            }
        }

        if touched {
            stats.pages_touched += 1;
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn page_with(media: [f32; 4], extras: &[(&str, [f32; 4])]) -> Document {
        let mut doc = Document::with_version("1.7");
        let mut page = dictionary! {
            "Type" => "Page",
            "MediaBox" => Object::Array(media.iter().map(|v| Object::Real(*v)).collect()),
        };
        for (k, r) in extras {
            page.set(
                *k,
                Object::Array(r.iter().map(|v| Object::Real(*v)).collect()),
            );
        }
        let page_id = doc.add_object(page);
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1i64,
        });
        // Wire parent
        {
            let p = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
            p.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(cat_id));
        doc
    }

    fn get_rect(doc: &Document, key: &[u8]) -> Option<Rect> {
        let pages = doc.get_pages();
        let (_, page_id) = pages.iter().next()?;
        inherited_rect(doc, *page_id, key)
    }

    #[test]
    fn synthesizes_trimbox_from_mediabox_when_absent() {
        let mut doc = page_with([0.0, 0.0, 612.0, 792.0], &[]);
        let stats = ensure_print_boxes(&mut doc, &GeometryOptions::default()).unwrap();
        assert_eq!(stats.trimbox_synthesized, 1);
        assert_eq!(stats.pages_touched, 1);
        let trim = get_rect(&doc, b"TrimBox").unwrap();
        assert_eq!(trim.urx, 612.0);
        assert_eq!(trim.ury, 792.0);
    }

    #[test]
    fn synthesizes_trimbox_from_cropbox_when_present() {
        let mut doc = page_with(
            [0.0, 0.0, 612.0, 792.0],
            &[("CropBox", [10.0, 10.0, 602.0, 782.0])],
        );
        let stats = ensure_print_boxes(&mut doc, &GeometryOptions::default()).unwrap();
        assert_eq!(stats.trimbox_synthesized, 1);
        let trim = get_rect(&doc, b"TrimBox").unwrap();
        assert_eq!(trim.llx, 10.0);
        assert_eq!(trim.urx, 602.0);
    }

    #[test]
    fn preserves_valid_existing_trimbox() {
        let mut doc = page_with(
            [0.0, 0.0, 612.0, 792.0],
            &[("TrimBox", [50.0, 50.0, 500.0, 700.0])],
        );
        let stats = ensure_print_boxes(&mut doc, &GeometryOptions::default()).unwrap();
        assert_eq!(stats.trimbox_synthesized, 0);
        assert_eq!(stats.trimbox_preserved, 1);
        assert_eq!(stats.pages_touched, 0);
        let trim = get_rect(&doc, b"TrimBox").unwrap();
        assert_eq!(trim.llx, 50.0);
    }

    #[test]
    fn clamps_trimbox_that_exceeds_mediabox() {
        let mut doc = page_with(
            [0.0, 0.0, 612.0, 792.0],
            &[("TrimBox", [-10.0, -10.0, 700.0, 800.0])],
        );
        let stats = ensure_print_boxes(&mut doc, &GeometryOptions::default()).unwrap();
        assert_eq!(stats.trimbox_clamped, 1);
        let trim = get_rect(&doc, b"TrimBox").unwrap();
        assert_eq!(trim.llx, 0.0);
        assert_eq!(trim.urx, 612.0);
        assert_eq!(trim.ury, 792.0);
    }

    #[test]
    fn adds_bleed_box_when_requested() {
        let mut doc = page_with([0.0, 0.0, 612.0, 792.0], &[]);
        let opts = GeometryOptions {
            add_bleed: true,
            bleed_pts: DEFAULT_BLEED_PTS,
        };
        let stats = ensure_print_boxes(&mut doc, &opts).unwrap();
        assert_eq!(stats.bleed_added, 1);
        let bleed = get_rect(&doc, b"BleedBox").unwrap();
        // TrimBox = MediaBox, so BleedBox clamps back to MediaBox.
        assert_eq!(bleed.llx, 0.0);
        assert_eq!(bleed.urx, 612.0);
    }

    #[test]
    fn bleed_outsets_when_trim_is_inset() {
        let mut doc = page_with(
            [0.0, 0.0, 612.0, 792.0],
            &[("TrimBox", [50.0, 50.0, 500.0, 700.0])],
        );
        let opts = GeometryOptions {
            add_bleed: true,
            bleed_pts: 10.0,
        };
        let stats = ensure_print_boxes(&mut doc, &opts).unwrap();
        assert_eq!(stats.bleed_added, 1);
        assert_eq!(stats.trimbox_preserved, 1);
        let bleed = get_rect(&doc, b"BleedBox").unwrap();
        assert_eq!(bleed.llx, 40.0);
        assert_eq!(bleed.lly, 40.0);
        assert_eq!(bleed.urx, 510.0);
        assert_eq!(bleed.ury, 710.0);
    }

    #[test]
    fn idempotent_on_second_run() {
        let mut doc = page_with([0.0, 0.0, 612.0, 792.0], &[]);
        let opts = GeometryOptions {
            add_bleed: true,
            ..Default::default()
        };
        ensure_print_boxes(&mut doc, &opts).unwrap();
        let stats2 = ensure_print_boxes(&mut doc, &opts).unwrap();
        // Second run: TrimBox preserved, no new BleedBox added.
        assert_eq!(stats2.trimbox_preserved, 1);
        assert_eq!(stats2.trimbox_synthesized, 0);
        assert_eq!(stats2.bleed_added, 0);
        assert_eq!(stats2.pages_touched, 0);
    }

    #[test]
    fn handles_inherited_mediabox_from_pages_node() {
        // Page leaf with NO MediaBox; Pages parent carries it.
        let mut doc = Document::with_version("1.7");
        let page_id = doc.add_object(dictionary! { "Type" => "Page" });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1i64,
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0), Object::Real(0.0),
                Object::Real(612.0), Object::Real(792.0),
            ]),
        });
        {
            let p = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
            p.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(cat_id));

        let stats = ensure_print_boxes(&mut doc, &GeometryOptions::default()).unwrap();
        assert_eq!(stats.trimbox_synthesized, 1);
    }
}
