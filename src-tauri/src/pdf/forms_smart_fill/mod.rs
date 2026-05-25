// v3.30.0 "Quill Smart Fill" — local AI-powered source-to-form mapping.
//
// Pipeline (this slice = backend only):
//
//   source doc (PDF/TXT/MD/CSV)  ──extract::extract_source_text──▶  source_text: String
//                                                                          │
//   target AcroForm PDF  ──forms::inspect──▶  Vec<FormField>  ──map──▶  Vec<FieldSpec>
//                                                                          │
//   (source_text, target FieldSpecs)  ──mapper::propose_fills──▶  SmartFillProposal
//
// `SmartFillProposal` is the data the front-end will render line-by-line
// in the Quill Smart Fill diff UI (Slice 2). Each entry carries a
// confidence so we can sort low-confidence rows to the bottom and let
// the user accept/reject before calling the existing `forms::fill`
// engine.
//
// All AI calls go through `crate::ai::AiProvider`, so the same code
// path supports Ollama (default), OpenAI-compatible, and the in-test
// MockProvider.

pub mod extract;
pub mod mapper;

use std::path::Path;
use std::sync::Arc;

use crate::ai::AiProvider;
use crate::pdf::{forms, PdfError};

pub use mapper::{FieldSpec, ProposalEntry, SmartFillProposal};

/// Top-level error type for the smart-fill pipeline.
#[derive(Debug, thiserror::Error)]
pub enum SmartFillError {
    #[error("pdf error: {0}")]
    Pdf(#[from] PdfError),

    #[error("ai error: {0}")]
    Ai(#[from] crate::ai::AiError),

    #[error("target PDF has no AcroForm fields to fill")]
    NoFields,
}

/// End-to-end: inspect the target PDF, extract text from the source,
/// ask the provider for a mapping, and return a [`SmartFillProposal`].
///
/// The caller is responsible for showing the proposal to the user and,
/// once accepted, building the `name -> value` map for
/// [`crate::pdf::forms::fill`].
pub async fn propose_smart_fill(
    target_pdf: &Path,
    source_doc: &Path,
    provider: Arc<dyn AiProvider>,
) -> Result<SmartFillProposal, SmartFillError> {
    let report = forms::inspect(target_pdf)?;
    if !report.has_acroform || report.fields.is_empty() {
        return Err(SmartFillError::NoFields);
    }
    let specs: Vec<FieldSpec> = report
        .fields
        .iter()
        .filter(|f| !f.read_only)
        .map(FieldSpec::from_form_field)
        .collect();
    if specs.is_empty() {
        return Err(SmartFillError::NoFields);
    }
    let source_text = extract::extract_source_text(source_doc)?;
    let proposal = mapper::propose_fills(provider, &source_text, &specs).await?;
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiError, AiProvider, ChatMessage, ChatOpts, ChatResponse};
    use async_trait::async_trait;
    use lopdf::{dictionary, Document, Object};
    use std::sync::Mutex;

    /// Build a temp AcroForm PDF with `(name, kind)` pairs as text/button leaves.
    fn build_tiny_acroform_pdf(fields: &[(&str, &str)]) -> tempfile::NamedTempFile {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut field_ids = Vec::new();
        let mut annots = Vec::new();
        let mut y = 700_i32;
        for (name, kind) in fields {
            let ft: &[u8] = match *kind {
                "button" => b"Btn",
                _ => b"Tx",
            };
            let id = doc.add_object(dictionary! {
                "T" => Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                "FT" => Object::Name(ft.to_vec()),
                "Rect" => vec![50.into(), (y - 20).into(), 250.into(), y.into()],
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "Type" => Object::Name(b"Annot".to_vec()),
            });
            field_ids.push(Object::Reference(id));
            annots.push(Object::Reference(id));
            y -= 30;
        }
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Annots" => annots,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let acroform_id = doc.add_object(dictionary! { "Fields" => field_ids });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let f = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        std::fs::write(f.path(), &buf).unwrap();
        f
    }

    fn write_temp_txt(content: &str) -> tempfile::NamedTempFile {
        let f = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        std::fs::write(f.path(), content).unwrap();
        f
    }

    struct MockProvider {
        reply: String,
        captured: Mutex<Vec<ChatMessage>>,
    }

    impl MockProvider {
        fn with_response(reply: impl Into<String>) -> Self {
            Self {
                reply: reply.into(),
                captured: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            *self.captured.lock().unwrap() = msgs.to_vec();
            Ok(ChatResponse {
                content: self.reply.clone(),
                model: "mock-smartfill".into(),
            })
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            unimplemented!()
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn end_to_end_propose_with_mock_provider() {
        let pdf = build_tiny_acroform_pdf(&[("full_name", "text"), ("email", "text")]);
        let src = write_temp_txt("Name: Jane Doe\nEmail: jane@x.com");
        let provider = Arc::new(MockProvider::with_response(
            r#"{"full_name": "Jane Doe", "email": "jane@x.com"}"#,
        ));
        let proposal = propose_smart_fill(pdf.path(), src.path(), provider)
            .await
            .unwrap();
        assert_eq!(proposal.entries.len(), 2);
        let by: std::collections::HashMap<_, _> = proposal
            .entries
            .iter()
            .map(|e| (e.field.as_str(), e.value.as_deref()))
            .collect();
        assert_eq!(by.get("full_name").copied().flatten(), Some("Jane Doe"));
        assert_eq!(by.get("email").copied().flatten(), Some("jane@x.com"));
    }

    #[tokio::test]
    async fn end_to_end_returns_no_fields_when_target_has_none() {
        let pdf = build_tiny_acroform_pdf(&[]);
        let src = write_temp_txt("anything");
        let provider = Arc::new(MockProvider::with_response("{}"));
        let err = propose_smart_fill(pdf.path(), src.path(), provider)
            .await
            .unwrap_err();
        assert!(matches!(err, SmartFillError::NoFields));
    }

    #[tokio::test]
    async fn end_to_end_unknown_target_extension_is_pdf_error() {
        let pdf = build_tiny_acroform_pdf(&[("name", "text")]);
        let bogus = tempfile::Builder::new()
            .suffix(".bogus")
            .tempfile()
            .unwrap();
        std::fs::write(bogus.path(), "x").unwrap();
        let provider = Arc::new(MockProvider::with_response("{}"));
        let err = propose_smart_fill(pdf.path(), bogus.path(), provider)
            .await
            .unwrap_err();
        assert!(matches!(err, SmartFillError::Pdf(_)));
    }
}
