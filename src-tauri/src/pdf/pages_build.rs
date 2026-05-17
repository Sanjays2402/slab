// Composite page-builder — given a list of cells (each representing one
// output page: either a copy of a source page or a blank), produce a single
// PDF that materializes that layout exactly. Each cell can also carry a
// per-page rotation that's applied on top of the source's existing rotation.
//
// This is the "Apply" backend for the visual Pages panel (drag-reorder grid).
// It composes our existing primitives:
//   * `extract_pages_to` handles the source-page multiset + ordering
//   * `pdf::insert::insert` splices blank pages at the right positions
//   * `pdf::pages::rotate_pages` applies per-cell rotation
//
// One Tauri round-trip; no chained intermediate files; deterministic.

use crate::pdf::insert::{insert, InsertOpts, InsertSource};
use crate::pdf::pages::{rotate_pages, Rotation};
use crate::pdf::split::{extract_pages_to, page_count};
use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One output page. `source = Some(n)` copies page `n` from the input (1-based).
/// `source = None` inserts a blank page at this cell position.
/// `rotation` is added on top of the source's existing rotation, in degrees
/// (must be 0, 90, 180, or 270).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PageCell {
    pub source: Option<u32>,
    #[serde(default)]
    pub rotation: u16,
}

/// Default size for blank pages (US Letter in points). Frontend can override.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct BlankSize {
    pub width: f32,
    pub height: f32,
}

impl Default for BlankSize {
    fn default() -> Self {
        BlankSize {
            width: 612.0,
            height: 792.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PagesBuildOpts {
    pub cells: Vec<PageCell>,
    #[serde(default)]
    pub blank: Option<BlankSize>,
}

/// Materialize a sequence of [`PageCell`]s into a new PDF on disk.
///
/// Returns the number of pages written (always equal to `cells.len()`).
pub fn pages_build(input: &Path, opts: &PagesBuildOpts, output: &Path) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if opts.cells.is_empty() {
        return Err(PdfError::Other("no cells specified".into()));
    }
    let total = page_count(input)?;

    // Validate: rotations are clean multiples of 90; source indices in range.
    for (i, c) in opts.cells.iter().enumerate() {
        if ![0u16, 90, 180, 270].contains(&c.rotation) {
            return Err(PdfError::Other(format!(
                "cell {} has invalid rotation {} (must be 0, 90, 180, or 270)",
                i + 1,
                c.rotation
            )));
        }
        if let Some(p) = c.source {
            if p == 0 || p > total {
                return Err(PdfError::Other(format!(
                    "cell {} references page {p} out of range (1..={total})",
                    i + 1
                )));
            }
        }
    }
    if opts.cells.iter().all(|c| c.source.is_none()) {
        return Err(PdfError::Other(
            "at least one cell must reference a source page".into(),
        ));
    }

    // Strategy:
    //   1. Build the ordered multiset of source indices (skipping blanks),
    //      and extract them in one shot into a working file. After this,
    //      the working file's page order corresponds 1-to-1 with the
    //      non-blank cells.
    //   2. Walk the cell list left-to-right. Maintain a `cursor` that points
    //      to where in the working file the next blank insertion should land.
    //      For each blank cell, splice exactly one blank page in.
    //   3. Apply per-cell rotation: group cells by rotation degrees and call
    //      `rotate_pages` once per group.
    //
    // The intermediate files live in the same directory as `output`, named
    // `__slab_build_<step>.pdf`, and are cleaned up at the end.

    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&parent)?;
    let intermediate = |step: u32| parent.join(format!("__slab_build_{step}.pdf"));
    let mut intermediates: Vec<PathBuf> = Vec::new();

    // Step 1: extract non-blank source pages (multiset preserved).
    let sources: Vec<u32> = opts.cells.iter().filter_map(|c| c.source).collect();
    let mut step = 0u32;
    step += 1;
    let mut current = intermediate(step);
    extract_pages_to(input, &sources, &current)?;
    intermediates.push(current.clone());

    // Step 2: splice blank pages in at each blank cell's index.
    let blank_size = opts.blank.unwrap_or_default();
    for (cell_idx, c) in opts.cells.iter().enumerate() {
        if c.source.is_some() {
            continue;
        }
        // Insert blank at cell_idx (0-based) → at = cell_idx + 1 (1-based,
        // meaning "place before existing page at that position"). When
        // cell_idx == 0, insert at the very start. When cell_idx >= current
        // page count, insert at the end (insert() clamps the index).
        step += 1;
        let next = intermediate(step);
        insert(
            &current,
            &next,
            InsertOpts {
                at: (cell_idx + 1) as u32,
                source: InsertSource::Blank {
                    count: 1,
                    width: blank_size.width,
                    height: blank_size.height,
                },
            },
        )?;
        current = next.clone();
        intermediates.push(next);
    }

    // Step 3: apply per-cell rotations, grouped by angle.
    let mut by_rot: [Vec<u32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (cell_idx, c) in opts.cells.iter().enumerate() {
        let page_no = (cell_idx + 1) as u32;
        match c.rotation {
            90 => by_rot[0].push(page_no),
            180 => by_rot[1].push(page_no),
            270 => by_rot[2].push(page_no),
            _ => {}
        }
    }
    for (deg, pages) in [
        (Rotation::Cw90, 0),
        (Rotation::Cw180, 1),
        (Rotation::Cw270, 2),
    ]
    .into_iter()
    .filter_map(|(r, i)| (!by_rot[i].is_empty()).then_some((r, std::mem::take(&mut by_rot[i]))))
    {
        step += 1;
        let next = intermediate(step);
        rotate_pages(&current, &pages, deg, &next)?;
        current = next.clone();
        intermediates.push(next);
    }

    // Move the final intermediate to `output`.
    if current != output {
        // Rename works inside the same dir.
        if let Err(e) = std::fs::rename(&current, output) {
            // Fall back to copy + remove for cross-volume edge cases.
            std::fs::copy(&current, output)
                .map_err(|_| PdfError::Other(format!("failed to finalize output: {e}")))?;
            let _ = std::fs::remove_file(&current);
        }
    }

    // Clean up any remaining intermediates (the last one was renamed above).
    for p in intermediates {
        if p == *output {
            continue;
        }
        let _ = std::fs::remove_file(p);
    }

    Ok(opts.cells.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::split::page_count;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    fn cell(source: Option<u32>, rotation: u16) -> PageCell {
        PageCell { source, rotation }
    }

    #[test]
    fn build_pure_reorder() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![cell(Some(3), 0), cell(Some(1), 0), cell(Some(2), 0)],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 3);
        assert_eq!(page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn build_subset_deletion() {
        // 5pp → keep [1, 3, 5]
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 5);

        let opts = PagesBuildOpts {
            cells: vec![cell(Some(1), 0), cell(Some(3), 0), cell(Some(5), 0)],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 3);
        assert_eq!(page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn build_with_duplicates() {
        // 3pp → [1, 2, 2, 3, 3, 3]
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![
                cell(Some(1), 0),
                cell(Some(2), 0),
                cell(Some(2), 0),
                cell(Some(3), 0),
                cell(Some(3), 0),
                cell(Some(3), 0),
            ],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 6);
        assert_eq!(page_count(&dst).unwrap(), 6);
    }

    #[test]
    fn build_with_blank_at_start() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![
                cell(None, 0),
                cell(Some(1), 0),
                cell(Some(2), 0),
                cell(Some(3), 0),
            ],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 4);
        assert_eq!(page_count(&dst).unwrap(), 4);
    }

    #[test]
    fn build_with_blank_in_middle() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![
                cell(Some(1), 0),
                cell(None, 0),
                cell(Some(2), 0),
                cell(Some(3), 0),
            ],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn build_multiple_blanks() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);

        let opts = PagesBuildOpts {
            cells: vec![
                cell(Some(1), 0),
                cell(None, 0),
                cell(None, 0),
                cell(Some(2), 0),
                cell(None, 0),
            ],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 5);
        assert_eq!(page_count(&dst).unwrap(), 5);
    }

    #[test]
    fn build_with_rotation() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![cell(Some(1), 90), cell(Some(2), 180), cell(Some(3), 270)],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn build_combo_dup_blank_rotate() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        // Output: [P1, P1(rot 90), BLANK, P3, P2(rot 180)]
        let opts = PagesBuildOpts {
            cells: vec![
                cell(Some(1), 0),
                cell(Some(1), 90),
                cell(None, 0),
                cell(Some(3), 0),
                cell(Some(2), 180),
            ],
            blank: None,
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 5);
        assert_eq!(page_count(&dst).unwrap(), 5);
    }

    #[test]
    fn build_rejects_empty_cells() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![],
            blank: None,
        };
        let err = pages_build(&src, &opts, &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("no cells"));
    }

    #[test]
    fn build_rejects_all_blank() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![cell(None, 0), cell(None, 0)],
            blank: None,
        };
        let err = pages_build(&src, &opts, &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("at least one cell"));
    }

    #[test]
    fn build_rejects_out_of_range_source() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![cell(Some(99), 0)],
            blank: None,
        };
        let err = pages_build(&src, &opts, &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn build_rejects_bad_rotation() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let opts = PagesBuildOpts {
            cells: vec![cell(Some(1), 45)],
            blank: None,
        };
        let err = pages_build(&src, &opts, &dst).unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("invalid rotation"));
    }

    #[test]
    fn build_missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");

        let opts = PagesBuildOpts {
            cells: vec![cell(Some(1), 0)],
            blank: None,
        };
        let err = pages_build(&bogus, &opts, &dst).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }

    #[test]
    fn build_custom_blank_size() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        // A4 portrait in points.
        let opts = PagesBuildOpts {
            cells: vec![cell(Some(1), 0), cell(None, 0)],
            blank: Some(BlankSize {
                width: 595.0,
                height: 842.0,
            }),
        };
        let n = pages_build(&src, &opts, &dst).unwrap();
        assert_eq!(n, 2);
    }
}
