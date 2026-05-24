// Errors for the reflow PDF→DOCX pipeline.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReflowError {
    #[error("input PDF not found: {0}")]
    InputMissing(String),

    #[error("output path is not writable: {0}")]
    OutputNotWritable(String),

    #[error("PDF parse error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("DOCX/ZIP write error: {0}")]
    Zip(String),

    #[error("XML emission error: {0}")]
    Xml(String),

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("this code path is not yet implemented (stub)")]
    NotYetImplemented,
}
