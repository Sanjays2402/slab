// Core types for the reflow PDF→DOCX pipeline.
//
// We model the pipeline as three layers:
//
//   TextRun  →  Block  →  DOCX OOXML
//
// where TextRun is a single positioned glyph-run pulled out of the PDF
// content stream, Block is a higher-level layout primitive (paragraph,
// heading, list item, table row), and the writer turns Blocks into
// `<w:p>` / `<w:tbl>` etc.

use serde::{Deserialize, Serialize};

/// Caller-tunable knobs for the conversion. All optional; defaults match the
/// "drop a typical office PDF and get something Word-editable" path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflowOptions {
    /// If true, detect tables via column-x clustering. Default: true.
    pub detect_tables: bool,
    /// If true, detect bullet/numbered lists. Default: true.
    pub detect_lists: bool,
    /// Heading classifier: any paragraph whose median font size is >=
    /// `heading_size_ratio * body_size` is treated as a heading.
    /// Default 1.25.
    pub heading_size_ratio: f32,
    /// If true, embed page-break markers between source PDF pages. Default: false
    /// (Word's auto-reflow is the whole point).
    pub preserve_page_breaks: bool,
    /// Locale hint for bullet/number-list recognition (e.g. "1." vs "1)" vs "①").
    /// Default "en".
    pub locale: String,
}

impl Default for ReflowOptions {
    fn default() -> Self {
        Self {
            detect_tables: true,
            detect_lists: true,
            heading_size_ratio: 1.25,
            preserve_page_breaks: false,
            locale: "en".to_string(),
        }
    }
}

/// A single positioned text fragment pulled out of a PDF content stream.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub page: u32,
    /// User-space X of the first glyph baseline.
    pub x: f32,
    /// User-space Y of the baseline (PDF coords — Y grows upward).
    pub y: f32,
    /// Decoded UTF-8 text.
    pub text: String,
    pub font_name: String,
    /// Effective font size in points (post-Tm scaling).
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
}

/// A list-item flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListKind {
    Bullet,
    Number,
}

/// Higher-level layout primitive emitted by the layout pass and consumed by
/// the DOCX writer.
#[derive(Debug, Clone)]
pub enum Block {
    Body {
        text: String,
    },
    Heading {
        level: u8, // 1..=3
        text: String,
    },
    ListItem {
        kind: ListKind,
        text: String,
        /// Indent level (0 = top-level, 1 = first nested, ...).
        indent: u8,
    },
    /// One row of a detected table. Multiple consecutive `TableRow`s with the
    /// same column count form a single `<w:tbl>` in the output.
    TableRow {
        cells: Vec<String>,
    },
}

/// Summary returned by `convert_to_docx`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflowReport {
    pub pages: u32,
    pub paragraphs: u32,
    pub headings: u32,
    pub list_items: u32,
    pub table_rows: u32,
    pub bytes_written: u64,
    pub duration_ms: u64,
}

impl ReflowReport {
    pub fn empty() -> Self {
        Self {
            pages: 0,
            paragraphs: 0,
            headings: 0,
            list_items: 0,
            table_rows: 0,
            bytes_written: 0,
            duration_ms: 0,
        }
    }
}
