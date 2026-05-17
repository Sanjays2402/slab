//! Table extraction from text-native PDFs.
//!
//! Strategy: shell out to `pdftotext -bbox-layout` (poppler) to get
//! per-word bounding boxes in XHTML; cluster words into rows by y-band
//! overlap; cluster columns by 1-D x-gap; snap each word to its column
//! and emit a 2-D `Table`. Deterministic, no ML, cheap.
//!
//! This module assumes the PDF has real (extractable) text. Scanned
//! PDFs must be sent through `pdf::ocr` first; the Reader UI gates
//! the Tables panel on the scan_audit recommendation accordingly.
//!
//! ## External binaries
//!
//! * `pdftotext` (poppler) — XHTML word-bbox output. Probed at call
//!   time with a friendly install hint if missing.

use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Axis-aligned bounding box in PDF user-space points. pdftotext emits
/// top-left origin coordinates, which we mirror unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

/// A single detected table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Page number (1-based).
    pub page: u32,
    /// Logical ordinal of this table on the page (1-based, top-to-bottom).
    pub index: u32,
    /// Bounding box of the table on the page.
    pub bbox: BBox,
    /// Rows of cells; each row has `columns` entries (padded with "" for
    /// missing cells).
    pub rows: Vec<Vec<String>>,
    /// Number of columns the heuristic locked onto.
    pub columns: u32,
}

impl Table {
    /// Convenience: total number of rows including any header.
    pub fn row_count(&self) -> u32 {
        self.rows.len() as u32
    }
}

/// Knobs for the extractor.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TableOpts {
    /// Page to extract from (1-based).
    pub page: u32,
    /// Minimum rows for a candidate block to count as a table.
    /// Defaults to 2 (header + ≥1 data row).
    #[serde(default = "default_min_rows")]
    pub min_rows: u32,
    /// Minimum columns for a candidate block to count as a table.
    /// Defaults to 2 (single-column "tables" are just paragraphs).
    #[serde(default = "default_min_cols")]
    pub min_cols: u32,
}

fn default_min_rows() -> u32 {
    2
}
fn default_min_cols() -> u32 {
    2
}

/// Internal: a single word from pdftotext output with its bbox.
#[derive(Debug, Clone)]
struct Word {
    bbox: BBox,
    text: String,
}

/// Extract every detected table on `opts.page` of `input`. Returns an
/// empty vec if no candidate satisfies the min_rows / min_cols thresholds.
pub fn extract_tables(input: &Path, opts: &TableOpts) -> Result<Vec<Table>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    require_pdftotext()?;
    let words = run_pdftotext_bbox(input, opts.page)?;
    Ok(detect_tables(&words, opts))
}

/// Serialize a `Table` to CSV (RFC 4180 quoting: every cell wrapped in
/// `"…"`, inner double-quotes doubled). No leading BOM. LF line endings.
pub fn to_csv(t: &Table) -> String {
    let mut out = String::new();
    for row in &t.rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            for ch in cell.chars() {
                if ch == '"' {
                    out.push('"');
                }
                out.push(ch);
            }
            out.push('"');
        }
        out.push('\n');
    }
    out
}

// ---------- pdftotext invocation ----------

fn require_pdftotext() -> Result<(), PdfError> {
    match Command::new("pdftotext").arg("-v").output() {
        Ok(_) => Ok(()),
        Err(e) => Err(PdfError::Other(format!(
            "pdftotext not found on PATH ({e}). \
             On macOS: `brew install poppler`. \
             On Debian/Ubuntu: `sudo apt install poppler-utils`."
        ))),
    }
}

fn run_pdftotext_bbox(input: &Path, page: u32) -> Result<Vec<Word>, PdfError> {
    let output = Command::new("pdftotext")
        .arg("-bbox-layout")
        .arg("-f")
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg(input)
        .arg("-")
        .output()
        .map_err(|e| PdfError::Other(format!("run pdftotext: {e}")))?;
    if !output.status.success() {
        return Err(PdfError::Other(format!(
            "pdftotext exited {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let xml = String::from_utf8_lossy(&output.stdout);
    Ok(parse_bbox_xml(&xml))
}

// ---------- XML parsing (hand-rolled) ----------

/// Parse pdftotext's `-bbox-layout` XHTML into a flat list of words with
/// bboxes. The format is mechanically simple — one
/// `<word xMin="..." yMin="..." xMax="..." yMax="...">TEXT</word>` per
/// line — so we don't pull in a full XML parser.
fn parse_bbox_xml(xml: &str) -> Vec<Word> {
    let mut out = Vec::new();
    for line in xml.lines() {
        let line = line.trim();
        if !line.starts_with("<word ") {
            continue;
        }
        let x_min = parse_attr(line, "xMin=\"").unwrap_or(0.0);
        let y_min = parse_attr(line, "yMin=\"").unwrap_or(0.0);
        let x_max = parse_attr(line, "xMax=\"").unwrap_or(0.0);
        let y_max = parse_attr(line, "yMax=\"").unwrap_or(0.0);
        let text = match (line.find('>'), line.rfind("</word>")) {
            (Some(a), Some(b)) if a + 1 <= b => xml_unescape(&line[a + 1..b]),
            _ => continue,
        };
        if text.is_empty() {
            continue;
        }
        out.push(Word {
            bbox: BBox {
                x_min,
                y_min,
                x_max,
                y_max,
            },
            text,
        });
    }
    out
}

fn parse_attr(line: &str, key: &str) -> Option<f32> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ---------- Clustering ----------

/// Cluster words into rows by y-band overlap, then within each row sort
/// left-to-right. Tables that match `opts.min_rows` / `opts.min_cols`
/// are returned. Currently emits at most one table per page (the
/// dominant column set).
fn detect_tables(words: &[Word], opts: &TableOpts) -> Vec<Table> {
    if words.is_empty() {
        return Vec::new();
    }
    let rows = cluster_rows(words);
    if rows.len() < opts.min_rows as usize {
        return Vec::new();
    }

    // Column detection: cluster x_min across every word.
    let mut all_x: Vec<f32> = words.iter().map(|w| w.bbox.x_min).collect();
    all_x.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let col_centers = cluster_1d(&all_x, 12.0);
    if col_centers.len() < opts.min_cols as usize {
        return Vec::new();
    }

    let mut table_rows: Vec<Vec<String>> = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut cells: Vec<String> = vec![String::new(); col_centers.len()];
        for w in row {
            let idx = nearest_index(&col_centers, w.bbox.x_min);
            if !cells[idx].is_empty() {
                cells[idx].push(' ');
            }
            cells[idx].push_str(&w.text);
        }
        table_rows.push(cells);
    }

    let bbox = words_bbox(words);

    vec![Table {
        page: opts.page,
        index: 1,
        bbox,
        rows: table_rows,
        columns: col_centers.len() as u32,
    }]
}

/// Sort words top-to-bottom and group consecutive words whose y-bands
/// overlap into a single row, then sort each row left-to-right.
fn cluster_rows(words: &[Word]) -> Vec<Vec<Word>> {
    let mut sorted: Vec<Word> = words.to_vec();
    sorted.sort_by(|a, b| a.bbox.y_min.partial_cmp(&b.bbox.y_min).unwrap());
    let mut rows: Vec<Vec<Word>> = Vec::new();
    for w in sorted {
        let placed = match rows.last_mut() {
            Some(row) => {
                let row_y_min = row
                    .iter()
                    .map(|w| w.bbox.y_min)
                    .fold(f32::INFINITY, f32::min);
                let row_y_max = row
                    .iter()
                    .map(|w| w.bbox.y_max)
                    .fold(f32::NEG_INFINITY, f32::max);
                let overlap = (w.bbox.y_max.min(row_y_max) - w.bbox.y_min.max(row_y_min)).max(0.0);
                let h = (w.bbox.y_max - w.bbox.y_min).max(0.1);
                if overlap / h >= 0.5 {
                    row.push(w.clone());
                    true
                } else {
                    false
                }
            }
            None => false,
        };
        if !placed {
            rows.push(vec![w]);
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| a.bbox.x_min.partial_cmp(&b.bbox.x_min).unwrap());
    }
    rows
}

/// Cluster a sorted list of floats into centers; start a new cluster
/// whenever the gap from the previous value exceeds `gap`.
fn cluster_1d(sorted: &[f32], gap: f32) -> Vec<f32> {
    let mut centers = Vec::new();
    let mut current: Vec<f32> = Vec::new();
    let mut last: f32 = f32::NEG_INFINITY;
    for &x in sorted {
        if !current.is_empty() && x - last > gap {
            centers.push(current.iter().copied().sum::<f32>() / current.len() as f32);
            current.clear();
        }
        current.push(x);
        last = x;
    }
    if !current.is_empty() {
        centers.push(current.iter().copied().sum::<f32>() / current.len() as f32);
    }
    centers
}

fn nearest_index(centers: &[f32], x: f32) -> usize {
    let mut best = 0;
    let mut best_dist = f32::INFINITY;
    for (i, &c) in centers.iter().enumerate() {
        let d = (c - x).abs();
        if d < best_dist {
            best_dist = d;
            best = i;
        }
    }
    best
}

fn words_bbox(words: &[Word]) -> BBox {
    let mut b = BBox {
        x_min: f32::INFINITY,
        y_min: f32::INFINITY,
        x_max: f32::NEG_INFINITY,
        y_max: f32::NEG_INFINITY,
    };
    for w in words {
        b.x_min = b.x_min.min(w.bbox.x_min);
        b.y_min = b.y_min.min(w.bbox.y_min);
        b.x_max = b.x_max.max(w.bbox.x_max);
        b.y_max = b.y_max.max(w.bbox.y_max);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_table_pdf;

    #[test]
    fn missing_input_errors() {
        let opts = TableOpts {
            page: 1,
            min_rows: 2,
            min_cols: 2,
        };
        let r = extract_tables(Path::new("/this/path/does/not/exist.pdf"), &opts);
        assert!(matches!(r, Err(PdfError::InputMissing(_))));
    }

    #[test]
    fn parse_attr_basic() {
        let line = r#"<word xMin="10.5" yMin="20.0" xMax="30.0" yMax="40.0">Hi</word>"#;
        assert_eq!(parse_attr(line, "xMin=\""), Some(10.5));
        assert_eq!(parse_attr(line, "yMax=\""), Some(40.0));
        assert_eq!(parse_attr(line, "nope=\""), None);
    }

    #[test]
    fn parse_bbox_xml_two_words() {
        let xml = r#"
<doc>
  <word xMin="1.0" yMin="2.0" xMax="3.0" yMax="4.0">Hello</word>
  <word xMin="5.0" yMin="2.0" xMax="7.0" yMax="4.0">World</word>
</doc>"#;
        let words = parse_bbox_xml(xml);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[1].text, "World");
        assert_eq!(words[0].bbox.x_min, 1.0);
        assert_eq!(words[1].bbox.x_max, 7.0);
    }

    #[test]
    fn parse_bbox_xml_unescapes_entities() {
        let xml = r#"<word xMin="0" yMin="0" xMax="1" yMax="1">A &amp; B &lt;c&gt;</word>"#;
        let words = parse_bbox_xml(xml);
        assert_eq!(words[0].text, "A & B <c>");
    }

    #[test]
    fn parse_bbox_xml_ignores_non_word_lines() {
        let xml = r#"<doc><page><flow><block><line>
<word xMin="1" yMin="1" xMax="2" yMax="2">only</word>
</line></block></flow></page></doc>"#;
        let words = parse_bbox_xml(xml);
        assert_eq!(words.len(), 1);
    }

    #[test]
    fn cluster_1d_three_groups() {
        let sorted = vec![10.0, 11.0, 12.0, 50.0, 51.0, 100.0];
        let centers = cluster_1d(&sorted, 12.0);
        assert_eq!(centers.len(), 3);
        assert!((centers[0] - 11.0).abs() < 0.01);
        assert!((centers[1] - 50.5).abs() < 0.01);
        assert!((centers[2] - 100.0).abs() < 0.01);
    }

    #[test]
    fn cluster_1d_single_cluster_when_no_gap() {
        let sorted = vec![10.0, 11.0, 12.0, 13.0];
        let centers = cluster_1d(&sorted, 12.0);
        assert_eq!(centers.len(), 1);
    }

    fn mk_word(x: f32, y: f32, t: &str) -> Word {
        Word {
            bbox: BBox {
                x_min: x,
                y_min: y,
                x_max: x + 10.0,
                y_max: y + 10.0,
            },
            text: t.into(),
        }
    }

    #[test]
    fn cluster_rows_groups_overlapping_words() {
        let words = vec![
            mk_word(0.0, 100.0, "row1a"),
            mk_word(50.0, 100.0, "row1b"),
            mk_word(0.0, 200.0, "row2a"),
            mk_word(50.0, 200.0, "row2b"),
        ];
        let rows = cluster_rows(&words);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].text, "row1a");
        assert_eq!(rows[0][1].text, "row1b");
        assert_eq!(rows[1][0].text, "row2a");
    }

    #[test]
    fn detect_tables_synthetic_grid() {
        let mut words = Vec::new();
        for r in 0..3 {
            for c in 0..3 {
                words.push(Word {
                    bbox: BBox {
                        x_min: c as f32 * 100.0,
                        y_min: r as f32 * 20.0,
                        x_max: c as f32 * 100.0 + 30.0,
                        y_max: r as f32 * 20.0 + 10.0,
                    },
                    text: format!("r{r}c{c}"),
                });
            }
        }
        let opts = TableOpts {
            page: 1,
            min_rows: 2,
            min_cols: 2,
        };
        let tables = detect_tables(&words, &opts);
        assert_eq!(tables.len(), 1);
        let t = &tables[0];
        assert_eq!(t.columns, 3);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[0][0], "r0c0");
        assert_eq!(t.rows[2][2], "r2c2");
        assert_eq!(t.row_count(), 3);
    }

    #[test]
    fn detect_tables_below_min_rejected() {
        let words = vec![mk_word(0.0, 0.0, "lonely")];
        let opts = TableOpts {
            page: 1,
            min_rows: 2,
            min_cols: 2,
        };
        assert!(detect_tables(&words, &opts).is_empty());
    }

    #[test]
    fn detect_tables_empty_input() {
        let opts = TableOpts {
            page: 1,
            min_rows: 2,
            min_cols: 2,
        };
        assert!(detect_tables(&[], &opts).is_empty());
    }

    #[test]
    fn csv_rfc4180_quotes_doubled() {
        let t = Table {
            page: 1,
            index: 1,
            columns: 2,
            bbox: BBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 10.0,
                y_max: 10.0,
            },
            rows: vec![
                vec!["A".into(), "B,C".into()],
                vec!["she said \"hi\"".into(), "ok".into()],
            ],
        };
        let csv = to_csv(&t);
        assert_eq!(csv, "\"A\",\"B,C\"\n\"she said \"\"hi\"\"\",\"ok\"\n");
    }

    #[test]
    fn csv_empty_table_is_empty_string() {
        let t = Table {
            page: 1,
            index: 1,
            columns: 0,
            bbox: BBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 0.0,
                y_max: 0.0,
            },
            rows: vec![],
        };
        assert_eq!(to_csv(&t), "");
    }

    #[test]
    fn classification_serializes_through_json() {
        let t = Table {
            page: 2,
            index: 1,
            columns: 2,
            bbox: BBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 1.0,
                y_max: 1.0,
            },
            rows: vec![vec!["a".into(), "b".into()]],
        };
        let s = serde_json::to_string(&t).unwrap();
        let parsed: Table = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.page, 2);
        assert_eq!(parsed.columns, 2);
        assert_eq!(parsed.rows[0][1], "b");
    }

    fn pdftotext_available() -> bool {
        Command::new("pdftotext").arg("-v").output().is_ok()
    }

    #[test]
    fn end_to_end_extract_real_table_pdf() {
        if !pdftotext_available() {
            eprintln!("pdftotext unavailable — skipping");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_table_pdf(&p);
        let opts = TableOpts {
            page: 1,
            min_rows: 2,
            min_cols: 2,
        };
        let tables = extract_tables(&p, &opts).unwrap();
        assert_eq!(tables.len(), 1, "expected exactly one table");
        let t = &tables[0];
        assert!(
            t.columns >= 2 && t.columns <= 4,
            "got {} columns",
            t.columns
        );
        assert!(
            t.rows.len() >= 3 && t.rows.len() <= 5,
            "got {} rows",
            t.rows.len()
        );
        let flat: String = t
            .rows
            .iter()
            .flat_map(|r| r.iter().cloned())
            .collect::<Vec<_>>()
            .join(",");
        assert!(flat.contains("Name"), "header missing: {flat}");
        assert!(flat.contains("Alpha"), "data missing: {flat}");
    }

    #[test]
    fn end_to_end_csv_round_trip() {
        if !pdftotext_available() {
            eprintln!("pdftotext unavailable — skipping");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_table_pdf(&p);
        let opts = TableOpts {
            page: 1,
            min_rows: 2,
            min_cols: 2,
        };
        let tables = extract_tables(&p, &opts).unwrap();
        let csv = to_csv(&tables[0]);
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() >= 3, "expected ≥3 lines, got {}", lines.len());
        assert!(lines[0].contains("Name"));
    }
}
