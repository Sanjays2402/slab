//! Reversible page-tree operations for the Arranger undo/redo stack.
//!
//! The frontend records a 50-deep ring buffer of [`PageOp`]s and the backend
//! applies them in order against the host PDF on save. Every variant maps
//! 1:1 to an issue #26 acceptance criterion.
//!
//! See `docs/plans/2026-05-22-v2.1.2-arranger.md` Slice 1 for the design.

use serde::{Deserialize, Serialize};

/// A single, reversible page-tree operation.
///
/// All page indices are **1-based** to stay consistent with what the rest of
/// the `pdf::pages::*` API and the user-facing CLI expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageOp {
    /// Permanently delete the page at the given 1-based index.
    Delete { at: u32 },
    /// Duplicate page `from` and place the copy at `to` (1-based, post-insert).
    Duplicate { from: u32, to: u32 },
    /// Permanently rotate page `at` by `degrees` (one of `90`, `180`, `270`,
    /// or negative equivalents), baking the rotation into page geometry
    /// rather than the `/Rotate` viewer hint.
    Rotate { at: u32, degrees: i64 },
    /// Reorder pages — `order` is the full new ordering of 1-based indices.
    /// `order.len()` MUST equal the current page count.
    Reorder { order: Vec<u32> },
    /// Insert `count` blank pages of the given size at 1-based position `at`.
    InsertBlank {
        at: u32,
        count: u32,
        width: f32,
        height: f32,
    },
    /// Insert every page of another PDF at 1-based position `at`.
    InsertPdf { at: u32, path: String },
    /// Insert a PNG/JPG image as a single page at 1-based position `at`.
    InsertImage { at: u32, path: String },
}

impl PageOp {
    /// Human-readable label used in undo-stack tooltips on the frontend.
    pub fn label(&self) -> &'static str {
        match self {
            PageOp::Delete { .. } => "Delete page",
            PageOp::Duplicate { .. } => "Duplicate page",
            PageOp::Rotate { .. } => "Rotate page",
            PageOp::Reorder { .. } => "Reorder pages",
            PageOp::InsertBlank { .. } => "Insert blank page",
            PageOp::InsertPdf { .. } => "Insert PDF",
            PageOp::InsertImage { .. } => "Insert image",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_op_round_trips_through_serde_json() {
        let op = PageOp::Rotate { at: 3, degrees: 90 };
        let s = serde_json::to_string(&op).unwrap();
        let back: PageOp = serde_json::from_str(&s).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn page_op_uses_snake_case_kind_tag() {
        let s = serde_json::to_string(&PageOp::InsertBlank {
            at: 1,
            count: 2,
            width: 612.0,
            height: 792.0,
        })
        .unwrap();
        assert!(s.contains(r#""kind":"insert_blank""#), "got: {s}");
    }

    #[test]
    fn reorder_round_trip() {
        let op = PageOp::Reorder {
            order: vec![3, 1, 2, 4, 5],
        };
        let s = serde_json::to_string(&op).unwrap();
        let back: PageOp = serde_json::from_str(&s).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(PageOp::Delete { at: 1 }.label(), "Delete page");
        assert_eq!(
            PageOp::InsertImage {
                at: 1,
                path: "x.png".into()
            }
            .label(),
            "Insert image"
        );
    }
}
