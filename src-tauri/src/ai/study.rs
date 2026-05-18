// Beacon Study Mode — Q&A flashcards with SM-2-lite spaced repetition.
//
// Pipeline (Slice 13 of Beacon Bonus):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Chunk it with `ai::chunker::chunk_pages`.
//   3. For each chunk, ask the AiProvider to emit JSON `{cards:[{q,a}]}`
//      with a strict "answerable from THIS chunk alone" system prompt.
//   4. Validate (drop empty Q/A, drop too-long, dedupe by normalised Q).
//   5. Return `Vec<Flashcard>` tagged with origin `page`. Caller stores
//      them in `study_store` keyed by `pdf_hash`.
//
// Review side: `ai::sm2` schedules cards. `study_store` queries due cards.
// The frontend drives one card at a time, posting the user's ease rating
// back to `slab_beacon_study_review`.

use super::chunker::chunk_pages;
use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Hard cap on generated cards per `generate_deck` call.
pub const MAX_DECK_SIZE: usize = 200;

/// Per-Q/A length sanity check. Anything over this is almost always the
/// model misbehaving (rambling answer, multi-paragraph Q).
pub const MAX_FIELD_CHARS: usize = 1_000;

/// Default per-chunk card count we ask the model for. Tunable from the UI.
pub const DEFAULT_CARDS_PER_CHUNK: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Flashcard {
    pub page: u32,
    pub q: String,
    pub a: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckReport {
    pub cards: Vec<Flashcard>,
    /// Model identifier returned by the last provider call.
    pub model: String,
    /// How many chunks we asked the provider about.
    pub chunks_processed: u32,
    /// Cards dropped by validation (empty/too-long/dupes).
    pub dropped: u32,
}

/// Knobs for `generate_deck`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckOpts {
    /// How many cards to ask for per chunk. Clamped 1..=10.
    #[serde(default = "default_cpc")]
    pub cards_per_chunk: u32,
    /// Hard ceiling on total cards. Clamped to MAX_DECK_SIZE.
    #[serde(default = "default_max")]
    pub max_cards: u32,
}

fn default_cpc() -> u32 {
    DEFAULT_CARDS_PER_CHUNK
}
fn default_max() -> u32 {
    MAX_DECK_SIZE as u32
}

impl Default for DeckOpts {
    fn default() -> Self {
        Self {
            cards_per_chunk: DEFAULT_CARDS_PER_CHUNK,
            max_cards: MAX_DECK_SIZE as u32,
        }
    }
}

// ---------------------------------------------------------------
// LLM wire shape — liberal: every field optional.
// ---------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmCardsWire {
    #[serde(default)]
    pub(super) cards: Vec<LlmCardWire>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmCardWire {
    #[serde(default)]
    pub(super) q: Option<String>,
    #[serde(default)]
    pub(super) a: Option<String>,
}

/// Liberal JSON parser — same pattern as `outline::parse_llm_outline`.
pub(super) fn parse_llm_cards(raw: &str) -> Option<LlmCardsWire> {
    let s = raw.trim();
    let body = if let Some(rest) = s.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = s.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        s
    };
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&body[start..=end]).ok()
}

/// Whitespace-collapsed lowercase Q for dedupe. Local copy of the store's
/// normaliser to avoid coupling this module to the store at the type level.
fn normalise_q(q: &str) -> String {
    q.split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Validate + dedupe + cap. Pure — no IO.
pub fn validate_cards(raw: Vec<Flashcard>) -> Vec<Flashcard> {
    let mut out: Vec<Flashcard> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in raw {
        let q = c.q.trim().to_string();
        let a = c.a.trim().to_string();
        if q.is_empty() || a.is_empty() {
            continue;
        }
        if q.len() > MAX_FIELD_CHARS || a.len() > MAX_FIELD_CHARS {
            continue;
        }
        let n = normalise_q(&q);
        if !seen.insert(n) {
            continue;
        }
        out.push(Flashcard { page: c.page, q, a });
        if out.len() >= MAX_DECK_SIZE {
            break;
        }
    }
    out
}

const SYSTEM_PROMPT: &str = "You are Beacon, a study-buddy. Given a chunk of \
PDF text, write Q&A flashcards that test the reader's recall of the chunk's \
content. Reply with JSON ONLY, no prose, no markdown fences, in this exact \
shape:\n\
{\"cards\":[{\"q\":\"...\",\"a\":\"...\"}]}\n\
- q: a short, specific question. Answerable from the chunk ALONE.\n\
- a: a 1-2 sentence answer. Direct, no hedging.\n\
- Skip headings, page numbers, table-of-contents lines.\n\
- If the chunk has nothing testworthy, return {\"cards\":[]}. Don't invent.";

fn build_messages(chunk_text: &str, cards_per_chunk: u32) -> Vec<ChatMessage> {
    let cards_per_chunk = cards_per_chunk.clamp(1, 10);
    let user = format!(
        "Write up to {cards_per_chunk} flashcards from this chunk. JSON only.\n\
         \n\
         CHUNK:\n{chunk_text}"
    );
    vec![
        ChatMessage {
            role: ChatRole::System,
            content: SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: user,
        },
    ]
}

/// Top-level pipeline. Pre-extracted pages so it's easy to unit-test
/// against fixture strings (the real Tauri command calls
/// `generate_deck_from_path` below).
pub async fn generate_deck(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    opts: &DeckOpts,
) -> Result<DeckReport, AiError> {
    let chunks = chunk_pages(pages);
    let cap = (opts.max_cards as usize).min(MAX_DECK_SIZE);
    let cards_per_chunk = opts.cards_per_chunk.clamp(1, 10);

    let mut all: Vec<Flashcard> = Vec::new();
    let mut model = String::new();
    let mut chunks_processed = 0u32;

    for chunk in chunks.iter() {
        if all.len() >= cap {
            break;
        }
        let msgs = build_messages(&chunk.text, cards_per_chunk);
        let chat_opts = ChatOpts {
            temperature: Some(0.3),
            max_tokens: Some(800),
            ..Default::default()
        };
        let resp = provider.chat(&msgs, &chat_opts).await?;
        model = resp.model;
        chunks_processed += 1;
        let wire = parse_llm_cards(&resp.content).unwrap_or(LlmCardsWire { cards: Vec::new() });
        for raw in wire.cards {
            let q = raw.q.unwrap_or_default();
            let a = raw.a.unwrap_or_default();
            all.push(Flashcard {
                page: chunk.page,
                q,
                a,
            });
        }
    }

    let before = all.len();
    let cleaned = validate_cards(all);
    let dropped = (before.saturating_sub(cleaned.len())) as u32;

    Ok(DeckReport {
        cards: cleaned,
        model,
        chunks_processed,
        dropped,
    })
}

/// Convenience: read PDF text from disk, then generate.
pub async fn generate_deck_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    opts: &DeckOpts,
) -> Result<DeckReport, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    generate_deck(provider, &pages, opts).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flashcard_is_serializable() {
        let c = Flashcard {
            page: 7,
            q: "What is the capital of France?".into(),
            a: "Paris.".into(),
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"page\":7"));
        assert!(j.contains("\"q\":\"What is"));
    }

    #[test]
    fn opts_default_is_sensible() {
        let o = DeckOpts::default();
        assert_eq!(o.cards_per_chunk, DEFAULT_CARDS_PER_CHUNK);
        assert_eq!(o.max_cards, MAX_DECK_SIZE as u32);
    }

    #[test]
    fn parses_plain_cards_json() {
        let raw = r#"{"cards":[{"q":"What is X?","a":"Y"}]}"#;
        let w = parse_llm_cards(raw).expect("should parse");
        assert_eq!(w.cards.len(), 1);
        assert_eq!(w.cards[0].q.as_deref(), Some("What is X?"));
    }

    #[test]
    fn parses_cards_with_fence_and_chatter() {
        let raw =
            "Sure thing! ```json\n{\"cards\":[{\"q\":\"Q\",\"a\":\"A\"}]}\n```\nhope that helps";
        let w = parse_llm_cards(raw).expect("should parse");
        assert_eq!(w.cards.len(), 1);
    }

    #[test]
    fn parse_returns_none_on_garbage() {
        assert!(parse_llm_cards("not json").is_none());
        assert!(parse_llm_cards("").is_none());
    }

    #[test]
    fn validate_drops_empty_qa() {
        let raw = vec![
            Flashcard {
                page: 1,
                q: "  ".into(),
                a: "A".into(),
            },
            Flashcard {
                page: 1,
                q: "Q".into(),
                a: " ".into(),
            },
            Flashcard {
                page: 1,
                q: "Real?".into(),
                a: "Yes.".into(),
            },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].q, "Real?");
    }

    #[test]
    fn validate_dedupes_by_normalised_q() {
        let raw = vec![
            Flashcard {
                page: 1,
                q: "What  is  X?".into(),
                a: "Y".into(),
            },
            Flashcard {
                page: 1,
                q: "WHAT IS X?".into(),
                a: "Z".into(),
            },
            Flashcard {
                page: 2,
                q: "Something else?".into(),
                a: "Sure".into(),
            },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 2);
        // First occurrence wins.
        assert_eq!(v[0].a, "Y");
    }

    #[test]
    fn validate_caps_at_max_deck_size() {
        let raw: Vec<Flashcard> = (0..MAX_DECK_SIZE + 30)
            .map(|i| Flashcard {
                page: 1,
                q: format!("Q{i}"),
                a: "A".into(),
            })
            .collect();
        let v = validate_cards(raw);
        assert_eq!(v.len(), MAX_DECK_SIZE);
    }

    #[test]
    fn validate_drops_oversized_fields() {
        let huge = "x".repeat(MAX_FIELD_CHARS + 1);
        let raw = vec![
            Flashcard {
                page: 1,
                q: huge.clone(),
                a: "A".into(),
            },
            Flashcard {
                page: 1,
                q: "Short?".into(),
                a: huge,
            },
            Flashcard {
                page: 1,
                q: "OK?".into(),
                a: "Yes".into(),
            },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].q, "OK?");
    }
}
