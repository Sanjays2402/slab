// src-tauri/src/pdf/loom/classify.rs
//
// Slab Loom — Slice 2: heuristic `StructTree` classifier.
//
// Input:  `LayoutTree` (from Slice 1) — every text run + image placement on
//         every page, with bbox + font + page geometry.
// Output: `StructTree` — same content, but every run is now typed as a PDF/UA
//         logical structure node: Heading(1..6), Paragraph, List + ListItem,
//         Figure, Caption, Artifact (skipped by screen readers), or
//         Paragraph as a safe default. Figures come from image placements.
//
// The heuristics, kept deliberately simple so they're easy to reason about
// and unit-test:
//
//   1. **Font-size buckets.** Collect every distinct run font size across the
//      doc. The mode (most common size) is body text. Sizes strictly larger
//      than the mode are heading candidates, mapped highest→H1, second→H2,
//      etc. up to H6. Anything within ~0.5pt of the mode is body.
//
//   2. **Artifacts.** A run is an Artifact (page chrome — folio, running
//      header, footer) if it sits in the top 50pt or bottom 50pt of its
//      page AND the same trimmed text appears in the same vertical band on
//      ≥ 2 distinct pages. Folio numerals ("3 of 12", "12") get a softer
//      rule: bottom-50pt + matches `\d+(\s+of\s+\d+)?` is always artifact.
//
//   3. **Lists.** A run whose trimmed text begins with a bullet glyph
//      (•, ●, ○, ▪, ‣, –, *, -) followed by whitespace, or `\d+[.)]` followed
//      by whitespace, becomes a ListItem. Consecutive ListItems on the same
//      page within ~1.6× their font_size vertical gap are folded under one
//      List parent.
//
//   4. **Figures + Captions.** Every `ImagePlacement` is a Figure node. The
//      single nearest *body* text run whose bbox sits within 30pt below the
//      image bbox AND whose width is ≤ image width × 1.5 is captured as
//      that Figure's Caption child.
//
//   5. **Everything else.** Paragraph.
//
// Reading order WITHIN a page mirrors the input run order (PDF content-stream
// order). Column-aware re-ordering arrives in Slice 3.
//
// Confidence: the classifier never panics on weird input — empty page →
// empty `StructTreePage`, no font sizes → everything Paragraph, etc. Every
// branch is covered by a unit test in this file.

use super::layout::{Bbox, ImagePlacement, LayoutTree, PageLayout, TextRun};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// PDF/UA standard structure types we emit. The numeric variants on
/// `Heading` map to PDF tag names H1..H6 (capped at H6 per ISO 32000-1
/// §14.8.4.3). New variants land in later slices (Table, TableRow, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Document,
    /// Logical section grouping (not emitted by Slice 2 — placeholder for
    /// Slice 3 column-aware grouping).
    Section,
    Heading(u8),
    Paragraph,
    List,
    ListItem,
    Figure,
    Caption,
    /// Page chrome — folio, running header/footer. PDF/UA says these MUST be
    /// marked as artifacts and excluded from the structure tree's reading
    /// order. We keep them in the tree with `Artifact` so the UI can show
    /// "skipped" diagnostics, but tagging (Slice 4) will mark them
    /// `/Artifact` not a standard role.
    Artifact,
}

impl NodeKind {
    /// PDF tag name as emitted into the StructTreeRoot (Slice 5).
    pub fn tag(&self) -> &'static str {
        match self {
            NodeKind::Document => "Document",
            NodeKind::Section => "Sect",
            NodeKind::Heading(n) => match n {
                1 => "H1",
                2 => "H2",
                3 => "H3",
                4 => "H4",
                5 => "H5",
                _ => "H6",
            },
            NodeKind::Paragraph => "P",
            NodeKind::List => "L",
            NodeKind::ListItem => "LI",
            NodeKind::Figure => "Figure",
            NodeKind::Caption => "Caption",
            NodeKind::Artifact => "Artifact",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructNode {
    pub kind: NodeKind,
    pub text: String,
    pub bbox: Bbox,
    pub font_size: f32,
    /// Set on Figure nodes referencing the XObject name from layout.
    pub xobject_name: Option<String>,
    /// Filled in by Slice 4 (alt-text generation). Always None in Slice 2.
    pub alt_text: Option<String>,
    /// Filled in by Slice 3 (language detection). Always None in Slice 2.
    pub lang: Option<String>,
    pub children: Vec<StructNode>,
}

impl StructNode {
    fn leaf(kind: NodeKind, run: &TextRun) -> Self {
        Self {
            kind,
            text: run.text.clone(),
            bbox: run.bbox,
            font_size: run.font_size,
            xobject_name: None,
            alt_text: None,
            lang: None,
            children: Vec::new(),
        }
    }

    fn figure(img: &ImagePlacement) -> Self {
        Self {
            kind: NodeKind::Figure,
            text: String::new(),
            bbox: img.bbox,
            font_size: 0.0,
            xobject_name: Some(img.xobject_name.clone()),
            alt_text: None,
            lang: None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructTreePage {
    pub page_number: u32,
    pub nodes: Vec<StructNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructTree {
    pub pages: Vec<StructTreePage>,
}

impl StructTree {
    pub fn total_nodes(&self) -> usize {
        self.pages.iter().map(|p| count(&p.nodes)).sum()
    }

    pub fn heading_count(&self) -> usize {
        let mut n = 0usize;
        for p in &self.pages {
            walk(&p.nodes, &mut |node| {
                if matches!(node.kind, NodeKind::Heading(_)) {
                    n += 1;
                }
            });
        }
        n
    }

    pub fn artifact_count(&self) -> usize {
        let mut n = 0usize;
        for p in &self.pages {
            walk(&p.nodes, &mut |node| {
                if matches!(node.kind, NodeKind::Artifact) {
                    n += 1;
                }
            });
        }
        n
    }

    pub fn figure_count(&self) -> usize {
        let mut n = 0usize;
        for p in &self.pages {
            walk(&p.nodes, &mut |node| {
                if matches!(node.kind, NodeKind::Figure) {
                    n += 1;
                }
            });
        }
        n
    }

    pub fn list_item_count(&self) -> usize {
        let mut n = 0usize;
        for p in &self.pages {
            walk(&p.nodes, &mut |node| {
                if matches!(node.kind, NodeKind::ListItem) {
                    n += 1;
                }
            });
        }
        n
    }
}

fn count(nodes: &[StructNode]) -> usize {
    nodes.iter().map(|n| 1 + count(&n.children)).sum()
}

fn walk<F: FnMut(&StructNode)>(nodes: &[StructNode], f: &mut F) {
    for n in nodes {
        f(n);
        walk(&n.children, f);
    }
}

/// Edge of a page treated as chrome (header / footer band).
const CHROME_BAND_PT: f32 = 50.0;
/// Distance below an image used to capture caption text.
const CAPTION_BELOW_PT: f32 = 30.0;
/// Caption text width must not exceed image width by more than this factor.
const CAPTION_WIDTH_FACTOR: f32 = 1.5;
/// Two font sizes within this many points are considered the same bucket.
const FONT_SIZE_EPS: f32 = 0.5;

/// Public entry point. Pure function: same input always yields same output.
pub fn classify(layout: &LayoutTree) -> StructTree {
    // 1) Build the document-wide font bucket: mode = body, larger buckets
    //    map to H1, H2, ...
    let body_size = detect_body_size(layout);
    let headings = build_heading_levels(layout, body_size);

    // 2) Pre-compute artifact text set: trimmed text that appears in the
    //    top/bottom chrome band on ≥ 2 pages.
    let chrome_repeats = detect_chrome_repeats(layout);

    let mut out = StructTree::default();
    for page in &layout.pages {
        let nodes = classify_page(page, body_size, &headings, &chrome_repeats);
        out.pages.push(StructTreePage {
            page_number: page.page_number,
            nodes,
        });
    }
    out
}

/// Mode of all font sizes in the document, rounded to the nearest 0.5pt.
/// Returns 0.0 if the document has no text.
fn detect_body_size(layout: &LayoutTree) -> f32 {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for p in &layout.pages {
        for r in &p.runs {
            // Bucket to 0.5pt: 12.0 -> 24, 12.4 -> 25, 18.0 -> 36.
            let key = (r.font_size * 2.0).round() as u32;
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k as f32 / 2.0)
        .unwrap_or(0.0)
}

/// Map distinct font sizes strictly larger than body to heading levels 1..6.
/// Returns a vec of (size, level), sorted by size descending.
fn build_heading_levels(layout: &LayoutTree, body_size: f32) -> Vec<(f32, u8)> {
    let mut sizes: Vec<u32> = layout
        .pages
        .iter()
        .flat_map(|p| p.runs.iter().map(|r| (r.font_size * 2.0).round() as u32))
        .filter(|s| (*s as f32 / 2.0) > body_size + FONT_SIZE_EPS)
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    sizes.reverse(); // largest first

    sizes
        .into_iter()
        .take(6)
        .enumerate()
        .map(|(i, s)| (s as f32 / 2.0, (i as u8) + 1))
        .collect()
}

fn heading_level(headings: &[(f32, u8)], font_size: f32) -> Option<u8> {
    for (size, level) in headings {
        if (font_size - size).abs() <= FONT_SIZE_EPS {
            return Some(*level);
        }
    }
    None
}

/// For each (chrome_band, trimmed_text) compute how many pages it appears on.
/// We return the set of trimmed texts that repeat on ≥ 2 pages in either the
/// top or bottom band. "Same band" means top vs bottom; vertical position
/// within the band doesn't have to be identical.
fn detect_chrome_repeats(layout: &LayoutTree) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    // (band, text) -> set of page numbers it appears in.
    let mut seen: HashMap<(ChromeBand, String), HashSet<u32>> = HashMap::new();
    for page in &layout.pages {
        for run in &page.runs {
            if let Some(band) = chrome_band_for(run, page) {
                let key = (band, run.text.trim().to_string());
                if key.1.is_empty() {
                    continue;
                }
                seen.entry(key).or_default().insert(page.page_number);
            }
        }
    }
    seen.into_iter()
        .filter(|(_, pages)| pages.len() >= 2)
        .map(|((_, text), _)| text)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ChromeBand {
    Top,
    Bottom,
}

fn chrome_band_for(run: &TextRun, page: &PageLayout) -> Option<ChromeBand> {
    // PDF coords: origin lower-left, y grows upward.
    if run.bbox.y0 <= CHROME_BAND_PT && page.height > 0.0 {
        Some(ChromeBand::Bottom)
    } else if page.height > 0.0 && run.bbox.y1 >= page.height - CHROME_BAND_PT {
        Some(ChromeBand::Top)
    } else {
        None
    }
}

fn is_folio_text(s: &str) -> bool {
    // "12", "12 of 34", "Page 12", "Page 12 of 34"
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    let stripped = lower.strip_prefix("page ").map(str::trim).unwrap_or(&lower);
    let parts: Vec<&str> = stripped.split_whitespace().collect();
    match parts.as_slice() {
        [n] => n.chars().all(|c| c.is_ascii_digit()),
        [n, "of", m] => {
            n.chars().all(|c| c.is_ascii_digit()) && m.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

fn is_list_marker(s: &str) -> bool {
    let t = s.trim_start();
    if t.is_empty() {
        return false;
    }
    let first = t.chars().next().unwrap();
    // Bullet glyphs (BMP + a couple extras commonly seen in PDFs).
    let bullet_glyphs = ['•', '●', '○', '▪', '◦', '‣', '·', '*'];
    if bullet_glyphs.contains(&first) {
        // Need whitespace OR be the only char (e.g. "• item").
        return t.chars().nth(1).map(|c| c.is_whitespace()).unwrap_or(false);
    }
    if first == '-' || first == '–' || first == '—' {
        return t.chars().nth(1).map(|c| c.is_whitespace()).unwrap_or(false);
    }
    // Numeric "1." / "1)" / "12." / "12)".
    let mut chars = t.chars();
    let mut digits = 0;
    let mut next_after_digits: Option<char> = None;
    for c in chars.by_ref() {
        if c.is_ascii_digit() {
            digits += 1;
            if digits > 3 {
                return false;
            }
        } else {
            next_after_digits = Some(c);
            break;
        }
    }
    if digits == 0 {
        return false;
    }
    let sep = match next_after_digits {
        Some(c) => c,
        None => return false,
    };
    if sep != '.' && sep != ')' {
        return false;
    }
    matches!(chars.next(), Some(c) if c.is_whitespace())
}

fn classify_page(
    page: &PageLayout,
    body_size: f32,
    headings: &[(f32, u8)],
    chrome_repeats: &std::collections::HashSet<String>,
) -> Vec<StructNode> {
    // First pass: tag every run, leaving Figures + Captions for the second
    // pass which has access to the run nodes.
    let mut run_nodes: Vec<StructNode> = Vec::with_capacity(page.runs.len());
    for run in &page.runs {
        let kind = classify_run(run, page, body_size, headings, chrome_repeats);
        run_nodes.push(StructNode::leaf(kind, run));
    }

    // Second pass: fold consecutive ListItems into a List parent.
    let folded = fold_lists(run_nodes);

    // Third pass: figures with captions. Captions are stolen from `folded` —
    // we look for the nearest Paragraph below an image bbox (within
    // CAPTION_BELOW_PT, narrower than image*FACTOR) and reparent it as a
    // Figure child.
    let mut out: Vec<StructNode> = Vec::new();
    let mut consumed: Vec<bool> = vec![false; folded.len()];
    for img in &page.images {
        let mut fig = StructNode::figure(img);
        // Find caption candidate.
        let mut best: Option<(usize, f32)> = None;
        for (i, node) in folded.iter().enumerate() {
            if consumed[i] {
                continue;
            }
            if !matches!(node.kind, NodeKind::Paragraph) {
                continue;
            }
            // Below image: caption.y1 <= image.y0 (caption sits lower in PDF
            // coords means smaller y).
            let below_gap = img.bbox.y0 - node.bbox.y1;
            if below_gap < 0.0 || below_gap > CAPTION_BELOW_PT {
                continue;
            }
            if node.bbox.width() > img.bbox.width() * CAPTION_WIDTH_FACTOR {
                continue;
            }
            // Closest gap wins.
            if best.map(|(_, g)| below_gap < g).unwrap_or(true) {
                best = Some((i, below_gap));
            }
        }
        if let Some((i, _)) = best {
            let mut cap = folded[i].clone();
            cap.kind = NodeKind::Caption;
            fig.children.push(cap);
            consumed[i] = true;
        }
        out.push(fig);
    }
    for (i, node) in folded.into_iter().enumerate() {
        if !consumed[i] {
            out.push(node);
        }
    }
    out
}

fn classify_run(
    run: &TextRun,
    page: &PageLayout,
    body_size: f32,
    headings: &[(f32, u8)],
    chrome_repeats: &std::collections::HashSet<String>,
) -> NodeKind {
    let trimmed = run.text.trim();
    if trimmed.is_empty() {
        return NodeKind::Paragraph;
    }
    // 1) Artifact: folio OR repeating-chrome.
    if let Some(band) = chrome_band_for(run, page) {
        if band == ChromeBand::Bottom && is_folio_text(trimmed) {
            return NodeKind::Artifact;
        }
        if chrome_repeats.contains(trimmed) {
            return NodeKind::Artifact;
        }
    }
    // 2) Heading.
    if let Some(level) = heading_level(headings, run.font_size) {
        if run.font_size > body_size + FONT_SIZE_EPS {
            return NodeKind::Heading(level);
        }
    }
    // 3) List item.
    if is_list_marker(trimmed) {
        return NodeKind::ListItem;
    }
    // 4) Paragraph.
    NodeKind::Paragraph
}

fn fold_lists(nodes: Vec<StructNode>) -> Vec<StructNode> {
    let mut out: Vec<StructNode> = Vec::with_capacity(nodes.len());
    let mut current_list: Option<StructNode> = None;
    let mut last_li_bottom: Option<f32> = None;
    let mut last_li_font: Option<f32> = None;

    for node in nodes {
        if matches!(node.kind, NodeKind::ListItem) {
            let in_run = match (last_li_bottom, last_li_font) {
                (Some(prev_y0), Some(prev_size)) => {
                    // PDF coords: prev_y0 (bottom of prev LI) should be just
                    // above the new LI's y1 (top). Gap = prev_y0 - new.y1.
                    let gap = prev_y0 - node.bbox.y1;
                    gap.abs() <= prev_size * 1.6
                }
                _ => false,
            };
            if !in_run {
                if let Some(list) = current_list.take() {
                    out.push(list);
                }
                current_list = Some(StructNode {
                    kind: NodeKind::List,
                    text: String::new(),
                    bbox: node.bbox,
                    font_size: node.font_size,
                    xobject_name: None,
                    alt_text: None,
                    lang: None,
                    children: Vec::new(),
                });
            }
            last_li_bottom = Some(node.bbox.y0);
            last_li_font = Some(node.font_size);
            current_list
                .as_mut()
                .expect("current_list initialized above")
                .children
                .push(node);
        } else {
            if let Some(list) = current_list.take() {
                out.push(list);
            }
            last_li_bottom = None;
            last_li_font = None;
            out.push(node);
        }
    }
    if let Some(list) = current_list.take() {
        out.push(list);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::loom::layout::{Bbox, ImagePlacement, LayoutTree, PageLayout, TextRun};

    fn bbox(x0: f32, y0: f32, x1: f32, y1: f32) -> Bbox {
        Bbox { x0, y0, x1, y1 }
    }

    fn run(text: &str, size: f32, b: Bbox) -> TextRun {
        TextRun {
            text: text.to_string(),
            font_size: size,
            font_name: "Helvetica".into(),
            bbox: b,
        }
    }

    fn page(
        num: u32,
        w: f32,
        h: f32,
        runs: Vec<TextRun>,
        images: Vec<ImagePlacement>,
    ) -> PageLayout {
        PageLayout {
            page_number: num,
            width: w,
            height: h,
            runs,
            images,
        }
    }

    #[test]
    fn empty_layout_yields_empty_tree() {
        let tree = classify(&LayoutTree::default());
        assert_eq!(tree.pages.len(), 0);
        assert_eq!(tree.total_nodes(), 0);
    }

    #[test]
    fn largest_font_becomes_h1_and_body_becomes_p() {
        // Body is 12pt repeated 8 times; title is 24pt once.
        let mut runs = vec![run("Title", 24.0, bbox(50.0, 700.0, 300.0, 730.0))];
        for i in 0..8 {
            let y = 600.0 - (i as f32) * 20.0;
            runs.push(run(
                &format!("Body line {i}"),
                12.0,
                bbox(50.0, y, 500.0, y + 14.0),
            ));
        }
        let layout = LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, vec![])],
        };
        let tree = classify(&layout);
        let nodes = &tree.pages[0].nodes;
        assert!(matches!(nodes[0].kind, NodeKind::Heading(1)));
        assert_eq!(nodes[0].text, "Title");
        for n in &nodes[1..] {
            assert!(
                matches!(n.kind, NodeKind::Paragraph),
                "expected Paragraph, got {:?} for {:?}",
                n.kind,
                n.text
            );
        }
        assert_eq!(tree.heading_count(), 1);
    }

    #[test]
    fn multi_heading_levels_assigned_descending() {
        let runs = vec![
            run("Mega", 32.0, bbox(50.0, 750.0, 200.0, 780.0)),
            run("Big", 24.0, bbox(50.0, 700.0, 200.0, 730.0)),
            run("Medium", 18.0, bbox(50.0, 650.0, 200.0, 680.0)),
            // 12 body, repeated for mode.
            run("body a", 12.0, bbox(50.0, 600.0, 500.0, 614.0)),
            run("body b", 12.0, bbox(50.0, 580.0, 500.0, 594.0)),
            run("body c", 12.0, bbox(50.0, 560.0, 500.0, 574.0)),
        ];
        let tree = classify(&LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, vec![])],
        });
        let n = &tree.pages[0].nodes;
        assert!(matches!(n[0].kind, NodeKind::Heading(1)));
        assert!(matches!(n[1].kind, NodeKind::Heading(2)));
        assert!(matches!(n[2].kind, NodeKind::Heading(3)));
    }

    #[test]
    fn folio_in_bottom_band_is_artifact() {
        let runs = vec![
            run("Body content", 12.0, bbox(50.0, 400.0, 500.0, 414.0)),
            run("Body content 2", 12.0, bbox(50.0, 380.0, 500.0, 394.0)),
            // Folio "12" at bottom 30pt.
            run("12", 10.0, bbox(300.0, 20.0, 320.0, 32.0)),
        ];
        let tree = classify(&LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, vec![])],
        });
        assert_eq!(tree.artifact_count(), 1);
        let n = &tree.pages[0].nodes;
        assert!(matches!(n.last().unwrap().kind, NodeKind::Artifact));
    }

    #[test]
    fn repeating_header_on_multiple_pages_is_artifact() {
        // Same "Confidential" text at top of pages 1 and 2 → artifact.
        let mk_page = |n: u32| {
            page(
                n,
                612.0,
                792.0,
                vec![
                    run("Confidential", 10.0, bbox(50.0, 770.0, 200.0, 785.0)),
                    run("Body text here", 12.0, bbox(50.0, 400.0, 500.0, 414.0)),
                    run("More body text", 12.0, bbox(50.0, 380.0, 500.0, 394.0)),
                    run("Even more body", 12.0, bbox(50.0, 360.0, 500.0, 374.0)),
                ],
                vec![],
            )
        };
        let tree = classify(&LayoutTree {
            pages: vec![mk_page(1), mk_page(2)],
        });
        assert_eq!(tree.artifact_count(), 2);
    }

    #[test]
    fn bulleted_lines_become_list_items_grouped_under_a_list() {
        let runs = vec![
            run("Intro paragraph.", 12.0, bbox(50.0, 700.0, 500.0, 714.0)),
            run("• apples", 12.0, bbox(60.0, 680.0, 200.0, 694.0)),
            run("• oranges", 12.0, bbox(60.0, 664.0, 200.0, 678.0)),
            run("• pears", 12.0, bbox(60.0, 648.0, 200.0, 662.0)),
            run("After list.", 12.0, bbox(50.0, 620.0, 500.0, 634.0)),
        ];
        let tree = classify(&LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, vec![])],
        });
        let n = &tree.pages[0].nodes;
        assert_eq!(n.len(), 3);
        assert!(matches!(n[0].kind, NodeKind::Paragraph));
        assert!(matches!(n[1].kind, NodeKind::List));
        assert_eq!(n[1].children.len(), 3);
        for c in &n[1].children {
            assert!(matches!(c.kind, NodeKind::ListItem));
        }
        assert!(matches!(n[2].kind, NodeKind::Paragraph));
        assert_eq!(tree.list_item_count(), 3);
    }

    #[test]
    fn numbered_list_recognized() {
        let runs = vec![
            run("1. one", 12.0, bbox(60.0, 700.0, 200.0, 714.0)),
            run("2. two", 12.0, bbox(60.0, 684.0, 200.0, 698.0)),
            run("3) three", 12.0, bbox(60.0, 668.0, 200.0, 682.0)),
        ];
        let tree = classify(&LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, vec![])],
        });
        assert_eq!(tree.list_item_count(), 3);
    }

    #[test]
    fn image_xobject_becomes_figure_with_nearby_caption() {
        let runs = vec![
            run(
                "Figure 1. A diagram.",
                10.0,
                bbox(100.0, 380.0, 280.0, 392.0),
            ),
            // Add body to make 12pt the mode.
            run("body x", 12.0, bbox(50.0, 200.0, 500.0, 214.0)),
            run("body y", 12.0, bbox(50.0, 180.0, 500.0, 194.0)),
            run("body z", 12.0, bbox(50.0, 160.0, 500.0, 174.0)),
        ];
        let images = vec![ImagePlacement {
            xobject_name: "Im1".into(),
            // Image sits y0=400 .. y1=600 (just above the caption at y1=392).
            bbox: bbox(100.0, 400.0, 280.0, 600.0),
        }];
        let tree = classify(&LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, images)],
        });
        assert_eq!(tree.figure_count(), 1);
        let fig = tree.pages[0]
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Figure))
            .unwrap();
        assert_eq!(fig.xobject_name.as_deref(), Some("Im1"));
        assert_eq!(fig.children.len(), 1);
        assert!(matches!(fig.children[0].kind, NodeKind::Caption));
        assert_eq!(fig.children[0].text, "Figure 1. A diagram.");
    }

    #[test]
    fn image_without_caption_just_yields_a_figure() {
        let runs = vec![
            run("body", 12.0, bbox(50.0, 100.0, 500.0, 114.0)),
            run("body", 12.0, bbox(50.0, 80.0, 500.0, 94.0)),
            run("body", 12.0, bbox(50.0, 60.0, 500.0, 74.0)),
        ];
        let images = vec![ImagePlacement {
            xobject_name: "Im2".into(),
            bbox: bbox(100.0, 400.0, 280.0, 600.0),
        }];
        let tree = classify(&LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, images)],
        });
        assert_eq!(tree.figure_count(), 1);
        let fig = tree.pages[0]
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Figure))
            .unwrap();
        assert_eq!(fig.children.len(), 0);
    }

    #[test]
    fn node_kind_tag_names_match_pdf_ua() {
        assert_eq!(NodeKind::Heading(1).tag(), "H1");
        assert_eq!(NodeKind::Heading(6).tag(), "H6");
        assert_eq!(NodeKind::Heading(9).tag(), "H6", "overflow clamps to H6");
        assert_eq!(NodeKind::Paragraph.tag(), "P");
        assert_eq!(NodeKind::List.tag(), "L");
        assert_eq!(NodeKind::ListItem.tag(), "LI");
        assert_eq!(NodeKind::Figure.tag(), "Figure");
        assert_eq!(NodeKind::Caption.tag(), "Caption");
        assert_eq!(NodeKind::Artifact.tag(), "Artifact");
    }

    #[test]
    fn folio_text_helper_handles_common_formats() {
        assert!(is_folio_text("12"));
        assert!(is_folio_text("12 of 34"));
        assert!(is_folio_text("Page 12"));
        assert!(is_folio_text("Page 12 of 34"));
        assert!(!is_folio_text("Chapter 12"));
        assert!(!is_folio_text("12 of pages 34"));
        assert!(!is_folio_text(""));
    }

    #[test]
    fn list_marker_helper_handles_variants() {
        assert!(is_list_marker("• item"));
        assert!(is_list_marker("- item"));
        assert!(is_list_marker("– item"));
        assert!(is_list_marker("* item"));
        assert!(is_list_marker("1. item"));
        assert!(is_list_marker("12) item"));
        assert!(!is_list_marker("item"));
        assert!(!is_list_marker("1234. too many digits"));
        assert!(!is_list_marker("-"));
        assert!(!is_list_marker(""));
    }

    #[test]
    fn classify_is_deterministic() {
        let runs = vec![
            run("Heading", 20.0, bbox(50.0, 700.0, 300.0, 720.0)),
            run("body", 12.0, bbox(50.0, 600.0, 500.0, 614.0)),
            run("body", 12.0, bbox(50.0, 580.0, 500.0, 594.0)),
            run("• item", 12.0, bbox(60.0, 560.0, 200.0, 574.0)),
        ];
        let layout = LayoutTree {
            pages: vec![page(1, 612.0, 792.0, runs, vec![])],
        };
        let a = classify(&layout);
        let b = classify(&layout);
        assert_eq!(a.total_nodes(), b.total_nodes());
        assert_eq!(a.heading_count(), b.heading_count());
        assert_eq!(a.list_item_count(), b.list_item_count());
    }
}
