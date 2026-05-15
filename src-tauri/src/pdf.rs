// PDF operations for Slab.
//
// Each operation lives in its own submodule so the surface stays small and
// each feature can be tested in isolation.

pub mod compress;
pub mod encrypt;
pub mod extract;
pub mod info;
pub mod merge;
pub mod pages;
pub mod split;
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
