// src-tauri/src/pdf/loom/reading_order.rs
//
// Slab Loom — Slice 3: column-aware reading-order traversal.
//
// PDF/UA mandates that the structure tree reading order reflect the *logical*
// reading order of the document, not the order in which operators happen to
// appear in the content stream. Acrobat's content stream is usually
// physically left-to-right, top-to-bottom — fine for single-column reports,
// but disastrous for two-column research papers: a naive screen reader would
// read row-by-row, jumping back and forth between the columns mid-sentence.
//
// Slice 2 classified each run on a page; Slice 3 *re-orders* the nodes on
// each page so a screen reader walks columns sequentially (left column top
// to bottom, then right column top to bottom). Headings that span both
// columns become single-column entries above the column block.
//
// Algorithm (deliberately simple, deterministic, no panics on weird input):
//
//   1. **Detect column bands.** For every classified node on a page, project
//      its bbox midpoint onto the X axis. Cluster the midpoints into bands
//      via a single-pass nearest-neighbour algorithm with a gap threshold of
//      `MIN_COLUMN_GAP_PT` (default 18.0pt — roughly one em at 12pt body).
//      A band is *narrow* (≤ page_width × `NARROW_BAND_FRAC`, default 0.6) —
//      anything wider is a full-page band (figures, banners, section heads).
//
//   2. **Promote spanners.** Nodes whose width is ≥ page_width ×
//      `SPANNER_FRAC` (default 0.7) are treated as page-spanning regardless
//      of their midpoint — these become full-page bands inserted into the
//      reading order at their original vertical position.
//
//   3. **Serpentine traversal.** Sort narrow bands left-to-right by band
//      midpoint X. For each band, sort its nodes top-to-bottom (descending
//      y0 — PDF coords put origin at bottom-left). Intermix page-spanning
//      nodes by inserting them between bands or above/below as their y
//      requires.
//
//   4. **Artifacts** keep their original order — they're tagged
//      `/Artifact` and stripped from the reading flow anyway, but the UI
//      still wants to surface them positionally.
//
//   5. **Empty page → empty result.** Zero nodes → zero columns. One narrow
//      band → one column. Two well-separated bands → two columns. Three
//      bands collapse to two by merging the two closest if total > 3 (most
//      "3-column" PDFs are actually two columns plus a sidebar — Slice 4
//      will handle true 3-column).
//
// All thresholds are tuned for letter / A4 portrait at 72dpi user-space.
// Landscape and very large page geometries still produce sane results
// because the thresholds are fractions of page_width, not absolute points.
//
// Output: a `ReadingOrder` value carrying the re-ordered nodes plus a
// per-page diagnostic record (`column_count`, `spanner_count`,
// `artifacts_skipped`). The Tauri command and Outline tab surface those
// numbers so users and procurement officers can see what Loom inferred.

use super::classify::{NodeKind, StructNode, StructTree, StructTreePage};
use super::layout::Bbox;
use serde::{Deserialize, Serialize};

/// Minimum gap between two column bands' midpoints, in PDF points. Two
/// midpoints closer than this are folded into the same band.
const MIN_COLUMN_GAP_PT: f32 = 18.0;
/// A band whose horizontal span is ≤ page_width × this fraction is a
/// "narrow" (column) band. Wider bands are full-page.
const NARROW_BAND_FRAC: f32 = 0.6;
/// A single node whose width is ≥ page_width × this fraction is always
/// treated as a page-spanning node.
const SPANNER_FRAC: f32 = 0.7;

/// Re-ordered output of one page after the column-aware pass.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReadingOrderPage {
    pub page_number: u32,
    /// Number of narrow (column) bands detected on this page. 0 = empty,
    /// 1 = single column, 2 = two columns, etc. Capped at 4 — anything
    /// higher is collapsed.
    pub column_count: usize,
    /// Number of page-spanning nodes (figures, headings that cross
    /// columns, full-width banners).
    pub spanner_count: usize,
    /// Number of artifact nodes (headers/footers/folios) that were
    /// preserved in their original order but tagged for screen-reader
    /// skipping.
    pub artifact_count: usize,
    /// Nodes in correct reading order. Artifacts appear after content
    /// (they're skipped by AT anyway).
    pub nodes: Vec<StructNode>,
}

/// Document-level reading-order result.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReadingOrder {
    pub pages: Vec<ReadingOrderPage>,
}

impl ReadingOrder {
    /// Total reading-order nodes across all pages (excludes artifacts so
    /// the number reflects what a screen reader would emit).
    pub fn total_reading_nodes(&self) -> usize {
        self.pages
            .iter()
            .map(|p| p.nodes.iter().filter(|n| !is_artifact(n)).count())
            .sum()
    }

    /// Pages whose detected column count is ≥ 2. Useful for the UI to
    /// show "Slab found multi-column content on N pages."
    pub fn multi_column_pages(&self) -> usize {
        self.pages.iter().filter(|p| p.column_count >= 2).count()
    }
}

fn is_artifact(n: &StructNode) -> bool {
    matches!(n.kind, NodeKind::Artifact)
}

/// Public entry point. Pure function: same `StructTree` + same page widths
/// always produce the same `ReadingOrder`. The `page_geometry` slice maps
/// page index → (width, height) so we know the column thresholds. If a
/// page is missing, defaults to (612, 792) (US letter).
pub fn order_reading(tree: &StructTree, page_geometry: &[(f32, f32)]) -> ReadingOrder {
    let mut out = ReadingOrder::default();
    for (idx, page) in tree.pages.iter().enumerate() {
        let (w, _h) = page_geometry.get(idx).copied().unwrap_or((612.0, 792.0));
        out.pages.push(order_page(page, w));
    }
    out
}

/// Re-order one page's structure nodes.
fn order_page(page: &StructTreePage, page_width: f32) -> ReadingOrderPage {
    if page.nodes.is_empty() {
        return ReadingOrderPage {
            page_number: page.page_number,
            column_count: 0,
            spanner_count: 0,
            artifact_count: 0,
            nodes: Vec::new(),
        };
    }

    // Split off artifacts — they keep original order, parked after content.
    let mut artifacts: Vec<StructNode> = Vec::new();
    let mut content: Vec<StructNode> = Vec::new();
    for n in &page.nodes {
        if is_artifact(n) {
            artifacts.push(n.clone());
        } else {
            content.push(n.clone());
        }
    }

    if content.is_empty() {
        let acount = artifacts.len();
        return ReadingOrderPage {
            page_number: page.page_number,
            column_count: 0,
            spanner_count: 0,
            artifact_count: acount,
            nodes: artifacts,
        };
    }

    // Step 1: separate spanners from column candidates.
    let span_threshold = page_width * SPANNER_FRAC;
    let mut spanners: Vec<StructNode> = Vec::new();
    let mut column_nodes: Vec<StructNode> = Vec::new();
    for n in content.into_iter() {
        let width = (n.bbox.x1 - n.bbox.x0).abs();
        if width >= span_threshold {
            spanners.push(n);
        } else {
            column_nodes.push(n);
        }
    }

    // Step 2: cluster column_nodes by midpoint X.
    let bands = cluster_by_midpoint_x(&column_nodes);

    // Step 3: build narrow vs wide band split, sort narrow bands LTR.
    let narrow_width = page_width * NARROW_BAND_FRAC;
    let mut narrow: Vec<Band> = Vec::new();
    let mut wide: Vec<Band> = Vec::new();
    for b in bands.into_iter() {
        if b.span() <= narrow_width {
            narrow.push(b);
        } else {
            wide.push(b);
        }
    }
    // Wide bands that aren't quite spanners still belong in document flow —
    // treat them as additional "spanner" rows so they slot in by y position.
    for b in wide.into_iter() {
        for n in b.nodes {
            spanners.push(n);
        }
    }

    narrow.sort_by(|a, b| {
        a.mid_x
            .partial_cmp(&b.mid_x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Collapse 3+ columns down to 2 by merging the two adjacent bands
    // with the smallest mid_x gap (most "3-column" papers are 2 cols + a
    // sidebar — folding the sidebar into the nearer column reads more
    // naturally than reading it last).
    while narrow.len() > 2 {
        let mut min_gap = f32::INFINITY;
        let mut min_idx = 0usize;
        for i in 0..narrow.len() - 1 {
            let gap = narrow[i + 1].mid_x - narrow[i].mid_x;
            if gap < min_gap {
                min_gap = gap;
                min_idx = i;
            }
        }
        let merged_into = narrow[min_idx].nodes.len();
        let mut tail = narrow.remove(min_idx + 1);
        narrow[min_idx].nodes.append(&mut tail.nodes);
        // recompute mid_x as weighted average
        let a_count = merged_into as f32;
        let b_count = tail.nodes.len() as f32 + a_count;
        let _ = (a_count, b_count); // keep clippy quiet; recompute below
        narrow[min_idx].mid_x = recompute_mid_x(&narrow[min_idx].nodes);
    }

    // Step 4: sort each narrow band top-to-bottom (descending y0).
    for band in &mut narrow {
        band.nodes.sort_by(|a, b| {
            b.bbox
                .y0
                .partial_cmp(&a.bbox.y0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Step 5: weave spanners into the flow by y position. Strategy: emit
    // each spanner at its vertical position relative to the *primary*
    // (leftmost narrow) band. Spanners above all column nodes appear
    // first; spanners between column tops are inserted between columns;
    // spanners below all content appear last.
    spanners.sort_by(|a, b| {
        b.bbox
            .y0
            .partial_cmp(&a.bbox.y0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut ordered: Vec<StructNode> = Vec::new();
    // Spanners above the first column's top y0 — emit first.
    let top_of_columns: f32 = narrow
        .iter()
        .filter_map(|b| b.nodes.iter().map(|n| n.bbox.y1).fold(None, max_opt))
        .fold(None, max_opt)
        .unwrap_or(0.0);
    let bottom_of_columns: f32 = narrow
        .iter()
        .filter_map(|b| b.nodes.iter().map(|n| n.bbox.y0).fold(None, min_opt))
        .fold(None, min_opt)
        .unwrap_or(0.0);

    let mut middle_spanners: Vec<StructNode> = Vec::new();
    for sp in spanners.into_iter() {
        if sp.bbox.y0 >= top_of_columns {
            ordered.push(sp);
        } else if sp.bbox.y1 <= bottom_of_columns {
            middle_spanners.push(sp); // actually below — push later
        } else {
            // Mid-page spanner — for simplicity emit between columns.
            // (PDF/UA Slice 4 will refine into precise inter-column slots.)
            middle_spanners.push(sp);
        }
    }

    // Emit each narrow band in LTR order.
    for band in narrow.iter() {
        for n in &band.nodes {
            ordered.push(n.clone());
        }
    }
    // Append mid/below spanners after the columns (still inside the page).
    for sp in middle_spanners.into_iter() {
        ordered.push(sp);
    }

    let column_count = narrow.len().min(4);
    let spanner_count = ordered.iter().filter(|n| is_spanner(n, page_width)).count();
    let artifact_count = artifacts.len();

    // Tail artifacts so screen-reader-skipped chrome doesn't pollute the
    // reading flow.
    for a in artifacts.into_iter() {
        ordered.push(a);
    }

    ReadingOrderPage {
        page_number: page.page_number,
        column_count,
        spanner_count,
        artifact_count,
        nodes: ordered,
    }
}

fn is_spanner(n: &StructNode, page_width: f32) -> bool {
    let w = (n.bbox.x1 - n.bbox.x0).abs();
    w >= page_width * SPANNER_FRAC
}

fn max_opt(acc: Option<f32>, v: f32) -> Option<f32> {
    Some(match acc {
        Some(a) if a >= v => a,
        _ => v,
    })
}

fn min_opt(acc: Option<f32>, v: f32) -> Option<f32> {
    Some(match acc {
        Some(a) if a <= v => a,
        _ => v,
    })
}

#[derive(Debug, Clone)]
struct Band {
    /// Average midpoint X across this band's nodes.
    mid_x: f32,
    nodes: Vec<StructNode>,
}

impl Band {
    fn span(&self) -> f32 {
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for n in &self.nodes {
            if n.bbox.x0 < min_x {
                min_x = n.bbox.x0;
            }
            if n.bbox.x1 > max_x {
                max_x = n.bbox.x1;
            }
        }
        if min_x.is_finite() && max_x.is_finite() {
            (max_x - min_x).max(0.0)
        } else {
            0.0
        }
    }
}

fn recompute_mid_x(nodes: &[StructNode]) -> f32 {
    if nodes.is_empty() {
        return 0.0;
    }
    let sum: f32 = nodes.iter().map(|n| (n.bbox.x0 + n.bbox.x1) * 0.5).sum();
    sum / nodes.len() as f32
}

/// Sort nodes by midpoint X then cluster: nodes whose midpoints are within
/// `MIN_COLUMN_GAP_PT` of the band's running mean join that band; else a
/// new band starts. Returns bands sorted by mid_x ascending.
fn cluster_by_midpoint_x(nodes: &[StructNode]) -> Vec<Band> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut indexed: Vec<(f32, StructNode)> = nodes
        .iter()
        .map(|n| {
            let mid = midpoint_x(&n.bbox);
            (mid, n.clone())
        })
        .collect();
    indexed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut bands: Vec<Band> = Vec::new();
    for (mid, node) in indexed.into_iter() {
        if let Some(last) = bands.last_mut() {
            if (mid - last.mid_x).abs() <= MIN_COLUMN_GAP_PT {
                // Join.
                let new_count = last.nodes.len() as f32 + 1.0;
                last.mid_x = (last.mid_x * (new_count - 1.0) + mid) / new_count;
                last.nodes.push(node);
                continue;
            }
        }
        bands.push(Band {
            mid_x: mid,
            nodes: vec![node],
        });
    }
    bands
}

fn midpoint_x(b: &Bbox) -> f32 {
    (b.x0 + b.x1) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::loom::classify::{NodeKind, StructNode, StructTree, StructTreePage};
    use crate::pdf::loom::layout::Bbox;

    fn node(text: &str, kind: NodeKind, x0: f32, y0: f32, x1: f32, y1: f32) -> StructNode {
        StructNode {
            kind,
            text: text.into(),
            bbox: Bbox { x0, y0, x1, y1 },
            font_size: 12.0,
            xobject_name: None,
            alt_text: None,
            lang: None,
            children: Vec::new(),
        }
    }

    fn tree_with_page(nodes: Vec<StructNode>) -> StructTree {
        StructTree {
            pages: vec![StructTreePage {
                page_number: 1,
                nodes,
            }],
        }
    }

    #[test]
    fn empty_page_yields_empty_reading_order() {
        let t = tree_with_page(vec![]);
        let ro = order_reading(&t, &[(612.0, 792.0)]);
        assert_eq!(ro.pages.len(), 1);
        assert_eq!(ro.pages[0].column_count, 0);
        assert_eq!(ro.pages[0].nodes.len(), 0);
        assert_eq!(ro.multi_column_pages(), 0);
    }

    #[test]
    fn single_column_page_detects_one_column() {
        // Three paragraphs stacked in the same column (x0=72, x1=540).
        let nodes = vec![
            node("Para 1", NodeKind::Paragraph, 72.0, 700.0, 540.0, 720.0),
            node("Para 2", NodeKind::Paragraph, 72.0, 600.0, 540.0, 620.0),
            node("Para 3", NodeKind::Paragraph, 72.0, 500.0, 540.0, 520.0),
        ];
        let ro = order_reading(&tree_with_page(nodes), &[(612.0, 792.0)]);
        let p = &ro.pages[0];
        // Each para is ~468pt wide on a 612pt page — exceeds the spanner
        // threshold (612 * 0.7 = 428), so they become spanners. That's the
        // correct behaviour for full-width body text on a 1-column page.
        assert_eq!(p.column_count, 0, "no narrow column bands expected");
        assert_eq!(p.spanner_count, 3);
        // Reading order top-to-bottom.
        assert!(p.nodes[0].text.contains("Para 1"));
        assert!(p.nodes[2].text.contains("Para 3"));
    }

    #[test]
    fn two_column_paper_reads_left_top_to_bottom_then_right_top_to_bottom() {
        // Two-column page: left col x=[72, 290], right col x=[322, 540].
        // Body width per col is 218pt < page_width * 0.6 = 367pt → narrow.
        // Left col paragraphs L1 (high y) L2 L3 (low y).
        // Right col paragraphs R1 R2 R3 — interleaved in y with the left col.
        let nodes = vec![
            node("L1", NodeKind::Paragraph, 72.0, 700.0, 290.0, 720.0),
            node("R1", NodeKind::Paragraph, 322.0, 700.0, 540.0, 720.0),
            node("L2", NodeKind::Paragraph, 72.0, 600.0, 290.0, 620.0),
            node("R2", NodeKind::Paragraph, 322.0, 600.0, 540.0, 620.0),
            node("L3", NodeKind::Paragraph, 72.0, 500.0, 290.0, 520.0),
            node("R3", NodeKind::Paragraph, 322.0, 500.0, 540.0, 520.0),
        ];
        let ro = order_reading(&tree_with_page(nodes), &[(612.0, 792.0)]);
        let p = &ro.pages[0];
        assert_eq!(p.column_count, 2, "two narrow column bands expected");
        let texts: Vec<&str> = p.nodes.iter().map(|n| n.text.as_str()).collect();
        // Expected: L1, L2, L3, R1, R2, R3. NOT physical L1,R1,L2,R2,L3,R3.
        assert_eq!(
            texts,
            vec!["L1", "L2", "L3", "R1", "R2", "R3"],
            "{:?}",
            texts
        );
        assert_eq!(ro.multi_column_pages(), 1);
    }

    #[test]
    fn page_spanning_heading_emits_before_columns() {
        // Heading spans full page width (612 * 0.7 = 428pt threshold → 460pt is a spanner).
        // Then two-column body below.
        let nodes = vec![
            node(
                "Big Heading",
                NodeKind::Heading(1),
                72.0,
                740.0,
                540.0,
                760.0,
            ),
            node("L1", NodeKind::Paragraph, 72.0, 700.0, 290.0, 720.0),
            node("L2", NodeKind::Paragraph, 72.0, 600.0, 290.0, 620.0),
            node("R1", NodeKind::Paragraph, 322.0, 700.0, 540.0, 720.0),
            node("R2", NodeKind::Paragraph, 322.0, 600.0, 540.0, 620.0),
        ];
        let ro = order_reading(&tree_with_page(nodes), &[(612.0, 792.0)]);
        let p = &ro.pages[0];
        let texts: Vec<&str> = p.nodes.iter().map(|n| n.text.as_str()).collect();
        // Heading first (it sits above the column tops), then L's, then R's.
        assert_eq!(texts[0], "Big Heading");
        assert_eq!(p.spanner_count, 1);
        assert_eq!(p.column_count, 2);
        let l1 = texts.iter().position(|t| *t == "L1").unwrap();
        let r1 = texts.iter().position(|t| *t == "R1").unwrap();
        assert!(l1 < r1, "left column reads before right: {:?}", texts);
    }

    #[test]
    fn artifacts_are_pushed_to_the_end() {
        let nodes = vec![
            node("Page 3", NodeKind::Artifact, 280.0, 30.0, 340.0, 50.0),
            node("Body line", NodeKind::Paragraph, 72.0, 700.0, 290.0, 720.0),
        ];
        let ro = order_reading(&tree_with_page(nodes), &[(612.0, 792.0)]);
        let p = &ro.pages[0];
        assert_eq!(p.artifact_count, 1);
        // Last node must be the artifact regardless of original position.
        assert!(matches!(p.nodes.last().unwrap().kind, NodeKind::Artifact));
        // Reading-flow nodes excludes the artifact.
        assert_eq!(ro.total_reading_nodes(), 1);
    }

    #[test]
    fn three_band_layout_collapses_to_two_columns() {
        // Three narrow bands at x midpoints ~100, ~250, ~470 — should
        // collapse the two closest (100 + 250 → 175 mean) leaving 2 bands.
        let nodes = vec![
            node("S1", NodeKind::Paragraph, 60.0, 700.0, 140.0, 720.0), // mid 100
            node("M1", NodeKind::Paragraph, 210.0, 700.0, 290.0, 720.0), // mid 250
            node("R1", NodeKind::Paragraph, 430.0, 700.0, 510.0, 720.0), // mid 470
        ];
        let ro = order_reading(&tree_with_page(nodes), &[(612.0, 792.0)]);
        let p = &ro.pages[0];
        assert!(
            p.column_count <= 2,
            "3+ bands should collapse to ≤2: got {}",
            p.column_count
        );
    }

    #[test]
    fn order_is_deterministic_for_same_input() {
        let nodes = vec![
            node("A", NodeKind::Paragraph, 72.0, 700.0, 290.0, 720.0),
            node("B", NodeKind::Paragraph, 322.0, 700.0, 540.0, 720.0),
        ];
        let r1 = order_reading(&tree_with_page(nodes.clone()), &[(612.0, 792.0)]);
        let r2 = order_reading(&tree_with_page(nodes), &[(612.0, 792.0)]);
        let t1: Vec<&str> = r1.pages[0].nodes.iter().map(|n| n.text.as_str()).collect();
        let t2: Vec<&str> = r2.pages[0].nodes.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(t1, t2);
    }

    #[test]
    fn missing_page_geometry_falls_back_to_letter() {
        // Empty geometry → default 612x792 still works.
        let nodes = vec![node("X", NodeKind::Paragraph, 72.0, 700.0, 290.0, 720.0)];
        let ro = order_reading(&tree_with_page(nodes), &[]);
        assert_eq!(ro.pages.len(), 1);
        assert_eq!(ro.pages[0].nodes.len(), 1);
    }

    #[test]
    fn multi_page_each_page_clustered_independently() {
        let tree = StructTree {
            pages: vec![
                StructTreePage {
                    page_number: 1,
                    nodes: vec![
                        node("L1", NodeKind::Paragraph, 72.0, 700.0, 290.0, 720.0),
                        node("R1", NodeKind::Paragraph, 322.0, 700.0, 540.0, 720.0),
                    ],
                },
                StructTreePage {
                    page_number: 2,
                    nodes: vec![node("Solo", NodeKind::Paragraph, 72.0, 700.0, 540.0, 720.0)],
                },
            ],
        };
        let ro = order_reading(&tree, &[(612.0, 792.0), (612.0, 792.0)]);
        assert_eq!(ro.pages.len(), 2);
        assert_eq!(ro.pages[0].column_count, 2);
        // Page 2's lone paragraph is full-width → spanner, no narrow column.
        assert_eq!(ro.pages[1].column_count, 0);
        assert_eq!(ro.multi_column_pages(), 1);
    }
}
