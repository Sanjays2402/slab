//! Detect tables in a PDF and emit them as `TableExtract` per page.
//!
//! Strategy: reuse the Reflow extraction + layout pipeline per page. Group
//! consecutive `Block::TableRow`s into per-page tables (one `TableExtract`
//! per contiguous group of rows). One detected table → one worksheet.

use super::types::TableExtract;
use super::TabulateError;
use crate::pdf::reflow::extract::extract_text_runs;
use crate::pdf::reflow::types::Block;
use crate::pdf::reflow::{layout, types::ReflowOptions};

/// Detect every table in `doc`, returning one `TableExtract` per contiguous
/// run of `Block::TableRow`s. Tables are tagged with the source page number.
pub fn extract_tables(doc: &lopdf::Document) -> Result<Vec<TableExtract>, TabulateError> {
    let runs = extract_text_runs(doc)?;
    if runs.is_empty() {
        return Ok(Vec::new());
    }

    // Force table detection ON regardless of caller — that's the whole point.
    let opts = ReflowOptions {
        detect_tables: true,
        ..ReflowOptions::default()
    };

    // Process page-by-page so we can tag every detected TableExtract with its
    // source page number. Reflow's `reconstruct_blocks` is page-agnostic, so
    // we slice the runs by page first.
    let mut out: Vec<TableExtract> = Vec::new();
    let max_page = runs.iter().map(|r| r.page).max().unwrap_or(0);
    for page in 1..=max_page {
        let page_runs: Vec<_> = runs.iter().filter(|r| r.page == page).cloned().collect();
        if page_runs.is_empty() {
            continue;
        }
        let blocks = layout::reconstruct_blocks(&page_runs, &opts);
        let mut current_rows: Vec<Vec<String>> = Vec::new();
        for block in blocks {
            match block {
                Block::TableRow { cells } => {
                    current_rows.push(cells);
                }
                _ => {
                    if !current_rows.is_empty() {
                        out.push(TableExtract {
                            page,
                            rows: std::mem::take(&mut current_rows),
                        });
                    }
                }
            }
        }
        if !current_rows.is_empty() {
            out.push(TableExtract {
                page,
                rows: current_rows,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_doc_yields_no_tables() {
        let doc = lopdf::Document::new();
        let tables = extract_tables(&doc).unwrap();
        assert!(tables.is_empty());
    }
}
