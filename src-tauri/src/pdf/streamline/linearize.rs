//! v3.13.0 placeholder — full linearizer lands in Task 6 of the plan.
//!
//! For now this surface returns a clear "not implemented" error so callers
//! get a useful error string in the UI rather than a panic.

use std::path::Path;

use crate::pdf::PdfError;

use super::dto::LinearizeReport;

pub fn linearize_pdf(_input: &Path, _output: &Path) -> Result<LinearizeReport, PdfError> {
    Err(PdfError::Other(
        "linearization writer not yet implemented (v3.13.0 task 6)".into(),
    ))
}
