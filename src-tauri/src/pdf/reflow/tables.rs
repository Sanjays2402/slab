// Table detection — find runs of paragraphs whose lines share aligned
// x-column positions, and report them as TableRows.
//
// Algorithm (DBSCAN-lite on column starts):
//
// 1. Collect candidate "row paragraphs": single-line paragraphs whose run-count
//    >= 2 (a real table row has multiple cells).
// 2. For each consecutive group of candidate rows, cluster their column-X
//    positions across rows with tolerance ±3pt.
// 3. If three or more consecutive rows share >= 2 column positions, that span
//    is a table.
//
// We deliberately avoid detecting tables from drawn lines (rectangles / ruling),
// because most modern PDFs ship tables as positioned text without drawn cells.

use super::layout::{Line, Paragraph};

/// One detected table span over `paragraphs[start..end]`, with the column
/// origin X-positions shared across its rows.
#[derive(Debug, Clone)]
pub struct TableSpan {
    pub start: usize,
    pub end: usize,
    pub column_xs: Vec<f32>,
}

/// Find table spans in the paragraph stream.
pub fn detect_tables(paragraphs: &[Paragraph]) -> Vec<TableSpan> {
    let mut spans: Vec<TableSpan> = Vec::new();
    let mut i = 0;
    while i < paragraphs.len() {
        // Find a maximal contiguous run of row-candidates starting at i.
        let mut j = i;
        while j < paragraphs.len() && is_row_candidate(&paragraphs[j]) {
            j += 1;
        }
        if j - i >= 3 {
            // Cluster column-X positions across paragraphs[i..j].
            let xs = cluster_column_xs(&paragraphs[i..j]);
            if xs.len() >= 2 && rows_agree_on_columns(&paragraphs[i..j], &xs) {
                spans.push(TableSpan {
                    start: i,
                    end: j,
                    column_xs: xs,
                });
                i = j;
                continue;
            }
        }
        i = if j > i { j } else { i + 1 };
    }
    spans
}

fn is_row_candidate(p: &Paragraph) -> bool {
    p.lines.len() == 1 && p.lines[0].runs.len() >= 2
}

/// Cluster the union of all run-X positions in `rows` with tolerance ±3pt.
fn cluster_column_xs(rows: &[Paragraph]) -> Vec<f32> {
    let tol: f32 = 3.0;
    let mut xs: Vec<f32> = Vec::new();
    for p in rows {
        for r in &p.lines[0].runs {
            xs.push(r.x);
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut clusters: Vec<(f32, u32)> = Vec::new();
    for x in xs {
        if let Some(last) = clusters.last_mut() {
            if (last.0 - x).abs() <= tol {
                // Update running mean.
                let n = last.1 as f32;
                last.0 = (last.0 * n + x) / (n + 1.0);
                last.1 += 1;
                continue;
            }
        }
        clusters.push((x, 1));
    }
    // Keep clusters that appear in at least 2 different rows (real columns).
    // Quick proxy: count >= rows.len() means most rows hit it; require at
    // least ceil(rows.len() * 0.5).
    let threshold = ((rows.len() as f32) * 0.5).ceil() as u32;
    clusters
        .into_iter()
        .filter(|(_, c)| *c >= threshold)
        .map(|(x, _)| x)
        .collect()
}

fn rows_agree_on_columns(rows: &[Paragraph], xs: &[f32]) -> bool {
    // Every row must have at least 2 runs that match columns within ±3pt.
    let tol: f32 = 3.0;
    for p in rows {
        let mut hits = 0;
        for col in xs {
            if p.lines[0].runs.iter().any(|r| (r.x - col).abs() <= tol) {
                hits += 1;
            }
        }
        if hits < 2 {
            return false;
        }
    }
    true
}

/// Given a paragraph that's part of a table span and the table's column origin
/// X values, return the concatenated cell text for each column.
pub fn extract_cells(p: &Paragraph, column_xs: &[f32]) -> Vec<String> {
    let line: &Line = &p.lines[0];
    let mut cells: Vec<String> = vec![String::new(); column_xs.len()];
    for run in &line.runs {
        // Assign to the nearest column.
        let mut best = 0usize;
        let mut best_dist = f32::INFINITY;
        for (i, c) in column_xs.iter().enumerate() {
            let d = (run.x - *c).abs();
            if d < best_dist {
                best_dist = d;
                best = i;
            }
        }
        if !cells[best].is_empty() {
            cells[best].push(' ');
        }
        cells[best].push_str(&run.text);
    }
    for c in cells.iter_mut() {
        *c = c.trim().to_string();
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::super::layout::{cluster_lines, cluster_paragraphs};
    use super::super::types::TextRun;
    use super::*;

    fn run(x: f32, y: f32, text: &str) -> TextRun {
        TextRun {
            page: 1,
            x,
            y,
            text: text.to_string(),
            font_name: "Helvetica".into(),
            font_size: 11.0,
            bold: false,
            italic: false,
        }
    }

    #[test]
    fn detects_3_row_table_with_3_columns() {
        // Header + 2 data rows: Name | Age | City at columns ≈100, 200, 300.
        let runs = vec![
            run(100.0, 700.0, "Name"),
            run(200.0, 700.0, "Age"),
            run(300.0, 700.0, "City"),
            run(100.0, 680.0, "Alice"),
            run(200.0, 680.0, "30"),
            run(300.0, 680.0, "Seattle"),
            run(100.0, 660.0, "Bob"),
            run(200.0, 660.0, "25"),
            run(300.0, 660.0, "Austin"),
        ];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let spans = detect_tables(&paragraphs);
        assert_eq!(spans.len(), 1, "spans = {:#?}", spans);
        let s = &spans[0];
        assert_eq!(s.end - s.start, 3);
        assert_eq!(s.column_xs.len(), 3);
        let cells = extract_cells(&paragraphs[s.start + 1], &s.column_xs);
        assert_eq!(cells, vec!["Alice", "30", "Seattle"]);
    }

    #[test]
    fn rejects_2_aligned_rows_below_threshold() {
        let runs = vec![
            run(100.0, 700.0, "Name"),
            run(200.0, 700.0, "Age"),
            run(100.0, 680.0, "Alice"),
            run(200.0, 680.0, "30"),
        ];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let spans = detect_tables(&paragraphs);
        assert!(spans.is_empty(), "spans = {:#?}", spans);
    }

    #[test]
    fn rejects_3_rows_without_column_alignment() {
        let runs = vec![
            run(100.0, 700.0, "Sentence one"),
            run(120.0, 680.0, "Sentence two"),
            run(140.0, 660.0, "Sentence three"),
        ];
        let lines = cluster_lines(&runs);
        let paragraphs = cluster_paragraphs(&lines);
        let spans = detect_tables(&paragraphs);
        assert!(spans.is_empty());
    }
}
