use thiserror::Error;

#[derive(Debug, Error)]
pub enum EpubError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("reflow: {0}")]
    Reflow(String),
    #[error("no extractable text — PDF appears to be image-only; run OCR first")]
    EmptyDocument,
    #[error("invalid output path: {0}")]
    InvalidPath(String),
}
