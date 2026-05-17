//! Beacon Auto-Tag — given a PDF (or its pre-extracted page text), ask the
//! configured `AiProvider` to suggest 3–5 short topical tags, then return a
//! cleaned `Vec<String>` ready to hand to `pdf::library::registry::add_tag`.
//!
//! Why a separate module: tagging is a one-shot, non-streamed, deterministic
//! call. It shares zero state with chat / summary / vision so a tiny pure
//! parser + a 30-line orchestrator is the whole module. The clean-up logic
//! (lower-case, strip bullets / quotes / numbers, dedupe, length-clamp) is
//! pulled into `parse_tag_reply` so we can unit-test it without touching a
//! mock provider.
//!
//! Prompt contract: the model returns ONLY a comma-separated list of
//! lower-case tags. We *don't* trust the model — every reply goes through
//! `parse_tag_reply` and a hard length cap.

use crate::ai::chat::build_context;
use crate::ai::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Default soft ceiling on inlined PDF text for tag prompts. Tag generation
/// needs less context than a Q&A turn — the first few pages usually contain
/// the title + abstract + intro, which is plenty. 6K chars ≈ 1.5K tokens.
pub const DEFAULT_MAX_CONTEXT_CHARS: usize = 6_000;

/// Hard ceiling on the length (in chars) of any single tag. Anything past
/// this is almost certainly the model returning a sentence by mistake; we
/// drop it rather than store a 200-char "tag" in the library.
const MAX_TAG_LEN_CHARS: usize = 32;

/// Absolute floor + ceiling on `max_tags`. Below 1 there's no point;
/// above 10 the tags get noisy and stop being useful.
const MIN_TAGS: u32 = 1;
const MAX_TAGS: u32 = 10;

/// User-tunable knobs for an auto-tag call. `Default::default()` is what
/// 99% of UI invocations send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTagOpts {
    /// Target tag count. Clamped to `1..=10` internally.
    #[serde(default = "default_max_tags")]
    pub max_tags: u32,
    /// Soft ceiling on inlined PDF text length (chars).
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
}

fn default_max_tags() -> u32 {
    5
}
fn default_max_context_chars() -> usize {
    DEFAULT_MAX_CONTEXT_CHARS
}

impl Default for AutoTagOpts {
    fn default() -> Self {
        Self {
            max_tags: default_max_tags(),
            max_context_chars: default_max_context_chars(),
        }
    }
}

/// Final shape surfaced to the front-end (and to the library orchestrator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTagResult {
    /// Cleaned, deduplicated, length-clamped tags. Always lower-case.
    pub tags: Vec<String>,
    /// Model identifier reported by the provider.
    pub model: String,
    /// Number of pages we actually inlined (truncates from the tail).
    pub pages_used: u32,
    /// Number of pages in the PDF (or source slice).
    pub pages_total: u32,
}

const SYSTEM_PROMPT: &str =
    "You are a precise PDF tag generator. You read documents and produce a \
     short, comma-separated list of topical tags. Tags are lower-case, 1-3 \
     words, and concrete. Do not explain. Do not number. Do not bullet. \
     Return ONLY the comma-separated list.";

/// Parse a raw LLM reply into a cleaned tag list.
///
/// Steps, in order:
/// 1. Split on commas (the prompt promises CSV).
/// 2. Strip leading bullet markers (`-`, `*`, `•`, digits + `.` or `)`).
/// 3. Strip wrapping quotes/backticks.
/// 4. Lower-case + trim whitespace.
/// 5. Drop empty / whitespace-only / overlong (> MAX_TAG_LEN_CHARS) tags.
/// 6. Dedupe while preserving first-seen order.
/// 7. Cap at `max_tags` (clamped to `MIN_TAGS..=MAX_TAGS`).
pub fn parse_tag_reply(reply: &str, max_tags: u32) -> Vec<String> {
    let cap = max_tags.clamp(MIN_TAGS, MAX_TAGS) as usize;
    let mut out: Vec<String> = Vec::with_capacity(cap);
    for raw in reply.split(',') {
        let cleaned = clean_one_tag(raw);
        if cleaned.is_empty() {
            continue;
        }
        if cleaned.chars().count() > MAX_TAG_LEN_CHARS {
            continue;
        }
        if !out.iter().any(|t| t == &cleaned) {
            out.push(cleaned);
            if out.len() >= cap {
                break;
            }
        }
    }
    out
}

/// Strip bullets, quotes, numeric list prefixes; lower-case + trim.
/// Internal helper for `parse_tag_reply`; not part of the public API.
fn clean_one_tag(raw: &str) -> String {
    // Step 1: trim outer whitespace + newlines.
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return s;
    }
    // Step 2: strip leading bullet/number prefixes. Iterate because some
    // models like to chain (e.g. "1. - tag").
    loop {
        let before = s.clone();
        // bullet chars
        s = s
            .trim_start_matches(['-', '*', '•', '·', '–', '—'])
            .to_string();
        // numeric "1." / "1)" / "1:"
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                let after_digits = s.trim_start_matches(|c: char| c.is_ascii_digit());
                if let Some(c2) = after_digits.chars().next() {
                    if c2 == '.' || c2 == ')' || c2 == ':' {
                        s = after_digits[c2.len_utf8()..].to_string();
                    }
                }
            }
        }
        s = s.trim_start().to_string();
        if s == before {
            break;
        }
    }
    // Step 3: strip wrapping quotes/backticks.
    let trimmable: &[char] = &['"', '\'', '`'];
    while s.starts_with(trimmable) {
        s = s[1..].to_string();
    }
    while s.ends_with(trimmable) {
        s.pop();
    }
    // Step 4: lower-case + collapse internal whitespace runs to single spaces.
    let lowered = s.trim().to_lowercase();
    let mut collapsed = String::with_capacity(lowered.len());
    let mut prev_space = false;
    for c in lowered.chars() {
        if c.is_whitespace() {
            if !prev_space && !collapsed.is_empty() {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    collapsed.trim_end().to_string()
}

/// Build the chat messages vec for an auto-tag call. Pulled out so tests
/// can pin the prompt shape without invoking the provider.
fn build_messages(pages: &[String], opts: &AutoTagOpts) -> (Vec<ChatMessage>, u32) {
    let n = opts.max_tags.clamp(MIN_TAGS, MAX_TAGS);
    let question = format!(
        "Produce exactly {n} short topical tags for the document above, lower-case, \
         comma-separated. No commentary."
    );
    let (user_block, pages_used, _chars_used, _chars_total) =
        build_context(pages, &question, opts.max_context_chars);
    let msgs = vec![
        ChatMessage {
            role: ChatRole::System,
            content: SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: user_block,
        },
    ];
    (msgs, pages_used)
}

/// Run an auto-tag turn against pre-extracted page text. Pure for easy
/// testing — see `auto_tag_from_path` for the path-loading wrapper.
pub async fn auto_tag(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    opts: &AutoTagOpts,
) -> Result<AutoTagResult, AiError> {
    let (msgs, pages_used) = build_messages(pages, opts);
    let chat_opts = ChatOpts {
        // Tag generation wants determinism + concision. 0.1 keeps the model
        // from inventing wild categories; max_tokens=64 keeps it from
        // turning the reply into prose.
        temperature: Some(0.1),
        max_tokens: Some(64),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &chat_opts).await?;
    let tags = parse_tag_reply(&resp.content, opts.max_tags);
    Ok(AutoTagResult {
        tags,
        model: resp.model,
        pages_used,
        pages_total: pages.len() as u32,
    })
}

/// Convenience wrapper: extract PDF text from disk, then auto-tag.
pub async fn auto_tag_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    opts: &AutoTagOpts,
) -> Result<AutoTagResult, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    auto_tag(provider, &pages, opts).await
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatResponse;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // -- parse_tag_reply ------------------------------------------------

    #[test]
    fn parse_handles_comma_separated() {
        let v = parse_tag_reply("finance, machine learning, ai", 5);
        assert_eq!(v, vec!["finance", "machine learning", "ai"]);
    }

    #[test]
    fn parse_lowercases_and_trims() {
        let v = parse_tag_reply("  Finance ,  AI/ML \n", 5);
        assert_eq!(v, vec!["finance", "ai/ml"]);
    }

    #[test]
    fn parse_dedupes_preserving_order() {
        let v = parse_tag_reply("a, b, A, c, b", 5);
        assert_eq!(v, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_drops_overlong() {
        // 50 'x' chars > MAX_TAG_LEN_CHARS (32)
        let long = "x".repeat(50);
        let input = format!("ok, {long}, also-ok");
        let v = parse_tag_reply(&input, 5);
        assert_eq!(v, vec!["ok", "also-ok"]);
    }

    #[test]
    fn parse_caps_at_max() {
        let v = parse_tag_reply("a,b,c,d,e,f,g,h", 3);
        assert_eq!(v, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_strips_bullets_quotes_numbers() {
        // model goes off-script and bullets/numbers/quotes the list
        let reply = r#"1. "law", - tax, • compliance, 2) "risk""#;
        let v = parse_tag_reply(reply, 5);
        assert_eq!(v, vec!["law", "tax", "compliance", "risk"]);
    }

    #[test]
    fn parse_drops_empty_and_whitespace_only() {
        let v = parse_tag_reply(", ,a,  ,", 5);
        assert_eq!(v, vec!["a"]);
    }

    // -- auto_tag orchestrator (mock provider) -------------------------

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
        ) -> Result<ChatResponse, AiError> {
            *self.captured.lock().unwrap() = msgs.to_vec();
            Ok(ChatResponse {
                content: self.reply.clone(),
                model: "mock-tag:test".into(),
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
    async fn auto_tag_emits_provider_response() {
        let provider = Arc::new(MockProvider::new("x, y, z"));
        let pages = vec!["abstract".to_string(), "body".to_string()];
        let r = auto_tag(provider.clone(), &pages, &AutoTagOpts::default())
            .await
            .unwrap();
        assert_eq!(r.tags, vec!["x", "y", "z"]);
        assert_eq!(r.model, "mock-tag:test");
        assert_eq!(r.pages_total, 2);
        assert_eq!(r.pages_used, 2);
        // System + user only — no history wired for auto-tag.
        let captured = provider.captured.lock().unwrap().clone();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].role, ChatRole::System);
        assert_eq!(captured[1].role, ChatRole::User);
    }
}
