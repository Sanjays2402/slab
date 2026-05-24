// Errors for the Markdown / HTML emit pipeline.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("input PDF not found: {0}")]
    InputMissing(String),
    #[error("output not writable: {0}")]
    OutputNotWritable(String),
    #[error("PDF parse error: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("reflow pipeline error: {0}")]
    Reflow(#[from] crate::pdf::reflow::ReflowError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
