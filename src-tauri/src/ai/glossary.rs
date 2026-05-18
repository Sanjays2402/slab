// Beacon Glossary — mine an opened PDF for domain-specific terminology
// (acronyms, italicised technical terms, defined-on-first-use phrases)
// and emit a clean, alphabetised glossary with LLM-generated definitions
// cited back to the source page.
//
// Workflow (Slice 14 of Beacon Bonus):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Regex-scan every page surfacing four kinds of `Candidate`:
//      - Acronym             ("AI", "LLM", "RAG", "NIST SP 800-53")
//      - DefinedOnFirstUse   ("RAG (retrieval-augmented generation)")
//      - Italicised          a-la *term* / _term_ remnants
//      - CapitalisedPhrase   low-frequency Title-Case multi-word phrases
//   3. Rank/dedupe candidates, take the top N, pack them with first-use
//      context, ask the AiProvider for JSON
//      `{entries:[{term,definition,page,confidence,kind}]}`.
//   4. Liberal JSON parser (mirrors `ai::citations::parse_llm_references`)
//      then validate+dedupe+cap → Vec<GlossaryEntry>.
//   5. Return `GlossaryReport` to the front-end. Cached per-PDF via
//      `ai::glossary_cache`.
//
// Pure types + regex scanning are unit-tested against deterministic
// strings — no real LLM in CI.

#![allow(dead_code)] // populated in later tasks of this slice

use serde::{Deserialize, Serialize};

/// Hard cap on candidates fed to the LLM. Defends against runaway docs.
pub const MAX_CANDIDATES_TO_LLM: usize = 200;

/// Hard cap on glossary entries returned. Realistic technical docs have
/// 30-100 terms; 500 is the runaway-model defence.
pub const MAX_GLOSSARY_ENTRIES: usize = 500;

/// LLM-context budget for the definition-extraction call.
pub const DEFAULT_GLOSSARY_MAX_CHARS: usize = 30_000;

/// Acronyms shorter than this are likely false-positives ("AI" is fine,
/// "I" isn't). Minimum lowered to 2 in Task 2's regex but the validator
/// rejects single-char "terms" upstream too.
pub const MIN_ACRONYM_CHARS: usize = 2;

/// Kind of candidate the scanner found. Surfaced to the UI as a chip
/// so users can filter (e.g. "acronyms only").
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateKind {
    Acronym,
    DefinedOnFirstUse,
    Italicised,
    CapitalisedPhrase,
}

/// One raw candidate, pre-LLM. Cheap to produce; many will be discarded
/// when the LLM either can't define them or merges them with synonyms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    /// The matched text, trimmed.
    pub term: String,
    /// 1-indexed page where the candidate was found.
    pub page: u32,
    /// What kind of pattern surfaced this candidate.
    pub kind: CandidateKind,
    /// ~200 chars of surrounding context (anchored on the term match).
    /// Empty for acronyms whose only context is the acronym itself.
    pub snippet: String,
}

/// One LLM-emitted, validated entry. This is what the UI renders.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlossaryEntry {
    /// Display term (sentence case, expanded if it was an acronym we
    /// linked, otherwise as-found).
    pub term: String,
    /// 1-3 sentence definition.
    pub definition: String,
    /// 1-indexed source page where the term first appears.
    pub page: u32,
    /// Model self-rated 0.0-1.0 confidence. Below 0.4 is dropped by
    /// `validate_entries`.
    pub confidence: f32,
    /// Echoed from the candidate kind so the UI can colour-chip it.
    pub kind: CandidateKind,
    /// Trimmed source snippet (≤ 240 chars). Kept so the UI can show
    /// "as seen in the document" beneath the LLM definition.
    pub source_snippet: String,
}

/// Counters for the UI footer + audit trail.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlossarySummary {
    pub candidates_total: u32,
    pub accepted: u32,
    pub rejected: u32,
    /// (kind, count) buckets for the accepted entries.
    pub kept_acronyms: u32,
    pub kept_defined_first_use: u32,
    pub kept_italicised: u32,
    pub kept_capitalised_phrase: u32,
}

/// What the frontend gets back from `slab_beacon_build_glossary` and
/// loads from cache. Schema-versioned at the cache layer (not here).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryReport {
    pub entries: Vec<GlossaryEntry>,
    pub summary: GlossarySummary,
    /// Model identifier for the definition-extraction call. Empty if
    /// `include_llm_pass=false` (regex-only run).
    pub model: String,
}

/// Knobs for `build_glossary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryOpts {
    /// If true, ask the AiProvider for definitions; if false, return raw
    /// candidates with empty definitions (useful for offline / tests).
    /// Default: true.
    #[serde(default = "default_true")]
    pub include_llm_pass: bool,
    /// Hard ceiling on LLM context (chars of candidate snippets).
    #[serde(default = "default_budget")]
    pub max_context_chars: u32,
    /// Hard ceiling on candidates fed to the LLM (the validator sorts
    /// by rank descending and trims). Default: `MAX_CANDIDATES_TO_LLM`.
    #[serde(default = "default_max_candidates")]
    pub max_candidates: u32,
}

fn default_true() -> bool {
    true
}
fn default_budget() -> u32 {
    DEFAULT_GLOSSARY_MAX_CHARS as u32
}
fn default_max_candidates() -> u32 {
    MAX_CANDIDATES_TO_LLM as u32
}

impl Default for GlossaryOpts {
    fn default() -> Self {
        Self {
            include_llm_pass: true,
            max_context_chars: DEFAULT_GLOSSARY_MAX_CHARS as u32,
            max_candidates: MAX_CANDIDATES_TO_LLM as u32,
        }
    }
}

/// Stub — Task 2 fills it in. Exists now so the module compiles.
pub fn scan_candidates(_pages: &[String]) -> Vec<Candidate> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glossary_entry_round_trips_through_json() {
        let e = GlossaryEntry {
            term: "RAG".into(),
            definition: "Retrieval-augmented generation.".into(),
            page: 3,
            confidence: 0.91,
            kind: CandidateKind::Acronym,
            source_snippet: "Using RAG, the model can…".into(),
        };
        let j = serde_json::to_string(&e).unwrap();
        let back: GlossaryEntry = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
        assert!(j.contains("\"kind\":\"acronym\""));
    }

    #[test]
    fn scan_candidates_stub_returns_empty() {
        assert!(scan_candidates(&[]).is_empty());
        assert!(scan_candidates(&["any text".to_string()]).is_empty());
    }
}
