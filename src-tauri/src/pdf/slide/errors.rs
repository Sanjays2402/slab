// Errors for the Slide (PDF → PPTX) pipeline.

use thiserror::Error;

use crate::pdf::reflow::errors::ReflowError;

#[derive(Debug, Error)]
pub enum SlideError {
    #[error("input PDF not found: {0}")]
    InputMissing(String),

    #[error("output directory does not exist or is not writable: {0}")]
    OutputNotWritable(String),

    #[error("PDF parse error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("zip write error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no pages in PDF")]
    Empty,

    #[error("upstream reflow error: {0}")]
    Reflow(String),
}

impl From<ReflowError> for SlideError {
    fn from(e: ReflowError) -> Self {
        match e {
            ReflowError::InputMissing(s) => SlideError::InputMissing(s),
            ReflowError::OutputNotWritable(s) => SlideError::OutputNotWritable(s),
            ReflowError::Pdf(e) => SlideError::Pdf(e),
            ReflowError::Io(e) => SlideError::Io(e),
            other => SlideError::Reflow(other.to_string()),
        }
    }
}
