//! Tabulate — PDF → Excel public types.

use serde::{Deserialize, Serialize};

/// Toggles controlling the tabulate pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabulateOptions {
    /// Try to parse cell text as f64; emit numeric `<c t="n">` cells when it parses.
    pub type_numbers: bool,
    /// Try to parse cell text as a date (ISO 8601 / common US+EU formats);
    /// emit Excel serial date numbers with a date style when it parses.
    pub type_dates: bool,
    /// If true, also emit a "Body Text" sheet containing all non-table
    /// paragraphs. Off by default — most users want only the tables.
    pub include_non_table_text: bool,
    /// Worksheet naming pattern. `{n}` → 1-based page number. Excel sheet
    /// names are limited to 31 chars; we truncate after formatting.
    pub sheet_name_pattern: String,
}

impl Default for TabulateOptions {
    fn default() -> Self {
        Self {
            type_numbers: true,
            type_dates: true,
            include_non_table_text: false,
            sheet_name_pattern: "Page {n}".to_string(),
        }
    }
}

/// One detected table on one page, ready for Excel emission.
#[derive(Debug, Clone, PartialEq)]
pub struct TableExtract {
    pub page: u32,
    pub rows: Vec<Vec<String>>,
}

/// Summary returned to the frontend after a successful conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabulateReport {
    pub pages: u32,
    pub sheets: u32,
    pub tables: u32,
    pub rows: u32,
    pub cells: u32,
    pub numeric_cells: u32,
    pub date_cells: u32,
    pub bytes_written: u64,
    pub duration_ms: u64,
}

impl TabulateReport {
    pub fn empty() -> Self {
        Self {
            pages: 0,
            sheets: 0,
            tables: 0,
            rows: 0,
            cells: 0,
            numeric_cells: 0,
            date_cells: 0,
            bytes_written: 0,
            duration_ms: 0,
        }
    }
}
