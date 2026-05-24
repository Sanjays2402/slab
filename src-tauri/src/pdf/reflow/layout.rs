// Layout reconstruction — turn a flat `Vec<TextRun>` into a structured
// `Vec<Block>` (paragraphs, headings, list items, table rows).
//
// Pipeline:
//   1. group runs into LINES by y-coordinate clustering (page-aware).
//   2. sort each line's runs left-to-right and concatenate.
//   3. group lines into PARAGRAPHS by vertical-gap + indent change.
//   4. classify each paragraph:
//        - Heading{1..3}  if median font_size >= heading_size_ratio * body_size
//        - ListItem{Bullet|Number} if it starts with a bullet/number prefix
//        - TableRow       (handled separately in the `tables` step)
//        - Body           otherwise
//
// Coordinates: PDF user space, Y grows upward. We treat each page independently.

use super::extract::FontInfo;
use super::types::{Block, ListKind, ReflowOptions, TextRun};

/// Reconstruct blocks for an entire document.
pub fn reconstruct_blocks(runs: &[TextRun], opts: &ReflowOptions) -> Vec<Block> {
    if runs.is_empty() {
        return Vec::new();
    }
    let body_size = estimate_body_font_size(runs);
    // Partition runs per page (already in page order from extract::extract_text_runs).
    let mut blocks: Vec<Block> = Vec::new();
    let mut page_buf: Vec<TextRun> = Vec::new();
    let mut current_page: u32 = runs[0].page;
    for r in runs {
        if r.page != current_page {
            let lines = cluster_lines(&page_buf);
            let paragraphs = cluster_paragraphs(&lines);
            if opts.detect_tables {
                emit_classified(&paragraphs, body_size, opts, &mut blocks);
            } else {
                emit_classified_no_tables(&paragraphs, body_size, opts, &mut blocks);
            }
            page_buf.clear();
            current_page = r.page;
        }
        page_buf.push(r.clone());
    }
    let lines = cluster_lines(&page_buf);
    let paragraphs = cluster_paragraphs(&lines);
    if opts.detect_tables {
        emit_classified(&paragraphs, body_size, opts, &mut blocks);
    } else {
        emit_classified_no_tables(&paragraphs, body_size, opts, &mut blocks);
    }
    blocks
}

/// A single physical line of text (one y-cluster) on a page.
#[derive(Debug, Clone)]
pub struct Line {
    pub y: f32,
    pub x_start: f32,
    pub runs: Vec<TextRun>,
    pub text: String,
    pub median_font_size: f32,
    pub mostly_bold: bool,
}

/// A paragraph = consecutive lines with similar indent and small vertical gap.
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub lines: Vec<Line>,
    pub x_start: f32,
    pub median_font_size: f32,
    pub mostly_bold: bool,
}

impl Paragraph {
    pub fn text(&self) -> String {
        let parts: Vec<String> = self.lines.iter().map(|l| l.text.clone()).collect();
        parts.join(" ")
    }
}

/// Cluster runs on a single page into lines.
///
/// Two runs share a line iff their baselines are within
/// `0.5 * median(font_size)`.
pub fn cluster_lines(runs: &[TextRun]) -> Vec<Line> {
    if runs.is_empty() {
        return Vec::new();
    }
    // Sort by descending y (top-to-bottom on the page), then ascending x.
    let mut sorted: Vec<TextRun> = runs.to_vec();
    sorted.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let median_size = median(&sorted.iter().map(|r| r.font_size).collect::<Vec<_>>());
    let line_tol = (median_size * 0.5).max(1.0);

    let mut lines: Vec<Line> = Vec::new();
    for r in sorted {
        let placed = lines
            .iter_mut()
            .rev()
            .find(|l| (l.y - r.y).abs() <= line_tol);
        if let Some(line) = placed {
            line.runs.push(r);
        } else {
            lines.push(Line {
                y: r.y,
                x_start: 0.0,
                runs: vec![r],
                text: String::new(),
                median_font_size: 0.0,
                mostly_bold: false,
            });
        }
    }
    for line in &mut lines {
        line.runs
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        line.x_start = line.runs.first().map(|r| r.x).unwrap_or(0.0);
        line.median_font_size = median(&line.runs.iter().map(|r| r.font_size).collect::<Vec<_>>());
        let bold_count = line.runs.iter().filter(|r| r.bold).count();
        line.mostly_bold = bold_count * 2 >= line.runs.len();
        // Recompute baseline as average y of runs (more stable than first-seen).
        let avg_y = line.runs.iter().map(|r| r.y).sum::<f32>() / line.runs.len() as f32;
        line.y = avg_y;
        line.text = build_line_text(&line.runs);
    }
    lines
}

fn build_line_text(runs: &[TextRun]) -> String {
    // Insert a single space between runs unless the previous one already
    // ended with whitespace or the next one starts with whitespace.
    let mut out = String::new();
    for (i, r) in runs.iter().enumerate() {
        if i > 0 {
            let prev_ends_ws = out
                .chars()
                .last()
                .map(|c| c.is_whitespace())
                .unwrap_or(false);
            let next_starts_ws = r
                .text
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false);
            // Heuristic spacing: insert a space if neither side has one AND
            // there's a meaningful x-gap (>= 0.25 em).
            let prev_x_end = runs[i - 1].x + estimate_advance(&runs[i - 1]);
            let gap = r.x - prev_x_end;
            if !prev_ends_ws && !next_starts_ws && gap > r.font_size * 0.25 {
                out.push(' ');
            }
        }
        out.push_str(&r.text);
    }
    out
}

fn estimate_advance(r: &TextRun) -> f32 {
    // Cheap monospace-ish estimate. Good enough for line-building gap detection.
    // Real glyph widths would come from the font's /Widths array (Task 6 polish).
    r.text.chars().count() as f32 * r.font_size * 0.5
}

/// Cluster lines into paragraphs.
///
/// Two consecutive lines join into the same paragraph iff:
///   - vertical gap (line[i-1].y - line[i].y) <= 1.5 * line[i].median_font_size
///   - left indent difference <= 0.5 * em (≈ 0.5 * font_size)
///   - font-size ratio within 1.15x (heading boundaries split paragraphs)
pub fn cluster_paragraphs(lines: &[Line]) -> Vec<Paragraph> {
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    if lines.is_empty() {
        return paragraphs;
    }
    let mut current: Vec<Line> = Vec::new();
    let mut prev: Option<&Line> = None;
    for line in lines {
        let new_paragraph = match prev {
            None => false,
            Some(p) => {
                let gap = p.y - line.y;
                let same_block_v = gap <= 1.5 * line.median_font_size.max(p.median_font_size);
                let indent_change = (line.x_start - p.x_start).abs()
                    > 0.5 * line.median_font_size.max(p.median_font_size);
                let size_ratio = (line.median_font_size / p.median_font_size.max(1e-3))
                    .max(p.median_font_size / line.median_font_size.max(1e-3));
                let size_break = size_ratio > 1.15;
                let bold_change = line.mostly_bold != p.mostly_bold;
                !same_block_v || indent_change || size_break || bold_change
            }
        };
        if new_paragraph && !current.is_empty() {
            paragraphs.push(finalize_paragraph(std::mem::take(&mut current)));
        }
        current.push(line.clone());
        prev = current.last();
    }
    if !current.is_empty() {
        paragraphs.push(finalize_paragraph(current));
    }
    paragraphs
}

fn finalize_paragraph(lines: Vec<Line>) -> Paragraph {
    let x_start = lines
        .iter()
        .map(|l| l.x_start)
        .fold(f32::INFINITY, f32::min);
    let median_font_size = median(
        &lines
            .iter()
            .flat_map(|l| l.runs.iter().map(|r| r.font_size))
            .collect::<Vec<_>>(),
    );
    let bold_lines = lines.iter().filter(|l| l.mostly_bold).count();
    let mostly_bold = bold_lines * 2 >= lines.len();
    Paragraph {
        lines,
        x_start,
        median_font_size,
        mostly_bold,
    }
}

/// Median of a slice of f32, returning 0.0 for empty.
fn median(xs: &[f32]) -> f32 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// Estimate the document's body font size: the most-common rounded font size,
/// preferring smaller (since headings are by definition outliers above body).
pub fn estimate_body_font_size(runs: &[TextRun]) -> f32 {
    if runs.is_empty() {
        return 12.0;
    }
    use std::collections::HashMap;
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for r in runs {
        let bucket = (r.font_size * 2.0).round() as u32; // 0.5pt buckets
        *counts.entry(bucket).or_insert(0) += r.text.len().max(1) as u32;
    }
    // Pick the most common bucket; tie-break by smaller font size (body, not headline).
    let (bucket, _) = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0).reverse()))
        .unwrap();
    bucket as f32 / 2.0
}

/// Classify each paragraph into a `Block`. Calls `tables::detect_table_runs`
/// to opportunistically fold consecutive aligned paragraphs into TableRows.
fn emit_classified(
    paragraphs: &[Paragraph],
    body_size: f32,
    opts: &ReflowOptions,
    out: &mut Vec<Block>,
) {
    // Two-pass: first mark indices that belong to detected tables, then emit.
    let table_spans = super::tables::detect_tables(paragraphs);
    let mut i = 0;
    while i < paragraphs.len() {
        if let Some(span) = table_spans.iter().find(|s| s.start == i) {
            // Emit one TableRow Block per paragraph in the span.
            for p in &paragraphs[span.start..span.end] {
                let row_cells = super::tables::extract_cells(p, &span.column_xs);
                out.push(Block::TableRow { cells: row_cells });
            }
            i = span.end;
            continue;
        }
        out.push(classify_paragraph(&paragraphs[i], body_size, opts));
        i += 1;
    }
}

fn emit_classified_no_tables(
    paragraphs: &[Paragraph],
    body_size: f32,
    opts: &ReflowOptions,
    out: &mut Vec<Block>,
) {
    for p in paragraphs {
        out.push(classify_paragraph(p, body_size, opts));
    }
}

pub(super) fn classify_paragraph(p: &Paragraph, body_size: f32, opts: &ReflowOptions) -> Block {
    let text = p.text();
    let trimmed = text.trim_start();

    // Heading classification first — bold OR large.
    let heading_ratio = p.median_font_size / body_size.max(1e-3);
    let is_heading_size = heading_ratio >= opts.heading_size_ratio;
    let is_heading_bold_short = p.mostly_bold && trimmed.len() <= 120 && p.lines.len() == 1;
    if is_heading_size || is_heading_bold_short {
        let level = heading_level(heading_ratio);
        return Block::Heading {
            level,
            text: trimmed.to_string(),
        };
    }

    if opts.detect_lists {
        if let Some((kind, body_text)) = detect_list_prefix(trimmed) {
            return Block::ListItem {
                kind,
                text: body_text,
                indent: 0,
            };
        }
    }

    Block::Body {
        text: trimmed.to_string(),
    }
}

fn heading_level(ratio: f32) -> u8 {
    if ratio >= 1.75 {
        1
    } else if ratio >= 1.45 {
        2
    } else {
        3
    }
}

/// Recognize common list prefixes. Returns `(kind, body_without_prefix)`.
///
/// Bullets: `•`, `·`, `‣`, `▪`, `▫`, `-` (when followed by space), `*` (followed by space).
/// Numbers: `1.`, `1)`, `(1)`, `i.`, `a.`, `A.`.
pub fn detect_list_prefix(s: &str) -> Option<(ListKind, String)> {
    let trimmed = s.trim_start();
    // Bullet glyphs.
    for bullet in ["•", "·", "‣", "▪", "▫", "◦"] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let rest = rest.trim_start();
            if !rest.is_empty() {
                return Some((ListKind::Bullet, rest.to_string()));
            }
        }
    }
    // ASCII bullets — must be followed by whitespace (avoids matching minus signs in math).
    for ch in ['-', '*', '+'] {
        let prefix: [u8; 2] = [ch as u8, b' '];
        let p = std::str::from_utf8(&prefix).unwrap();
        if let Some(rest) = trimmed.strip_prefix(p) {
            return Some((ListKind::Bullet, rest.trim_start().to_string()));
        }
    }

    // Numbered lists: 1. / 1) / (1)
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    if bytes.first().copied() == Some(b'(') {
        let mut j = 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > 1 && bytes.get(j).copied() == Some(b')') && bytes.get(j + 1).copied() == Some(b' ') {
            return Some((
                ListKind::Number,
                std::str::from_utf8(&bytes[j + 2..])
                    .ok()?
                    .trim_start()
                    .to_string(),
            ));
        }
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0
        && (bytes.get(i).copied() == Some(b'.') || bytes.get(i).copied() == Some(b')'))
        && bytes.get(i + 1).copied() == Some(b' ')
    {
        return Some((
            ListKind::Number,
            std::str::from_utf8(&bytes[i + 2..])
                .ok()?
                .trim_start()
                .to_string(),
        ));
    }

    // Letter / roman: `a.` `A.` `i.` `iv.` — keep simple: single alpha + `. `
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b'.' && bytes[2] == b' ' {
        return Some((
            ListKind::Number,
            std::str::from_utf8(&bytes[3..])
                .ok()?
                .trim_start()
                .to_string(),
        ));
    }
    None
}

// Suppress unused-import warnings — FontInfo will be re-exported once the
// docx writer (Task 5) consumes it.
#[allow(dead_code)]
fn _ensure_font_info_link(_: &FontInfo) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(page: u32, x: f32, y: f32, text: &str, size: f32, bold: bool) -> TextRun {
        TextRun {
            page,
            x,
            y,
            text: text.to_string(),
            font_name: if bold {
                "Helvetica-Bold".into()
            } else {
                "Helvetica".into()
            },
            font_size: size,
            bold,
            italic: false,
        }
    }

    #[test]
    fn cluster_lines_groups_by_y_baseline() {
        let runs = vec![
            run(1, 100.0, 700.0, "Hello", 12.0, false),
            run(1, 140.0, 700.0, "world", 12.0, false),
            run(1, 100.0, 680.0, "Next line", 12.0, false),
        ];
        let lines = cluster_lines(&runs);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].text.contains("Hello"));
        assert!(lines[0].text.contains("world"));
        assert_eq!(lines[1].text, "Next line");
    }

    #[test]
    fn cluster_paragraphs_splits_on_large_vertical_gap() {
        let lines_runs = vec![
            run(1, 100.0, 700.0, "Para A line 1", 12.0, false),
            run(1, 100.0, 686.0, "Para A line 2", 12.0, false),
            run(1, 100.0, 640.0, "Para B line 1", 12.0, false), // 46pt gap
        ];
        let lines = cluster_lines(&lines_runs);
        let paragraphs = cluster_paragraphs(&lines);
        assert_eq!(paragraphs.len(), 2, "{:#?}", paragraphs);
        assert!(paragraphs[0].text().contains("Para A line 1"));
        assert!(paragraphs[0].text().contains("Para A line 2"));
        assert!(paragraphs[1].text().contains("Para B line 1"));
    }

    #[test]
    fn classify_paragraph_detects_heading_by_font_size() {
        let opts = ReflowOptions::default();
        let runs = vec![run(1, 100.0, 700.0, "Chapter 1: Introduction", 18.0, false)];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let block = classify_paragraph(&paragraphs[0], 12.0, &opts);
        match block {
            Block::Heading { level, text } => {
                assert!((1..=3).contains(&level), "level={}", level);
                assert_eq!(text, "Chapter 1: Introduction");
            }
            other => panic!("expected Heading, got {:?}", other),
        }
    }

    #[test]
    fn classify_paragraph_detects_heading_by_bold_short_line() {
        let opts = ReflowOptions::default();
        let runs = vec![run(1, 100.0, 700.0, "Section A", 12.0, true)];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let block = classify_paragraph(&paragraphs[0], 12.0, &opts);
        assert!(matches!(block, Block::Heading { .. }));
    }

    #[test]
    fn classify_paragraph_detects_bullet_list_item() {
        let opts = ReflowOptions::default();
        let runs = vec![run(1, 100.0, 700.0, "• First bullet point", 12.0, false)];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let block = classify_paragraph(&paragraphs[0], 12.0, &opts);
        match block {
            Block::ListItem { kind, text, .. } => {
                assert_eq!(kind, ListKind::Bullet);
                assert_eq!(text, "First bullet point");
            }
            other => panic!("expected ListItem(Bullet), got {:?}", other),
        }
    }

    #[test]
    fn classify_paragraph_detects_numbered_list_item() {
        let opts = ReflowOptions::default();
        let runs = vec![run(1, 100.0, 700.0, "1. Numbered item", 12.0, false)];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let block = classify_paragraph(&paragraphs[0], 12.0, &opts);
        match block {
            Block::ListItem { kind, text, .. } => {
                assert_eq!(kind, ListKind::Number);
                assert_eq!(text, "Numbered item");
            }
            other => panic!("expected ListItem(Number), got {:?}", other),
        }
    }

    #[test]
    fn detect_list_prefix_handles_common_forms() {
        assert_eq!(
            detect_list_prefix("• foo"),
            Some((ListKind::Bullet, "foo".into()))
        );
        assert_eq!(
            detect_list_prefix("- foo"),
            Some((ListKind::Bullet, "foo".into()))
        );
        assert_eq!(
            detect_list_prefix("1) foo"),
            Some((ListKind::Number, "foo".into()))
        );
        assert_eq!(
            detect_list_prefix("(2) foo"),
            Some((ListKind::Number, "foo".into()))
        );
        assert_eq!(
            detect_list_prefix("a. foo"),
            Some((ListKind::Number, "foo".into()))
        );
        // Negative: a minus sign in a math expression must NOT be a bullet.
        assert!(detect_list_prefix("-5 + 3").is_none());
    }

    #[test]
    fn body_font_size_picks_most_common_run_size() {
        let runs = vec![
            run(1, 0.0, 700.0, "title", 24.0, true),
            run(1, 0.0, 670.0, "body text aaaa", 12.0, false),
            run(1, 0.0, 650.0, "body text bbbb", 12.0, false),
            run(1, 0.0, 630.0, "body text cccc", 12.0, false),
        ];
        assert!((estimate_body_font_size(&runs) - 12.0).abs() < 0.01);
    }

    #[test]
    fn reconstruct_blocks_full_pipeline_smoke() {
        let opts = ReflowOptions::default();
        let runs = vec![
            run(1, 100.0, 720.0, "My Document", 18.0, true),
            run(1, 100.0, 690.0, "This is the first paragraph.", 12.0, false),
            run(1, 100.0, 676.0, "It continues here.", 12.0, false),
            run(1, 100.0, 640.0, "• bullet one", 12.0, false),
            run(1, 100.0, 626.0, "• bullet two", 12.0, false),
        ];
        let blocks = reconstruct_blocks(&runs, &opts);
        assert!(
            matches!(blocks.first(), Some(Block::Heading { .. })),
            "first = {:?}",
            blocks.first()
        );
        // Should contain at least one Body and at least one ListItem (bullet).
        let bodies = blocks
            .iter()
            .filter(|b| matches!(b, Block::Body { .. }))
            .count();
        let bullets = blocks
            .iter()
            .filter(|b| {
                matches!(
                    b,
                    Block::ListItem {
                        kind: ListKind::Bullet,
                        ..
                    }
                )
            })
            .count();
        assert!(bodies >= 1, "blocks = {:#?}", blocks);
        assert!(bullets >= 1, "blocks = {:#?}", blocks);
    }
}
