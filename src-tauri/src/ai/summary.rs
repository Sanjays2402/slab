// Beacon Summary — one-call TL;DR / Short / Long summary of a PDF.
//
// Design rationale: we just lean on the existing provider abstraction and
// reuse the page-aware truncation from `chat::build_context`. The summary
// prompt sets a hard length expectation; the model handles the rest.
//
// Cache is intentionally OUT of scope for v0.10.0 Slice 5 — the front-end
// memoizes per session (no point burning a write to disk for a 10s call
// that's free on local Ollama). The proposal calls for a sqlite cache
// later if we add semantic search anyway (Slice 6+).

use super::chat::build_context;
use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Summary length the user picked. Drives the prompt instruction; doesn't
/// hard-cap the model output — we trust the model + temperature here.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SummaryLength {
    /// One sentence. For the navbar "what is this?" use case.
    Tldr,
    /// One paragraph. The default Beacon "summarize" action.
    #[default]
    Short,
    /// Five bullets. For the user who actually wants to skim.
    Long,
}

fn length_instruction(len: SummaryLength) -> &'static str {
    match len {
        SummaryLength::Tldr => "Reply with a SINGLE SENTENCE summary. No preamble.",
        SummaryLength::Short => {
            "Reply with ONE concise paragraph (3-5 sentences). No bullet points."
        }
        SummaryLength::Long => {
            "Reply with EXACTLY FIVE bullet points, each starting with '- '. \
             Cover the most important findings, audience, and structure."
        }
    }
}

/// What the front-end gets back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconSummary {
    /// Plain-text summary content.
    pub content: String,
    /// Model that produced it.
    pub model: String,
    /// Length the caller asked for — echoed so the UI can label the cache.
    pub length: SummaryLength,
    /// Number of pages we actually inlined (truncates from the tail).
    pub pages_used: u32,
    /// Number of pages in the PDF.
    pub pages_total: u32,
}

/// Build the prompt vec for a summary call. Pulled out so the test can pin it.
fn build_messages(
    pages: &[String],
    length: SummaryLength,
    max_chars: usize,
) -> (Vec<ChatMessage>, u32) {
    let instruction = length_instruction(length);
    let question = format!(
        "Summarize the PDF above. {instruction} \
         Stay strictly within the PDF's content — do not speculate."
    );
    let (user_block, pages_used, _chars_used, _chars_total) =
        build_context(pages, &question, max_chars);
    let msgs = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "You are Beacon, a PDF summarization assistant. Be precise. \
                      Do not include phrases like 'this PDF' or 'the document' — \
                      just state the facts."
                .to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: user_block,
        },
    ];
    (msgs, pages_used)
}

/// Run a Beacon summary turn against pre-extracted page text. Pure for
/// easy testing — see `beacon_summary_from_path` for the path-loading
/// wrapper.
pub async fn beacon_summary(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    length: SummaryLength,
    max_context_chars: usize,
) -> Result<BeaconSummary, AiError> {
    let (msgs, pages_used) = build_messages(pages, length, max_context_chars);
    let opts = ChatOpts {
        // Summaries want determinism. 0.1 keeps the model from hallucinating
        // facts while still letting it pick natural phrasing.
        temperature: Some(0.1),
        max_tokens: Some(match length {
            SummaryLength::Tldr => 80,
            SummaryLength::Short => 400,
            SummaryLength::Long => 800,
        }),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &opts).await?;
    Ok(BeaconSummary {
        content: resp.content,
        model: resp.model,
        length,
        pages_used,
        pages_total: pages.len() as u32,
    })
}

/// Convenience wrapper: read PDF text from disk, then summarize.
pub async fn beacon_summary_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    length: SummaryLength,
    max_context_chars: usize,
) -> Result<BeaconSummary, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    beacon_summary(provider, &pages, length, max_context_chars).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockProvider {
        captured: Mutex<Vec<ChatMessage>>,
        captured_opts: Mutex<Option<ChatOpts>>,
        reply: String,
    }

    impl MockProvider {
        fn new(reply: impl Into<String>) -> Self {
            Self {
                captured: Mutex::new(Vec::new()),
                captured_opts: Mutex::new(None),
                reply: reply.into(),
            }
        }
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            msgs: &[ChatMessage],
            opts: &ChatOpts,
        ) -> Result<super::super::ChatResponse, AiError> {
            *self.captured.lock().unwrap() = msgs.to_vec();
            *self.captured_opts.lock().unwrap() = Some(opts.clone());
            Ok(super::super::ChatResponse {
                content: self.reply.clone(),
                model: "mock-sum:test".into(),
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
    async fn summary_emits_provider_response() {
        let provider = Arc::new(MockProvider::new("This is the summary."));
        let pages = vec!["chapter one".to_string(), "chapter two".to_string()];
        let s = beacon_summary(provider, &pages, SummaryLength::Short, 5_000)
            .await
            .unwrap();
        assert_eq!(s.content, "This is the summary.");
        assert_eq!(s.length, SummaryLength::Short);
        assert_eq!(s.pages_total, 2);
        assert_eq!(s.pages_used, 2);
    }

    #[tokio::test]
    async fn summary_includes_length_instruction_in_prompt() {
        let provider = Arc::new(MockProvider::new("ok"));
        let pages = vec!["body".to_string()];
        let _ = beacon_summary(provider.clone(), &pages, SummaryLength::Long, 5_000)
            .await
            .unwrap();
        let captured = provider.captured.lock().unwrap().clone();
        let user_msg = &captured
            .iter()
            .find(|m| m.role == ChatRole::User)
            .unwrap()
            .content;
        assert!(
            user_msg.contains("FIVE bullet points"),
            "long summary should hint at 5 bullets, got {user_msg}"
        );
    }

    #[tokio::test]
    async fn summary_tldr_uses_tighter_token_budget() {
        let provider = Arc::new(MockProvider::new("ok"));
        let pages = vec!["body".to_string()];
        let _ = beacon_summary(provider.clone(), &pages, SummaryLength::Tldr, 5_000)
            .await
            .unwrap();
        let opts = provider.captured_opts.lock().unwrap().clone().unwrap();
        assert_eq!(opts.max_tokens, Some(80));
        // Long mode for contrast:
        let _ = beacon_summary(provider.clone(), &pages, SummaryLength::Long, 5_000)
            .await
            .unwrap();
        let opts2 = provider.captured_opts.lock().unwrap().clone().unwrap();
        assert_eq!(opts2.max_tokens, Some(800));
    }

    #[tokio::test]
    async fn summary_uses_low_temperature() {
        let provider = Arc::new(MockProvider::new("ok"));
        let _ = beacon_summary(
            provider.clone(),
            &["body".to_string()],
            SummaryLength::Short,
            5_000,
        )
        .await
        .unwrap();
        let opts = provider.captured_opts.lock().unwrap().clone().unwrap();
        let t = opts.temperature.unwrap();
        assert!(
            t <= 0.2,
            "summary temp should be deterministic-ish, got {t}"
        );
    }

    #[test]
    fn length_default_is_short() {
        assert_eq!(SummaryLength::default(), SummaryLength::Short);
    }
}
