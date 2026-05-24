//! Tabulate — offline PDF → Excel (.xlsx) conversion.
//!
//! Reuses the Reflow extraction + layout + table-detection pipeline and
//! emits OOXML SpreadsheetML instead of WordprocessingML.

pub mod cells;
pub mod errors;
pub mod extract;
pub mod types;
pub mod xlsx;

pub use errors::TabulateError;
pub use types::{TableExtract, TabulateOptions, TabulateReport};

use std::path::Path;
use std::time::Instant;

/// Convert a PDF on disk into an `.xlsx` workbook on disk.
///
/// Reuses the v3.14.0 Reflow extraction + layout + table-detection pipeline.
/// Returns `TabulateError::NoTablesFound` if no aligned-column tables are
/// detected; the caller can show a friendly empty-state.
pub fn convert_to_xlsx(
    input: &Path,
    output: &Path,
    opts: &TabulateOptions,
) -> Result<TabulateReport, TabulateError> {
    if !input.exists() {
        return Err(TabulateError::InputMissing(input.display().to_string()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(TabulateError::OutputNotWritable(
                output.display().to_string(),
            ));
        }
    }
    let started = Instant::now();
    let doc = lopdf::Document::load(input)?;
    let tables = extract::extract_tables(&doc)?;
    if tables.is_empty() {
        return Err(TabulateError::NoTablesFound);
    }
    let pages = doc.get_pages().len() as u32;
    let bytes = xlsx::write_xlsx(&tables, opts)?;
    std::fs::write(output, &bytes)?;

    let mut report = TabulateReport::empty();
    report.pages = pages;
    report.sheets = tables.len() as u32;
    report.tables = tables.len() as u32;
    for t in &tables {
        report.rows += t.rows.len() as u32;
        for row in &t.rows {
            report.cells += row.len() as u32;
            for raw in row {
                match cells::classify(raw) {
                    cells::Cell::Number(_) => report.numeric_cells += 1,
                    cells::Cell::Date(_) => report.date_cells += 1,
                    _ => {}
                }
            }
        }
    }
    report.bytes_written = bytes.len() as u64;
    report.duration_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_is_reasonable() {
        let opts = TabulateOptions::default();
        assert!(opts.type_numbers);
        assert!(opts.type_dates);
        assert!(!opts.include_non_table_text);
        assert_eq!(opts.sheet_name_pattern, "Page {n}");
    }

    #[test]
    fn report_empty_is_all_zeros() {
        let r = TabulateReport::empty();
        assert_eq!(r.pages, 0);
        assert_eq!(r.sheets, 0);
        assert_eq!(r.tables, 0);
        assert_eq!(r.cells, 0);
    }

    #[test]
    fn missing_input_returns_input_missing_error() {
        let out = std::env::temp_dir().join("tabulate-missing-input.xlsx");
        let result = convert_to_xlsx(
            Path::new("/definitely/does/not/exist.pdf"),
            &out,
            &TabulateOptions::default(),
        );
        assert!(matches!(result, Err(TabulateError::InputMissing(_))));
    }
}
