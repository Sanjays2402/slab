//! v3.13.0 "Streamline" — PDF linearization (Fast Web View).
//!
//! Reorders objects in a PDF so the first page object subtree appears
//! immediately after the linearization parameter dictionary + primary
//! hint stream, letting a streaming reader render page 1 long before
//! the rest of the file finishes downloading. See PDF 1.4 §F.

pub mod audit;
pub mod depgraph;
pub mod dto;
pub mod hint_stream;
pub mod inspect;
pub mod linearize;
pub mod param_dict;

pub use audit::{audit_folder, AuditEntry, AuditReport};
pub use dto::{LinearizationStatus, LinearizeReport, LinearizeStats};
pub use hint_stream::{build_primary_hint_stream, HintInputs, PageRecord};
pub use inspect::is_linearized;
pub use linearize::linearize_pdf;
pub use param_dict::{build_param_dict, LinearizationParams};
