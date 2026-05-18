// Beacon Study Mode — Q&A flashcards with SM-2-lite spaced repetition.
//
// Pipeline (Slice 13 of Beacon Bonus):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Chunk it with `ai::chunker::chunk_pages`.
//   3. For each chunk, ask the AiProvider to emit JSON `{cards:[{q,a}]}`
//      with a strict "answerable from THIS chunk alone" system prompt.
//   4. Validate (drop empty Q/A, drop too-long, dedupe by normalised Q).
//   5. Return `Vec<Flashcard>` tagged with origin `page`.
//   6. Caller stores them in `study_store` keyed by `pdf_hash`.
//
// Review side: `ai::sm2` schedules cards. `study_store` queries due cards.
// The frontend drives one card at a time, posting the user's ease rating
// back to `slab_beacon_study_review`.

#![allow(dead_code)] // populated in later tasks of this slice

use serde::{Deserialize, Serialize};

/// Hard cap on generated cards per `generate_deck` call. Defends the UI
/// (and the user's review backlog) from a runaway model. 600-page novels
/// can still exceed this — that's fine, user can re-run for "more".
pub const MAX_DECK_SIZE: usize = 200;

/// One flashcard. Pre-store shape: no DB id, no ease, no due date —
/// those land when `study_store::insert_deck` writes the row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Flashcard {
    /// 1-indexed source page (lets the UI "jump to source" on a card).
    pub page: u32,
    /// Question text. Trimmed, non-empty after validate.
    pub q: String,
    /// Answer text. Trimmed, non-empty after validate.
    pub a: String,
}

/// Stub — Task 4 fills it in. Exists now so the module compiles.
pub fn validate_cards(_raw: Vec<Flashcard>) -> Vec<Flashcard> {
    Vec::new()
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
    fn validate_stub_returns_empty() {
        // Will be replaced in Task 4 with real coverage.
        assert!(validate_cards(Vec::new()).is_empty());
    }
}
