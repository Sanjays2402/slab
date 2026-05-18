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

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Hard cap on candidates fed to the LLM. Defends against runaway docs.
pub const MAX_CANDIDATES_TO_LLM: usize = 200;

/// Hard cap on glossary entries returned. Realistic technical docs have
/// 30-100 terms; 500 is the runaway-model defence.
pub const MAX_GLOSSARY_ENTRIES: usize = 500;

/// LLM-context budget for the definition-extraction call.
pub const DEFAULT_GLOSSARY_MAX_CHARS: usize = 30_000;

/// Acronyms shorter than this are likely false-positives ("AI" is fine,
/// "I" isn't).
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
    /// ~240 chars of surrounding context (anchored on the term match).
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

// ---------------- Regex scanners (Task 2) ----------------

/// Matches all-caps acronyms 2-10 chars long, optionally with numerics
/// or hyphenated suffixes (e.g. "NIST SP 800-53", "GPT-4o"). The
/// `passes_acronym_neighbourhood` linker rejects acronyms with no
/// lower-case neighbours within ±30 chars (defends against all-caps
/// section headers).
fn re_acronym() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b([A-Z][A-Z0-9]{1,9}(?:[- ][A-Z0-9]{1,8})?)\b").unwrap())
}

/// Matches "Term (expansion goes here)" with the expansion in parens.
/// The capture group 1 is the term-ish bit just before the paren.
fn re_defined_first_use_parens() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"([A-Z][A-Za-z][A-Za-z0-9\- ]{1,40})\s+\(([a-zA-Z][a-zA-Z0-9 ,\-]{8,140})\)")
            .unwrap()
    })
}

/// Matches "Term — definition" or "Term -- definition" (em-dash or
/// double-hyphen). The definition must be 8-200 chars and start lowercase
/// (avoids matching section headers).
fn re_defined_first_use_dash() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"([A-Z][A-Za-z][A-Za-z0-9\- ]{1,40})\s+(?:\u{2014}|--)\s+([a-z][a-zA-Z0-9 ,\-]{8,200})",
        )
        .unwrap()
    })
}

/// Matches italicised remnants — pdf extraction usually strips the
/// formatting, but `*term*` / `_term_` markers leak through when the
/// source is generated from markdown / LaTeX `\emph{}` / `\textit{}`.
fn re_italicised() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?:\*|_)([A-Za-z][A-Za-z0-9 \-]{2,40})(?:\*|_)").unwrap())
}

/// Matches 2-5 word Title-Case phrases NOT at the start of a sentence
/// (the regex requires a leading lowercase word + space). Captures the
/// phrase only.
fn re_title_case_phrase() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?:[a-z]+ )((?:[A-Z][a-z]{2,15} ){1,4}[A-Z][a-z]{2,15})").unwrap()
    })
}

/// True iff the acronym has at least one lower-case ASCII letter within
/// `radius` chars on either side — proxy for "not in an all-caps header".
fn passes_acronym_neighbourhood(
    text: &str,
    match_start: usize,
    match_end: usize,
    radius: usize,
) -> bool {
    let lo = match_start.saturating_sub(radius);
    let hi = text.len().min(match_end + radius);
    // Shift to UTF-8 char boundaries.
    let safe_lo = (lo..=match_start)
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(match_start);
    let safe_hi = (match_end..=hi)
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(match_end);
    text[safe_lo..safe_hi]
        .chars()
        .any(|c| c.is_ascii_lowercase())
}

/// Pull a ≤ 240-char window centred on the match.
fn snippet(text: &str, m_start: usize, m_end: usize) -> String {
    let lo = m_start.saturating_sub(80);
    let hi = text.len().min(m_end + 160);
    let safe_lo = (lo..=m_start)
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(m_start);
    let safe_hi = (m_end..=hi)
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(m_end);
    text[safe_lo..safe_hi].replace('\n', " ").trim().to_string()
}

/// Lower-case fold for dedupe.
fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Scan every page and emit a deduped, ranked list of candidates.
/// Ranking: DefinedOnFirstUse > Acronym > Italicised > CapitalisedPhrase,
/// then by frequency desc, then by first-seen page asc.
pub fn scan_candidates(pages: &[String]) -> Vec<Candidate> {
    let mut seen: HashMap<String, Candidate> = HashMap::new();
    let mut freq: HashMap<String, u32> = HashMap::new();

    for (i, page) in pages.iter().enumerate() {
        let page_no = (i as u32) + 1;

        // Pass 1a: DefinedOnFirstUse (parens form).
        for caps in re_defined_first_use_parens().captures_iter(page) {
            if let (Some(term_m), Some(_def_m)) = (caps.get(1), caps.get(2)) {
                let term = term_m.as_str().trim().to_string();
                if term.len() < 2 || term.len() > 40 {
                    continue;
                }
                let key = norm(&term);
                *freq.entry(key.clone()).or_default() += 1;
                seen.entry(key).or_insert_with(|| Candidate {
                    term,
                    page: page_no,
                    kind: CandidateKind::DefinedOnFirstUse,
                    snippet: snippet(page, term_m.start(), caps.get(0).unwrap().end()),
                });
            }
        }

        // Pass 1b: DefinedOnFirstUse (em-dash form).
        for caps in re_defined_first_use_dash().captures_iter(page) {
            if let (Some(term_m), Some(_def_m)) = (caps.get(1), caps.get(2)) {
                let term = term_m.as_str().trim().to_string();
                if term.len() < 2 || term.len() > 40 {
                    continue;
                }
                let key = norm(&term);
                *freq.entry(key.clone()).or_default() += 1;
                seen.entry(key).or_insert_with(|| Candidate {
                    term,
                    page: page_no,
                    kind: CandidateKind::DefinedOnFirstUse,
                    snippet: snippet(page, term_m.start(), caps.get(0).unwrap().end()),
                });
            }
        }

        // Pass 2: Acronyms (only if surrounded by lower-case prose).
        for caps in re_acronym().captures_iter(page) {
            let m = caps.get(1).unwrap();
            let term = m.as_str().trim().to_string();
            if term.len() < MIN_ACRONYM_CHARS {
                continue;
            }
            if !passes_acronym_neighbourhood(page, m.start(), m.end(), 30) {
                continue;
            }
            let key = norm(&term);
            *freq.entry(key.clone()).or_default() += 1;
            seen.entry(key).or_insert_with(|| Candidate {
                term,
                page: page_no,
                kind: CandidateKind::Acronym,
                snippet: snippet(page, m.start(), m.end()),
            });
        }

        // Pass 3: Italicised remnants.
        for caps in re_italicised().captures_iter(page) {
            if let Some(m) = caps.get(1) {
                let term = m.as_str().trim().to_string();
                if term.len() < 3 || term.len() > 40 {
                    continue;
                }
                let key = norm(&term);
                *freq.entry(key.clone()).or_default() += 1;
                seen.entry(key).or_insert_with(|| Candidate {
                    term,
                    page: page_no,
                    kind: CandidateKind::Italicised,
                    snippet: snippet(page, m.start(), m.end()),
                });
            }
        }

        // Pass 4: Title-Case multi-word phrases.
        for caps in re_title_case_phrase().captures_iter(page) {
            if let Some(m) = caps.get(1) {
                let term = m.as_str().trim().to_string();
                if term.len() < 6 || term.len() > 60 {
                    continue;
                }
                let key = norm(&term);
                *freq.entry(key.clone()).or_default() += 1;
                seen.entry(key).or_insert_with(|| Candidate {
                    term,
                    page: page_no,
                    kind: CandidateKind::CapitalisedPhrase,
                    snippet: snippet(page, m.start(), m.end()),
                });
            }
        }
    }

    // Rank: kind weight desc, then freq desc, then page asc.
    fn kind_weight(k: CandidateKind) -> u32 {
        match k {
            CandidateKind::DefinedOnFirstUse => 4,
            CandidateKind::Acronym => 3,
            CandidateKind::Italicised => 2,
            CandidateKind::CapitalisedPhrase => 1,
        }
    }
    let mut out: Vec<Candidate> = seen.into_values().collect();
    out.sort_by(|a, b| {
        let ka = kind_weight(a.kind);
        let kb = kind_weight(b.kind);
        kb.cmp(&ka)
            .then_with(|| {
                let fa = *freq.get(&norm(&a.term)).unwrap_or(&1);
                let fb = *freq.get(&norm(&b.term)).unwrap_or(&1);
                fb.cmp(&fa)
            })
            .then_with(|| a.page.cmp(&b.page))
    });
    out
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
    fn scan_candidates_empty_input_returns_empty() {
        assert!(scan_candidates(&[]).is_empty());
    }

    #[test]
    fn scans_acronym_in_lowercase_neighbourhood() {
        let pages = vec!["RAG is widely used in retrieval pipelines.".into()];
        let cs = scan_candidates(&pages);
        assert!(cs
            .iter()
            .any(|c| c.term == "RAG" && c.kind == CandidateKind::Acronym));
    }

    #[test]
    fn rejects_acronym_inside_all_caps_header() {
        let pages = vec!["SECTION 4: NIST AND THE ACT".into()];
        let cs = scan_candidates(&pages);
        // No lowercase neighbourhood → all acronyms here are dropped.
        assert!(cs.iter().all(|c| c.kind != CandidateKind::Acronym));
    }

    #[test]
    fn picks_up_defined_on_first_use_parens() {
        let pages = vec![
            "We use Retrieval Augmented Generation (RAG, a hybrid LLM technique) extensively."
                .into(),
        ];
        let cs = scan_candidates(&pages);
        assert!(cs.iter().any(|c| {
            c.term.contains("Retrieval Augmented Generation")
                && c.kind == CandidateKind::DefinedOnFirstUse
        }));
    }

    #[test]
    fn picks_up_em_dash_definition() {
        let pages = vec![
            "A Transformer \u{2014} a deep learning architecture based on self-attention.".into(),
        ];
        let cs = scan_candidates(&pages);
        assert!(cs
            .iter()
            .any(|c| c.term == "Transformer" && c.kind == CandidateKind::DefinedOnFirstUse));
    }

    #[test]
    fn picks_up_italicised_remnant() {
        let pages = vec!["the *attention mechanism* is central to this model.".into()];
        let cs = scan_candidates(&pages);
        assert!(cs
            .iter()
            .any(|c| c.term == "attention mechanism" && c.kind == CandidateKind::Italicised));
    }

    #[test]
    fn dedupes_across_pages_and_takes_first_page() {
        let pages = vec![
            "intro mentions RAG once.".into(),
            "page two also mentions RAG and RAG and RAG.".into(),
        ];
        let cs = scan_candidates(&pages);
        let rag: Vec<_> = cs.iter().filter(|c| c.term == "RAG").collect();
        assert_eq!(rag.len(), 1, "RAG must dedupe to a single candidate");
        assert_eq!(rag[0].page, 1, "first-seen page wins");
    }

    #[test]
    fn ranks_defined_first_use_above_plain_acronym() {
        let pages = vec![
            "lowercase RAG appears. Now Retrieval Augmented Generation (RAG, a hybrid method) is introduced.".into(),
        ];
        let cs = scan_candidates(&pages);
        // First entry should be DefinedOnFirstUse.
        assert!(matches!(cs[0].kind, CandidateKind::DefinedOnFirstUse));
    }
}
