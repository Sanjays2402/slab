//! v3.13.0 "Streamline" — PDF linearization (Fast Web View).
//!
//! Reorders objects in a PDF so the first page object subtree appears
//! immediately after the linearization parameter dictionary + primary
//! hint stream, letting a streaming reader render page 1 long before
//! the rest of the file finishes downloading. See PDF 1.4 §F.

pub mod depgraph;
pub mod dto;
pub mod inspect;
pub mod linearize;

pub use dto::{LinearizationStatus, LinearizeReport, LinearizeStats};
pub use inspect::is_linearized;
pub use linearize::linearize_pdf;
