// v3.30.0 "Quill Smart Fill" — local AI-powered source-to-form mapping.
//
// Slice 1.1: source-document text extraction.
// Slice 1.2 (this commit): AI mapping engine.
// Slice 1.3: end-to-end pipeline (next commit).

pub mod extract;
pub mod mapper;

pub use mapper::{FieldSpec, ProposalEntry, SmartFillProposal};
