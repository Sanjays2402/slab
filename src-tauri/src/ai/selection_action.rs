// Beacon Selection Actions — quick LLM transforms on a text snippet the
// user highlighted in the PDF reader.
//
// Design: the front-end captures the user's selection (via `window.getSelection()`
// on the PDF.js text layer), pops a small floating bubble with five buttons
// (Translate / Explain / Define / Rewrite / Summarize), and the user picks one.
// The bubble fires the Tauri command, which calls the configured `AiProvider`
// and returns a plain-text reply that gets rendered inline.
//
// Why not reuse `chat::beacon_chat`? Chat builds a 30K-char PDF context — way
// too heavy for a 60-char "translate this sentence" call. Selection actions
// only need the snippet itself plus a tightly-scoped action prompt.
//
// All five actions share:
//   - system prompt enforcing terse, plain-text output (no preamble)
//   - low temperature (0.2) for stable rewrites
//   - tight max_tokens budget (snippet-sized)
//
// Tests use the same in-memory `MockProvider` pattern as `chat.rs` / `summary.rs` —
// no HTTP, no real Ollama.

use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What the user wants done to the selected text. Five v0.10.0 actions —
/// "Custom" is a v0.11 stretch and intentionally omitted here.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SelectionAction {
    /// Translate into `target_lang` (default: English).
    Translate,
    /// Explain the passage in plain English, like the user is a smart-but-busy adult.
    Explain,
    /// Define a single term or short phrase.
    Define,
    /// Rewrite the passage more clearly. No factual changes, just diction.
    Rewrite,
    /// One-sentence TL;DR of the selection. Distinct from full-doc summary.
    Summarize,
}

impl SelectionAction {
    /// Stable identifier for logging. Matches the Svelte enum string.
    pub fn slug(self) -> &'static str {
        match self {
            SelectionAction::Translate => "translate",
            SelectionAction::Explain => "explain",
            SelectionAction::Define => "define",
            SelectionAction::Rewrite => "rewrite",
            SelectionAction::Summarize => "summarize",
        }
    }

    /// User-visible label for the bubble button. Title case.
    pub fn label(self) -> &'static str {
        match self {
            SelectionAction::Translate => "Translate",
            SelectionAction::Explain => "Explain",
            SelectionAction::Define => "Define",
            SelectionAction::Rewrite => "Rewrite",
            SelectionAction::Summarize => "Summarize",
        }
    }

    /// Per-action token budget. Generous enough for full prose but bounded
    /// so a runaway model doesn't melt the local CPU.
    pub fn max_tokens(self) -> u32 {
        match self {
            SelectionAction::Define => 120,
            SelectionAction::Translate | SelectionAction::Rewrite => 400,
            SelectionAction::Explain => 500,
            SelectionAction::Summarize => 150,
        }
    }
}

/// What the front-end gets back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionActionReply {
    /// The transformed text. Already trimmed; no leading/trailing whitespace.
    pub content: String,
    /// Provider's reported model name.
    pub model: String,
    /// Echoes the action so the UI can pin the result to the right button.
    pub action: SelectionAction,
    /// Characters in the original selection. Useful for telemetry / UI ratio.
    pub input_chars: u32,
    /// Characters in the reply. Same.
    pub output_chars: u32,
}

/// Maximum selection size we'll forward to the LLM. We aren't trying to
/// translate a whole book in this bubble — long selections should use
/// the chat panel. Returns `InvalidResponse` if the selection is too big
/// (we re-use the error type rather than introducing another variant).
pub const MAX_SELECTION_CHARS: usize = 8_000;

/// Build the per-action system prompt. Pulled out so tests can pin wording.
fn system_prompt(action: SelectionAction, target_lang: Option<&str>) -> String {
    match action {
        SelectionAction::Translate => {
            let lang = target_lang.unwrap_or("English");
            format!(
                "You are a translator. Translate the user's text into {lang}. \
                 Reply with ONLY the translated text — no preamble, no explanation, \
                 no quotation marks around the result. Preserve formatting (line breaks, \
                 punctuation, lists) where reasonable."
            )
        }
        SelectionAction::Explain => "You are a teacher. Explain the user's passage in plain, \
             clear English. Imagine the reader is a smart adult who is short on \
             time and unfamiliar with the jargon. Reply with ONLY the explanation — \
             no preamble, no \"this passage means…\" framing. 2-4 sentences max."
            .to_string(),
        SelectionAction::Define => {
            "You are a dictionary. The user gives you a single word or short phrase. \
             Reply with ONLY the definition — a single concise sentence. \
             If the term is ambiguous, pick the most common modern meaning. \
             No preamble, no \"this term means…\" framing."
                .to_string()
        }
        SelectionAction::Rewrite => "You are an editor. Rewrite the user's text in clear, \
             plain English while preserving its meaning EXACTLY. \
             Do not add or remove facts. Do not change quoted content. \
             Reply with ONLY the rewritten text — no preamble, no commentary."
            .to_string(),
        SelectionAction::Summarize => "You are a summarizer. Reduce the user's text to a SINGLE \
             concise sentence that captures the main point. \
             Reply with ONLY that sentence — no preamble, no \"this says…\" framing."
            .to_string(),
    }
}

/// Run a selection-action turn. Pure function: caller passes the provider
/// and the text; no I/O beyond the LLM call.
///
/// Returns `InvalidResponse` if the selection is empty (whitespace only) or
/// exceeds `MAX_SELECTION_CHARS` — we want loud, actionable errors here
/// rather than wasting an LLM call on garbage input.
pub async fn run_selection_action(
    provider: Arc<dyn AiProvider>,
    text: &str,
    action: SelectionAction,
    target_lang: Option<String>,
) -> Result<SelectionActionReply, AiError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AiError::InvalidResponse(
            "selection is empty — highlight some text first".into(),
        ));
    }
    if trimmed.len() > MAX_SELECTION_CHARS {
        return Err(AiError::InvalidResponse(format!(
            "selection is {} chars — too big for the bubble (max {}). \
             Try the chat panel instead.",
            trimmed.len(),
            MAX_SELECTION_CHARS
        )));
    }
    let input_chars = trimmed.len() as u32;
    let msgs = vec![
        ChatMessage {
            role: ChatRole::System,
            content: system_prompt(action, target_lang.as_deref()),
        },
        ChatMessage {
            role: ChatRole::User,
            content: trimmed.to_string(),
        },
    ];
    let opts = ChatOpts {
        // Low but not zero — rewrites/explanations sound robotic at 0.
        temperature: Some(0.2),
        max_tokens: Some(action.max_tokens()),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &opts).await?;
    let content = resp.content.trim().to_string();
    let output_chars = content.len() as u32;
    Ok(SelectionActionReply {
        content,
        model: resp.model,
        action,
        input_chars,
        output_chars,
    })
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
                model: "mock-sel:test".into(),
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
    async fn selection_action_translate_uses_target_lang_in_prompt() {
        let p = Arc::new(MockProvider::new("Hola mundo"));
        let r = run_selection_action(
            p.clone(),
            "Hello world",
            SelectionAction::Translate,
            Some("Spanish".into()),
        )
        .await
        .unwrap();
        assert_eq!(r.content, "Hola mundo");
        assert_eq!(r.action, SelectionAction::Translate);
        let captured = p.captured.lock().unwrap().clone();
        let sys = &captured
            .iter()
            .find(|m| m.role == ChatRole::System)
            .unwrap()
            .content;
        assert!(
            sys.contains("Spanish"),
            "translate system prompt should name the target language, got {sys}"
        );
    }

    #[tokio::test]
    async fn selection_action_translate_defaults_to_english_when_none() {
        let p = Arc::new(MockProvider::new("Hello world"));
        let _ = run_selection_action(p.clone(), "Hola mundo", SelectionAction::Translate, None)
            .await
            .unwrap();
        let captured = p.captured.lock().unwrap().clone();
        let sys = &captured
            .iter()
            .find(|m| m.role == ChatRole::System)
            .unwrap()
            .content;
        assert!(
            sys.contains("English"),
            "translate w/o target_lang should default to English, got {sys}"
        );
    }

    #[tokio::test]
    async fn selection_action_explain_uses_explain_prompt() {
        let p = Arc::new(MockProvider::new("It means the heat death."));
        let _ = run_selection_action(
            p.clone(),
            "The entropy of an isolated system never decreases.",
            SelectionAction::Explain,
            None,
        )
        .await
        .unwrap();
        let captured = p.captured.lock().unwrap().clone();
        let sys = &captured[0].content;
        assert!(sys.contains("teacher") || sys.contains("Explain"));
        assert!(
            !sys.contains("translator"),
            "explain should not reuse translate prompt"
        );
    }

    #[tokio::test]
    async fn selection_action_rejects_empty_selection() {
        let p = Arc::new(MockProvider::new("anything"));
        let r = run_selection_action(p, "   \n\t  ", SelectionAction::Explain, None).await;
        assert!(matches!(r, Err(AiError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn selection_action_rejects_oversized_selection() {
        let p = Arc::new(MockProvider::new("anything"));
        let huge = "a".repeat(MAX_SELECTION_CHARS + 1);
        let r = run_selection_action(p, &huge, SelectionAction::Rewrite, None).await;
        match r {
            Err(AiError::InvalidResponse(msg)) => {
                assert!(msg.contains("too big") || msg.contains("max"));
            }
            other => panic!("expected oversized rejection, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn selection_action_trims_reply_whitespace() {
        let p = Arc::new(MockProvider::new("\n  trimmed text  \n"));
        let r = run_selection_action(p, "input", SelectionAction::Rewrite, None)
            .await
            .unwrap();
        assert_eq!(r.content, "trimmed text");
        assert_eq!(r.output_chars, "trimmed text".len() as u32);
    }

    #[tokio::test]
    async fn selection_action_uses_low_temperature_and_capped_tokens() {
        let p = Arc::new(MockProvider::new("brief"));
        let _ = run_selection_action(p.clone(), "term", SelectionAction::Define, None)
            .await
            .unwrap();
        let opts = p.captured_opts.lock().unwrap().clone().unwrap();
        let t = opts.temperature.unwrap();
        assert!(t <= 0.3, "selection actions should be low-temp, got {t}");
        // Define has the tightest budget.
        assert_eq!(opts.max_tokens, Some(SelectionAction::Define.max_tokens()));
        assert!(opts.max_tokens.unwrap() <= 200);
    }

    #[tokio::test]
    async fn selection_action_define_uses_dictionary_prompt() {
        let p = Arc::new(MockProvider::new("A unit of language."));
        let _ = run_selection_action(p.clone(), "morpheme", SelectionAction::Define, None)
            .await
            .unwrap();
        let captured = p.captured.lock().unwrap().clone();
        let sys = &captured[0].content;
        assert!(
            sys.contains("dictionary") || sys.contains("Definition") || sys.contains("definition")
        );
    }

    #[tokio::test]
    async fn selection_action_summarize_emits_one_sentence_prompt() {
        let p = Arc::new(MockProvider::new("Cats sleep a lot."));
        let _ = run_selection_action(
            p.clone(),
            "Cats sleep on average 15 hours a day. Some sleep more. Some less.",
            SelectionAction::Summarize,
            None,
        )
        .await
        .unwrap();
        let captured = p.captured.lock().unwrap().clone();
        let sys = &captured[0].content;
        assert!(
            sys.to_lowercase().contains("single") || sys.to_lowercase().contains("one sentence"),
            "summarize prompt should ask for one sentence, got {sys}"
        );
    }

    #[tokio::test]
    async fn selection_action_passes_user_text_verbatim() {
        let p = Arc::new(MockProvider::new("ok"));
        let snippet = "The quick brown fox jumps over the lazy dog.";
        let _ = run_selection_action(p.clone(), snippet, SelectionAction::Rewrite, None)
            .await
            .unwrap();
        let captured = p.captured.lock().unwrap().clone();
        let user = captured.iter().find(|m| m.role == ChatRole::User).unwrap();
        assert_eq!(user.content, snippet);
    }

    #[tokio::test]
    async fn selection_action_reply_carries_input_and_output_char_counts() {
        let p = Arc::new(MockProvider::new("twelve chars"));
        let r = run_selection_action(p, "abcdef", SelectionAction::Rewrite, None)
            .await
            .unwrap();
        assert_eq!(r.input_chars, 6);
        assert_eq!(r.output_chars, "twelve chars".len() as u32);
    }

    #[test]
    fn action_slugs_are_stable_and_match_serde() {
        assert_eq!(SelectionAction::Translate.slug(), "translate");
        assert_eq!(SelectionAction::Explain.slug(), "explain");
        assert_eq!(SelectionAction::Define.slug(), "define");
        assert_eq!(SelectionAction::Rewrite.slug(), "rewrite");
        assert_eq!(SelectionAction::Summarize.slug(), "summarize");
        // serde round-trip:
        let j = serde_json::to_string(&SelectionAction::Translate).unwrap();
        assert_eq!(j, "\"translate\"");
        let back: SelectionAction = serde_json::from_str(&j).unwrap();
        assert_eq!(back, SelectionAction::Translate);
    }

    #[test]
    fn action_labels_are_title_case() {
        for a in [
            SelectionAction::Translate,
            SelectionAction::Explain,
            SelectionAction::Define,
            SelectionAction::Rewrite,
            SelectionAction::Summarize,
        ] {
            let lbl = a.label();
            let first = lbl.chars().next().unwrap();
            assert!(
                first.is_uppercase(),
                "label {lbl:?} should be Title Case, got {first}"
            );
        }
    }
}
