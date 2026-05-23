//! Theater presentation state.
//!
//! Single source of truth for an active presenter-mode session. Cloned on
//! every mutation and broadcast to the audience window via Tauri events.
//! All page indices in the public API are **1-based** (matching pdf.js and
//! the rest of the Slab UI). Ink stroke points are normalized 0..1 in page
//! space so the audience renderer can scale them to any viewport size.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single freehand ink stroke captured on the presenter window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InkStroke {
    /// 1-based page index the stroke belongs to.
    pub page: u32,
    /// Normalized `[x, y]` pairs in `[0.0, 1.0]` page-space coordinates.
    pub points: Vec<[f32; 2]>,
    /// CSS color (e.g. `"#ff3b30"`).
    pub color: String,
    /// Stroke width in CSS pixels at 1× zoom.
    pub width: f32,
}

impl InkStroke {
    /// Construct a new empty stroke for `page` with the given color/width.
    pub fn new(page: u32, color: impl Into<String>, width: f32) -> Self {
        Self {
            page,
            points: Vec::new(),
            color: color.into(),
            width: width.max(0.5),
        }
    }

    /// Append a normalized point, clamping each coordinate to `[0, 1]`.
    pub fn push(&mut self, x: f32, y: f32) {
        self.points.push([x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)]);
    }
}

/// Live state of one presenter session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TheaterState {
    pub path: PathBuf,
    /// 1-based current page (clamped to `1..=total_pages`).
    pub current_page: u32,
    /// Total pages in the PDF; must be ≥ 1.
    pub total_pages: u32,
    pub blackout: bool,
    pub whiteout: bool,
    pub laser_on: bool,
    pub ink_mode: bool,
    pub spotlight_on: bool,
    pub ink_strokes: Vec<InkStroke>,
    /// Unix epoch milliseconds when the session started.
    pub started_at_ms: u64,
}

impl TheaterState {
    /// Create a new session at page 1 with all overlay flags off.
    pub fn new(path: PathBuf, total_pages: u32) -> Self {
        Self {
            path,
            current_page: 1,
            total_pages: total_pages.max(1),
            blackout: false,
            whiteout: false,
            laser_on: false,
            ink_mode: false,
            spotlight_on: false,
            ink_strokes: Vec::new(),
            started_at_ms: now_ms(),
        }
    }

    /// Advance one page; clamps at `total_pages`.
    pub fn next(&mut self) {
        if self.current_page < self.total_pages {
            self.current_page += 1;
        }
    }

    /// Step back one page; clamps at 1.
    pub fn prev(&mut self) {
        if self.current_page > 1 {
            self.current_page -= 1;
        }
    }

    /// Jump to an absolute page; clamps to `1..=total_pages`.
    pub fn jump(&mut self, page: u32) {
        self.current_page = page.clamp(1, self.total_pages);
    }

    /// Toggle blackout (mutually exclusive with whiteout).
    pub fn toggle_blackout(&mut self) {
        self.blackout = !self.blackout;
        if self.blackout {
            self.whiteout = false;
        }
    }

    /// Toggle whiteout (mutually exclusive with blackout).
    pub fn toggle_whiteout(&mut self) {
        self.whiteout = !self.whiteout;
        if self.whiteout {
            self.blackout = false;
        }
    }

    /// Toggle laser pointer (mutually exclusive with ink-mode capture).
    pub fn toggle_laser(&mut self) {
        self.laser_on = !self.laser_on;
        if self.laser_on {
            self.ink_mode = false;
        }
    }

    /// Toggle ink capture mode (mutually exclusive with laser).
    pub fn toggle_ink(&mut self) {
        self.ink_mode = !self.ink_mode;
        if self.ink_mode {
            self.laser_on = false;
        }
    }

    /// Toggle the spotlight cursor overlay.
    pub fn toggle_spotlight(&mut self) {
        self.spotlight_on = !self.spotlight_on;
    }

    /// Append an ink stroke; dropped silently if it has fewer than 2 points
    /// (a single dot conveys nothing across two windows).
    pub fn push_stroke(&mut self, stroke: InkStroke) {
        if stroke.points.len() >= 2 {
            self.ink_strokes.push(stroke);
        }
    }

    /// Remove the most recently captured stroke (presenter "undo").
    pub fn undo_stroke(&mut self) -> Option<InkStroke> {
        self.ink_strokes.pop()
    }

    /// Drop every stroke on every page.
    pub fn clear_strokes(&mut self) {
        self.ink_strokes.clear();
    }

    /// Read-only view of strokes belonging to `page`.
    pub fn strokes_for_page(&self, page: u32) -> impl Iterator<Item = &InkStroke> {
        self.ink_strokes.iter().filter(move |s| s.page == page)
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> TheaterState {
        TheaterState::new(PathBuf::from("/tmp/x.pdf"), 5)
    }

    #[test]
    fn new_session_starts_at_page_one_overlays_off() {
        let st = s();
        assert_eq!(st.current_page, 1);
        assert_eq!(st.total_pages, 5);
        assert!(!st.blackout);
        assert!(!st.whiteout);
        assert!(!st.laser_on);
        assert!(!st.ink_mode);
        assert!(!st.spotlight_on);
        assert!(st.ink_strokes.is_empty());
    }

    #[test]
    fn total_pages_clamps_to_one_minimum() {
        let st = TheaterState::new(PathBuf::from("/tmp/x.pdf"), 0);
        assert_eq!(st.total_pages, 1);
    }

    #[test]
    fn next_clamps_to_total() {
        let mut st = s();
        for _ in 0..10 {
            st.next();
        }
        assert_eq!(st.current_page, 5);
    }

    #[test]
    fn prev_clamps_to_one() {
        let mut st = s();
        st.prev();
        st.prev();
        assert_eq!(st.current_page, 1);
    }

    #[test]
    fn jump_clamps_both_ends() {
        let mut st = s();
        st.jump(99);
        assert_eq!(st.current_page, 5);
        st.jump(0);
        assert_eq!(st.current_page, 1);
        st.jump(3);
        assert_eq!(st.current_page, 3);
    }

    #[test]
    fn blackout_and_whiteout_are_mutually_exclusive() {
        let mut st = s();
        st.toggle_blackout();
        assert!(st.blackout);
        st.toggle_whiteout();
        assert!(st.whiteout);
        assert!(!st.blackout);
        st.toggle_blackout();
        assert!(st.blackout);
        assert!(!st.whiteout);
    }

    #[test]
    fn laser_and_ink_are_mutually_exclusive() {
        let mut st = s();
        st.toggle_laser();
        assert!(st.laser_on);
        st.toggle_ink();
        assert!(st.ink_mode);
        assert!(!st.laser_on);
        st.toggle_laser();
        assert!(st.laser_on);
        assert!(!st.ink_mode);
    }

    #[test]
    fn spotlight_toggle_is_independent() {
        let mut st = s();
        st.toggle_spotlight();
        assert!(st.spotlight_on);
        st.toggle_blackout();
        assert!(st.spotlight_on);
        st.toggle_spotlight();
        assert!(!st.spotlight_on);
    }

    #[test]
    fn ink_stroke_clamps_points_to_unit_square() {
        let mut stroke = InkStroke::new(1, "#ff0000", 2.0);
        stroke.push(-0.5, 1.5);
        stroke.push(0.5, 0.5);
        assert_eq!(stroke.points, vec![[0.0, 1.0], [0.5, 0.5]]);
    }

    #[test]
    fn ink_stroke_enforces_minimum_width() {
        let stroke = InkStroke::new(1, "#000", 0.1);
        assert!(stroke.width >= 0.5);
    }

    #[test]
    fn push_stroke_rejects_single_point() {
        let mut st = s();
        let mut stroke = InkStroke::new(1, "#000", 2.0);
        stroke.push(0.1, 0.1);
        st.push_stroke(stroke);
        assert!(st.ink_strokes.is_empty());
    }

    #[test]
    fn push_stroke_accepts_two_or_more_points() {
        let mut st = s();
        let mut stroke = InkStroke::new(1, "#000", 2.0);
        stroke.push(0.1, 0.1);
        stroke.push(0.2, 0.2);
        st.push_stroke(stroke);
        assert_eq!(st.ink_strokes.len(), 1);
    }

    #[test]
    fn undo_stroke_pops_last() {
        let mut st = s();
        for i in 0..3 {
            let mut k = InkStroke::new(1, "#000", 2.0);
            k.push(0.0, 0.0);
            k.push(i as f32 * 0.1 + 0.1, 0.1);
            st.push_stroke(k);
        }
        assert_eq!(st.ink_strokes.len(), 3);
        let popped = st.undo_stroke().unwrap();
        assert_eq!(popped.points[1][0], 0.3);
        assert_eq!(st.ink_strokes.len(), 2);
    }

    #[test]
    fn clear_strokes_drops_everything() {
        let mut st = s();
        let mut k = InkStroke::new(1, "#000", 2.0);
        k.push(0.0, 0.0);
        k.push(0.5, 0.5);
        st.push_stroke(k);
        st.clear_strokes();
        assert!(st.ink_strokes.is_empty());
    }

    #[test]
    fn strokes_for_page_filters() {
        let mut st = s();
        for p in [1u32, 2, 1, 3, 2] {
            let mut k = InkStroke::new(p, "#000", 2.0);
            k.push(0.0, 0.0);
            k.push(0.5, 0.5);
            st.push_stroke(k);
        }
        assert_eq!(st.strokes_for_page(1).count(), 2);
        assert_eq!(st.strokes_for_page(2).count(), 2);
        assert_eq!(st.strokes_for_page(3).count(), 1);
        assert_eq!(st.strokes_for_page(4).count(), 0);
    }

    #[test]
    fn state_serde_roundtrips() {
        let mut st = s();
        st.jump(3);
        st.toggle_blackout();
        let mut k = InkStroke::new(3, "#ff3b30", 2.5);
        k.push(0.1, 0.2);
        k.push(0.3, 0.4);
        st.push_stroke(k);
        let json = serde_json::to_string(&st).unwrap();
        let back: TheaterState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, st);
    }

    #[test]
    fn serialized_keys_are_snake_camel_stable() {
        // The frontend twin depends on these exact field names. If you
        // rename a field, you MUST update the TS twin in lockstep.
        let st = s();
        let json = serde_json::to_value(&st).unwrap();
        let obj = json.as_object().unwrap();
        for key in [
            "path",
            "current_page",
            "total_pages",
            "blackout",
            "whiteout",
            "laser_on",
            "ink_mode",
            "spotlight_on",
            "ink_strokes",
            "started_at_ms",
        ] {
            assert!(obj.contains_key(key), "missing key {key}");
        }
    }
}
