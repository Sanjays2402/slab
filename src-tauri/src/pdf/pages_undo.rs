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

/// Apply a sequence of [`PageOp`]s in order against `input`, writing the
/// final document to `output` via `atomic_save`. Intermediate steps cascade
/// through tempfiles inside the system temp directory so the user-visible
/// host PDF on disk is touched exactly once.
///
/// Best-effort: insert_pdf / insert_image / insert_blank that point at
/// non-existent paths return `PdfError::Io`. Unknown rotation degrees fall
/// back to the no-op identity.
pub fn apply_ops(
    input: &std::path::Path,
    ops: &[PageOp],
    output: &std::path::Path,
) -> Result<(), crate::pdf::PdfError> {
    use crate::pdf::{
        duplicate::duplicate_pages,
        insert::{insert, InsertOpts, InsertSource},
        pages::{delete_pages, reorder_pages, rotate_pages_permanent, Rotation},
    };
    use tempfile::NamedTempFile;

    // Pipe stage state: every iteration produces a new tempfile.
    let mut current: std::path::PathBuf = input.to_path_buf();
    let mut owned: Vec<NamedTempFile> = Vec::new();

    for op in ops {
        let next = NamedTempFile::new().map_err(crate::pdf::PdfError::Io)?;
        let next_path = next.path().to_path_buf();
        match op {
            PageOp::Delete { at } => {
                delete_pages(&current, &[*at], &next_path)?;
            }
            PageOp::Duplicate { from, .. } => {
                duplicate_pages(&current, &[*from], &next_path)?;
            }
            PageOp::Rotate { at, degrees } => {
                let rot = Rotation::from_int(*degrees)?;
                rotate_pages_permanent(&current, &[*at], rot, &next_path)?;
            }
            PageOp::Reorder { order } => {
                reorder_pages(&current, order, &next_path)?;
            }
            PageOp::InsertBlank {
                at,
                count,
                width,
                height,
            } => {
                insert(
                    &current,
                    &next_path,
                    InsertOpts {
                        at: *at,
                        source: InsertSource::Blank {
                            count: *count,
                            width: *width,
                            height: *height,
                        },
                    },
                )?;
            }
            PageOp::InsertPdf { at, path } => {
                insert(
                    &current,
                    &next_path,
                    InsertOpts {
                        at: *at,
                        source: InsertSource::Pdf { path: path.clone() },
                    },
                )?;
            }
            PageOp::InsertImage { at, path } => {
                insert(
                    &current,
                    &next_path,
                    InsertOpts {
                        at: *at,
                        source: InsertSource::Image {
                            path: path.clone(),
                            dpi: 150.0,
                        },
                    },
                )?;
            }
        }
        owned.push(next);
        current = next_path;
    }

    // Final atomic copy from the last stage to `output`.
    let bytes = std::fs::read(&current).map_err(crate::pdf::PdfError::Io)?;
    crate::pdf::atomic_save(output, &bytes)?;
    Ok(())
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

    #[test]
    fn apply_ops_chains_rotate_then_delete() {
        use crate::pdf::test_fixtures::make_n_page_pdf;
        let src = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(src.path(), 5);
        let out = tempfile::NamedTempFile::new().unwrap();
        let ops = vec![
            PageOp::Rotate { at: 1, degrees: 90 },
            PageOp::Delete { at: 3 },
        ];
        apply_ops(src.path(), &ops, out.path()).expect("apply_ops should succeed");
        let doc = lopdf::Document::load(out.path()).expect("output is valid pdf");
        assert_eq!(doc.get_pages().len(), 4);
    }

    #[test]
    fn apply_ops_empty_list_copies_input() {
        use crate::pdf::test_fixtures::make_n_page_pdf;
        let src = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(src.path(), 2);
        let out = tempfile::NamedTempFile::new().unwrap();
        apply_ops(src.path(), &[], out.path()).expect("empty ops should be a no-op copy");
        let doc = lopdf::Document::load(out.path()).expect("output is valid pdf");
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn apply_ops_reorder_full() {
        use crate::pdf::test_fixtures::make_n_page_pdf;
        let src = tempfile::NamedTempFile::new().unwrap();
        make_n_page_pdf(src.path(), 4);
        let out = tempfile::NamedTempFile::new().unwrap();
        let ops = vec![PageOp::Reorder {
            order: vec![4, 3, 2, 1],
        }];
        apply_ops(src.path(), &ops, out.path()).expect("reorder should succeed");
        let doc = lopdf::Document::load(out.path()).expect("output is valid pdf");
        assert_eq!(doc.get_pages().len(), 4);
    }
}
