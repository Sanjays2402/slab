// Beacon Chat — Q&A against the currently-open PDF.
//
// Design: the front-end hands us a PDF path, the user's question, and any
// prior turns. We:
//   1. Extract per-page text from the PDF (lopdf, already in `pdf::extract`).
//   2. Build a prompt that:
//        - tells the model it's a PDF assistant,
//        - tells it to cite pages with `[pN]` markers,
//        - inlines the PDF text as `<page N>...</page N>` blocks,
//        - includes the user's prior turns + the new question.
//   3. Truncate the inlined text to fit within `max_context_chars` so we
//      don't blow the model's context window. Page-aware: we never split
//      mid-page, we just drop trailing pages.
//   4. Call the configured `AiProvider`.
//   5. Parse the response text for `[pN]` citations and return the union.
//
// Buffered (non-streaming) for v0.10.0. Streaming arrives once we wire the
// Tauri event channel in v0.10.1 — the IO trait + parsing logic here stays
// intact, the only delta is the response shape.
//
// All HTTP and FS surfaces are stubbed by tests via a hand-rolled
// `MockProvider` rather than hitting real Ollama. The text-extraction
// path uses an in-memory `&[String]` injection point (`build_context`)
// so we don't need a real PDF either.

use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

/// Default soft ceiling on inlined PDF text. ~30K chars ≈ 7K tokens —
/// comfortable for a 3B local model and for OpenAI's 4o-mini (128K) alike.
/// Users can override per-call from the front-end if their model is bigger.
pub const DEFAULT_MAX_CONTEXT_CHARS: usize = 30_000;

/// Reply from the chat backend. Shape designed for the Svelte chat panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconChatReply {
    /// The assistant's full message text (already includes inline `[pN]`
    /// citation markers; the front-end can keep them or render them as chips).
    pub content: String,
    /// The model name reported by the provider (e.g. `"llama3.2:3b"`).
    pub model: String,
    /// Sorted, deduplicated page numbers referenced in `content`. 1-indexed.
    pub pages_cited: Vec<u32>,
    /// How many characters of PDF text we actually fed the model. Useful
    /// for the UI to render "answered using 12,400 / 87,000 characters
    /// of context".
    pub context_chars_used: u32,
    /// Total characters available in the PDF. Lets the UI show how much
    /// was truncated.
    pub context_chars_total: u32,
    /// How many pages the PDF has. Lets the UI render "answered using
    /// pages 1-8 of 42".
    pub pages_total: u32,
    /// How many pages worth of text were actually inlined into the
    /// prompt. Pages drop from the end first.
    pub pages_used: u32,
}

/// Build the system prompt. Pulled out so unit tests can pin the wording.
fn system_prompt() -> &'static str {
    "You are Beacon, an assistant embedded inside the Slab PDF reader. \
     You answer questions about the PDF below. \
     Rules:\n\
     - Only use facts from the PDF; if it doesn't say, say so.\n\
     - Cite the source by writing [pN] right after each fact, where N is the page number.\n\
     - Be concise. Plain text, no markdown headers.\n\
     - If the user asks about something the PDF doesn't cover, say \"The document doesn't say.\""
}

/// Assemble the user message that ferries the PDF text alongside the
/// question. We pack pages in order; truncate from the tail. Returns the
/// (assembled text, pages_used_count, chars_used_count, chars_total_count).
pub fn build_context(
    pages: &[String],
    question: &str,
    max_chars: usize,
) -> (String, u32, u32, u32) {
    let total_chars: u32 = pages.iter().map(|p| p.len() as u32).sum();
    let mut buf = String::with_capacity(max_chars.min(64 * 1024) + 256);
    buf.push_str("PDF CONTENT:\n");
    let mut pages_used: u32 = 0;
    let mut chars_used: u32 = 0;
    // Reserve some headroom for the question and the wrapper.
    let header_overhead = "PDF CONTENT:\n".len() + "\n\nQUESTION:\n".len() + question.len() + 64; // wrapper chars per page <page N>...</page N>
    let budget = max_chars.saturating_sub(header_overhead);
    for (i, text) in pages.iter().enumerate() {
        let page_no = (i as u32) + 1;
        let wrapped = format!("<page {page_no}>\n{}\n</page {page_no}>\n", text.trim_end());
        if chars_used as usize + wrapped.len() > budget {
            // Stop — including this page would blow the budget. We keep
            // pages strictly in document order, never splitting mid-page,
            // so this is "first N pages that fit".
            break;
        }
        buf.push_str(&wrapped);
        chars_used += wrapped.len() as u32;
        pages_used += 1;
    }
    buf.push_str("\nQUESTION:\n");
    buf.push_str(question);
    (buf, pages_used, chars_used, total_chars)
}

/// Extract every `[pN]` (or `[p N]`, `[pages 3, 4]`, `[page 7]`) reference
/// from the assistant text. Returns a sorted, deduplicated list of page
/// numbers. Stays liberal in what it accepts so model output style doesn't
/// matter — as long as the digit is bracketed and prefixed with `p`/`page`
/// we'll catch it.
pub fn extract_citations(text: &str) -> Vec<u32> {
    // Hand-rolled scan instead of `regex` so the parser is allocation-cheap
    // and the rules are obvious from the code.
    let bytes = text.as_bytes();
    let mut out: BTreeSet<u32> = BTreeSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Scan to the matching `]` or 64 chars, whichever first.
            let end = bytes[i + 1..]
                .iter()
                .position(|&b| b == b']')
                .map(|p| i + 1 + p)
                .unwrap_or(bytes.len());
            if end <= bytes.len() {
                let inner = &text[i + 1..end];
                collect_citation_numbers(inner, &mut out);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

fn collect_citation_numbers(inner: &str, sink: &mut BTreeSet<u32>) {
    let lower = inner.to_ascii_lowercase();
    // Must start with `p` or `page` to count as a citation — guards against
    // false positives like `[1]` (footnote markers) or `[2026-05-01]`.
    let rest = if let Some(r) = lower.strip_prefix("pages") {
        r
    } else if let Some(r) = lower.strip_prefix("page") {
        r
    } else if let Some(r) = lower.strip_prefix("pp.") {
        r
    } else if let Some(r) = lower.strip_prefix("pp") {
        r
    } else if let Some(r) = lower.strip_prefix('p') {
        r
    } else {
        return;
    };
    // Now scan `rest` for digit runs.
    let mut cur: u32 = 0;
    let mut in_num = false;
    for c in rest.chars() {
        if let Some(d) = c.to_digit(10) {
            cur = cur.saturating_mul(10).saturating_add(d);
            in_num = true;
        } else if in_num {
            sink.insert(cur);
            cur = 0;
            in_num = false;
        }
    }
    if in_num {
        sink.insert(cur);
    }
}

/// Run a Beacon chat turn. `pages` is the per-page text already extracted
/// from the PDF (so this function stays pure & easy to test). The Tauri
/// command wrapper handles the actual lopdf extraction.
pub async fn beacon_chat(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    question: &str,
    history: &[ChatMessage],
    max_context_chars: usize,
) -> Result<BeaconChatReply, AiError> {
    let (user_block, pages_used, chars_used, chars_total) =
        build_context(pages, question, max_context_chars);

    // Build the message vec: system → history → fresh user-with-context.
    let mut msgs: Vec<ChatMessage> = Vec::with_capacity(history.len() + 2);
    msgs.push(ChatMessage {
        role: ChatRole::System,
        content: system_prompt().to_string(),
    });
    for m in history {
        msgs.push(m.clone());
    }
    msgs.push(ChatMessage {
        role: ChatRole::User,
        content: user_block,
    });

    let opts = ChatOpts {
        temperature: Some(0.2),
        max_tokens: Some(800),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &opts).await?;
    let pages_cited = extract_citations(&resp.content);
    Ok(BeaconChatReply {
        content: resp.content,
        model: resp.model,
        pages_cited,
        context_chars_used: chars_used,
        context_chars_total: chars_total,
        pages_total: pages.len() as u32,
        pages_used,
    })
}

/// Convenience wrapper: read the PDF from disk, call `beacon_chat`, return
/// the reply. This is what the Tauri command surface hits.
pub async fn beacon_chat_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    question: &str,
    history: &[ChatMessage],
    max_context_chars: usize,
) -> Result<BeaconChatReply, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    beacon_chat(provider, &pages, question, history, max_context_chars).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// In-memory provider for tests. Captures the messages it was given
    /// and returns a canned response. No HTTP, no real models.
    struct MockProvider {
        captured: Mutex<Vec<ChatMessage>>,
        reply: String,
    }

    impl MockProvider {
        fn new(reply: impl Into<String>) -> Self {
            Self {
                captured: Mutex::new(Vec::new()),
                reply: reply.into(),
            }
        }
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<super::super::ChatResponse, AiError> {
            *self.captured.lock().unwrap() = msgs.to_vec();
            Ok(super::super::ChatResponse {
                content: self.reply.clone(),
                model: "mock-model:test".into(),
            })
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            unimplemented!("not exercised in chat tests")
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[test]
    fn build_context_inlines_pages_in_order() {
        let pages = vec![
            "page one body".to_string(),
            "page two body".to_string(),
            "page three body".to_string(),
        ];
        let (ctx, used, _chars, _total) = build_context(&pages, "What does it say?", 5_000);
        assert_eq!(used, 3);
        assert!(ctx.contains("<page 1>"));
        assert!(ctx.contains("<page 2>"));
        assert!(ctx.contains("<page 3>"));
        assert!(ctx.contains("QUESTION:\nWhat does it say?"));
        // Order matters: page 1 appears before page 2 in the buffer.
        let p1 = ctx.find("<page 1>").unwrap();
        let p2 = ctx.find("<page 2>").unwrap();
        let p3 = ctx.find("<page 3>").unwrap();
        assert!(p1 < p2 && p2 < p3);
    }

    #[test]
    fn build_context_truncates_to_budget() {
        // 5 pages of ~200 chars each = ~1K chars; budget of 350 chars
        // should land ~1-2 pages.
        let pages: Vec<String> = (0..5).map(|_| "x".repeat(200)).collect();
        let (ctx, used, chars_used, chars_total) = build_context(&pages, "Q?", 350);
        assert!(
            used <= 2,
            "expected ≤2 pages with 350-char budget, got {used}"
        );
        assert!(used >= 1, "expected ≥1 page to still fit, got {used}");
        assert!(chars_used as usize <= 350);
        assert_eq!(chars_total, 1000); // 5 * 200
        assert!(ctx.contains("QUESTION:\nQ?"));
    }

    #[test]
    fn build_context_drops_pages_from_the_end() {
        // Distinct content per page → easy assertion on which page survived.
        // Pad each marker so the budget actually forces a drop.
        let pages = vec![
            format!("FIRST_MARKER {}", "a".repeat(120)),
            format!("SECOND_MARKER {}", "b".repeat(120)),
            format!("THIRD_MARKER {}", "c".repeat(120)),
        ];
        let (ctx, used, _, _) = build_context(&pages, "q", 350);
        // Budget too small for all 3 — but the first one should make it.
        assert!(ctx.contains("FIRST_MARKER"));
        // We should have NOT included the last page.
        assert!(
            !ctx.contains("THIRD_MARKER"),
            "third page should have been dropped"
        );
        assert!((1..3).contains(&used), "expected 1 or 2 pages, got {used}");
    }

    #[test]
    fn extract_citations_handles_common_formats() {
        let cases = vec![
            ("This is on [p3].", vec![3]),
            ("See [page 7] and [page 12].", vec![7, 12]),
            ("Multiple pages [pages 2, 5, 9] mentioned.", vec![2, 5, 9]),
            ("Out of order: [p9] then [p2].", vec![2, 9]),
            ("Duplicates: [p4] and [p4] and [page 4].", vec![4]),
            ("No citations here.", vec![]),
            // False-positive guards:
            ("Footnote [1] is not a citation.", vec![]),
            ("Date [2026-05-01] is not a citation.", vec![]),
        ];
        for (input, want) in cases {
            let got = extract_citations(input);
            assert_eq!(got, want, "input was {input:?}");
        }
    }

    #[tokio::test]
    async fn beacon_chat_end_to_end_with_mock_provider() {
        let pages = vec![
            "The first chapter introduces the topic.".to_string(),
            "The second chapter dives into the details.".to_string(),
        ];
        let provider = Arc::new(MockProvider::new(
            "The PDF is about a topic [p1] with details in chapter 2 [p2].",
        ));
        let reply = beacon_chat(
            provider.clone(),
            &pages,
            "What's the PDF about?",
            &[],
            10_000,
        )
        .await
        .unwrap();
        assert_eq!(reply.model, "mock-model:test");
        assert_eq!(reply.pages_cited, vec![1, 2]);
        assert_eq!(reply.pages_total, 2);
        assert_eq!(reply.pages_used, 2);
        assert!(reply.context_chars_used > 0);
        assert!(reply.content.contains("The PDF is about a topic [p1]"));
    }

    #[tokio::test]
    async fn beacon_chat_forwards_history_in_order() {
        let provider = Arc::new(MockProvider::new("ack"));
        let history = vec![
            ChatMessage {
                role: ChatRole::User,
                content: "earlier user turn".into(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "earlier assistant turn".into(),
            },
        ];
        let _ = beacon_chat(
            provider.clone(),
            &["only page".to_string()],
            "new question",
            &history,
            5_000,
        )
        .await
        .unwrap();
        let captured = provider.captured.lock().unwrap().clone();
        // [system, hist-user, hist-assistant, new-user]
        assert_eq!(captured.len(), 4);
        assert_eq!(captured[0].role, ChatRole::System);
        assert_eq!(captured[1].role, ChatRole::User);
        assert_eq!(captured[1].content, "earlier user turn");
        assert_eq!(captured[2].role, ChatRole::Assistant);
        assert_eq!(captured[2].content, "earlier assistant turn");
        assert_eq!(captured[3].role, ChatRole::User);
        assert!(captured[3].content.contains("new question"));
        assert!(captured[3].content.contains("<page 1>"));
    }

    #[tokio::test]
    async fn beacon_chat_sets_system_prompt() {
        let provider = Arc::new(MockProvider::new("ok"));
        let _ = beacon_chat(provider.clone(), &["body".to_string()], "hi", &[], 5_000)
            .await
            .unwrap();
        let captured = provider.captured.lock().unwrap().clone();
        assert_eq!(captured[0].role, ChatRole::System);
        assert!(captured[0].content.contains("Beacon"));
        assert!(captured[0].content.contains("[pN]"));
    }
}
