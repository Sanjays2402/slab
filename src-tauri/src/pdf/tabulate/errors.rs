//! Errors emitted by the tabulate pipeline.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TabulateError {
    #[error("input file not found: {0}")]
    InputMissing(String),
    #[error("output directory does not exist: {0}")]
    OutputNotWritable(String),
    #[error("no tables detected in this PDF")]
    NoTablesFound,
    #[error(transparent)]
    Pdf(#[from] lopdf::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("xml emit failed: {0}")]
    Xml(String),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Reflow(#[from] crate::pdf::reflow::ReflowError),
}
