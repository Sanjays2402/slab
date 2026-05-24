//! Bind — offline PDF → EPUB 3.
//! Re-uses the reflow pipeline's extract + layout passes and the v3.17.0
//! semantic-HTML emitter, then wraps everything in an EPUB ZIP envelope.

pub mod errors;
pub mod package;
pub mod split;
pub mod types;
pub mod writer;

pub use errors::EpubError;
pub use types::{EpubOptions, EpubReport};

use std::path::Path;

/// Convert a PDF to an EPUB 3 file. Returns an `EpubReport`.
///
/// Implementation lands in Task 6 — Tasks 1-5 build the pieces.
pub fn convert_to_epub(
    _input: &Path,
    _output: &Path,
    _opts: &EpubOptions,
) -> Result<EpubReport, EpubError> {
    Err(EpubError::Reflow(
        "convert_to_epub not yet wired (Task 6)".to_string(),
    ))
}
