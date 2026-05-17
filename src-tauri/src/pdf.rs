// PDF operations for Slab.
//
// Each operation lives in its own submodule so the surface stays small and
// each feature can be tested in isolation.

pub mod annot_export;
pub mod annotations;
pub mod auto_redact;
pub mod compress;
pub mod crop;
pub mod diff;
pub mod duplicate;
pub mod edit_text;
pub mod encrypt;
pub mod extract;
pub mod flatten;
pub mod grayscale;
pub mod header_footer;
pub mod info;
pub mod insert;
pub mod library;
pub mod md2pdf;
pub mod merge;
pub mod metadata;
pub mod nup;
pub mod ocr;
pub mod outline;
pub mod page_labels;
pub mod page_numbers;
pub mod pages;
pub mod pages_build;
pub mod polyglot;
pub mod preflight;
pub mod redact;
pub mod repair;
pub mod sanitize;
pub mod scan_audit;
pub mod slides;
pub mod split;
pub mod split_pattern;
pub mod table_extract;
pub mod watermark;

#[cfg(test)]
pub mod test_fixtures;

use std::io;
use thiserror::Error;

/// Top-level error type. All PDF ops produce one of these; they convert
/// cleanly into the JSON `CmdResult::Err` the Svelte side renders.
#[derive(Debug, Error)]
pub enum PdfError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("pdf parse/serialize error: {0}")]
    Lopdf(#[from] lopdf::Error),

    #[error("input file does not exist: {0}")]
    InputMissing(String),

    #[error("no input files provided")]
    NoInputs,

    #[error("output path is empty")]
    EmptyOutput,

    #[error("operation failed: {0}")]
    Other(String),
}
