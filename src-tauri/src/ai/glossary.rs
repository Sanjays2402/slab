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

// ---------------- LLM definition extraction (Task 3) ----------------

use super::chunker::chunk_pages;
use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use std::path::Path;
use std::sync::Arc;

const SYSTEM_PROMPT: &str = "You are Beacon, a domain-glossary extractor. \
The user shows you a list of candidate terms from a technical document, \
each with the page it first appears on and a short surrounding snippet. \
Reply with JSON ONLY, no prose, no markdown fences:\n\
{\"entries\":[{\"term\":\"...\",\"definition\":\"...\",\"page\":N,\
\"confidence\":0.0-1.0,\"kind\":\"acronym|defined-on-first-use|italicised|capitalised-phrase\"}]}\n\
Rules:\n\
- definition: 1-3 sentences, plain English, grounded in the snippet. \
Never invent facts not in the snippet.\n\
- If the term is a common-English word with no domain-specific meaning \
here (e.g. \"the\", \"figure\", \"section\"), OMIT it entirely.\n\
- If the snippet doesn't actually define the term, set confidence <= 0.4 \
and keep your definition very tentative — the caller will drop low-\
confidence entries.\n\
- term: keep the canonical casing the user passed in.\n\
- page: echo the page from the input candidate.\n\
- kind: echo from the input candidate.";

/// Wire-shape returned by the LLM. Liberal — all fields optional.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmGlossaryWire {
    #[serde(default)]
    pub(super) entries: Vec<LlmGlossaryEntryWire>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmGlossaryEntryWire {
    #[serde(default)]
    pub(super) term: Option<String>,
    #[serde(default)]
    pub(super) definition: Option<String>,
    #[serde(default)]
    pub(super) page: Option<u32>,
    #[serde(default)]
    pub(super) confidence: Option<f32>,
    #[serde(default)]
    pub(super) kind: Option<String>,
}

/// Liberal JSON parser — mirrors `citations::parse_llm_references`.
pub(super) fn parse_llm_glossary(raw: &str) -> Option<LlmGlossaryWire> {
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

fn parse_kind(s: &str) -> Option<CandidateKind> {
    match s.trim().to_lowercase().as_str() {
        "acronym" => Some(CandidateKind::Acronym),
        "defined-on-first-use" | "defined_on_first_use" => Some(CandidateKind::DefinedOnFirstUse),
        "italicised" | "italicized" => Some(CandidateKind::Italicised),
        "capitalised-phrase" | "capitalized-phrase" | "capitalised_phrase" => {
            Some(CandidateKind::CapitalisedPhrase)
        }
        _ => None,
    }
}

/// Validate / dedupe / cap raw LLM entries. Returns the final list +
/// rejection count.
///
/// Rules:
/// - term must be 1-80 chars, non-empty after trim.
/// - definition must be ≥ 8 chars.
/// - page must be 1..=total_pages.
/// - confidence < 0.4 → drop.
/// - kind defaults to Acronym if missing/unknown.
/// - dedupe by case-folded term, first occurrence wins.
/// - cap at `MAX_GLOSSARY_ENTRIES`.
pub(super) fn validate_entries(
    raw: Vec<LlmGlossaryEntryWire>,
    candidates: &[Candidate],
    total_pages: u32,
) -> (Vec<GlossaryEntry>, u32) {
    let snippets: HashMap<String, &Candidate> =
        candidates.iter().map(|c| (norm(&c.term), c)).collect();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut out: Vec<GlossaryEntry> = Vec::new();
    let mut rejected = 0u32;
    for e in raw {
        let term = e.term.unwrap_or_default().trim().to_string();
        if term.is_empty() || term.len() > 80 {
            rejected += 1;
            continue;
        }
        let definition = e.definition.unwrap_or_default().trim().to_string();
        if definition.len() < 8 {
            rejected += 1;
            continue;
        }
        let page = match e.page {
            Some(p) if p >= 1 && p <= total_pages => p,
            _ => {
                rejected += 1;
                continue;
            }
        };
        let confidence = e.confidence.unwrap_or(0.0).clamp(0.0, 1.0);
        if confidence < 0.4 {
            rejected += 1;
            continue;
        }
        let kind = e
            .kind
            .as_deref()
            .and_then(parse_kind)
            .unwrap_or(CandidateKind::Acronym);
        let key = norm(&term);
        if seen.contains_key(&key) {
            rejected += 1;
            continue;
        }
        seen.insert(key.clone(), ());
        let source_snippet = snippets
            .get(&key)
            .map(|c| c.snippet.clone())
            .unwrap_or_default();
        out.push(GlossaryEntry {
            term,
            definition,
            page,
            confidence,
            kind,
            source_snippet,
        });
        if out.len() >= MAX_GLOSSARY_ENTRIES {
            break;
        }
    }
    // Alphabetical sort (case-insensitive). Stable so dupes-by-folding
    // keep their relative order.
    out.sort_by(|a, b| a.term.to_lowercase().cmp(&b.term.to_lowercase()));
    (out, rejected)
}

/// Build the summary footer from the validated entries.
pub(super) fn build_summary(
    candidates_total: u32,
    entries: &[GlossaryEntry],
    rejected: u32,
) -> GlossarySummary {
    let mut s = GlossarySummary {
        candidates_total,
        accepted: entries.len() as u32,
        rejected,
        ..GlossarySummary::default()
    };
    for e in entries {
        match e.kind {
            CandidateKind::Acronym => s.kept_acronyms += 1,
            CandidateKind::DefinedOnFirstUse => s.kept_defined_first_use += 1,
            CandidateKind::Italicised => s.kept_italicised += 1,
            CandidateKind::CapitalisedPhrase => s.kept_capitalised_phrase += 1,
        }
    }
    s
}

/// Pack candidates into a budget-bounded user prompt.
fn build_glossary_messages(
    candidates: &[Candidate],
    max_chars: usize,
    max_count: usize,
) -> Vec<ChatMessage> {
    let mut buf = String::from("Candidates:\n");
    let mut count = 0usize;
    for c in candidates.iter().take(max_count) {
        let kind_str = match c.kind {
            CandidateKind::Acronym => "acronym",
            CandidateKind::DefinedOnFirstUse => "defined-on-first-use",
            CandidateKind::Italicised => "italicised",
            CandidateKind::CapitalisedPhrase => "capitalised-phrase",
        };
        let line = format!(
            "- term={:?} page={} kind={} snippet={:?}\n",
            c.term, c.page, kind_str, c.snippet
        );
        if buf.len() + line.len() > max_chars {
            break;
        }
        buf.push_str(&line);
        count += 1;
    }
    let user = format!(
        "Below are {count} candidate terms mined from a PDF. Reply with \
         JSON only as instructed.\n{buf}"
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

/// Top-level: run the full glossary pipeline against pre-extracted pages.
pub async fn build_glossary(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    opts: &GlossaryOpts,
) -> Result<GlossaryReport, AiError> {
    // Pre-flight chunker (fail fast on truly empty docs).
    let _chunks = chunk_pages(pages);

    let candidates = scan_candidates(pages);
    let candidates_total = candidates.len() as u32;
    let mut entries: Vec<GlossaryEntry> = Vec::new();
    let mut model = String::new();
    let mut rejected = 0u32;

    if opts.include_llm_pass && !candidates.is_empty() {
        let msgs = build_glossary_messages(
            &candidates,
            opts.max_context_chars as usize,
            opts.max_candidates as usize,
        );
        let chat_opts = ChatOpts {
            temperature: Some(0.1),
            max_tokens: Some(8_000),
            ..Default::default()
        };
        let resp = provider.chat(&msgs, &chat_opts).await?;
        model = resp.model;
        let wire = parse_llm_glossary(&resp.content).unwrap_or(LlmGlossaryWire {
            entries: Vec::new(),
        });
        let (validated, rej) = validate_entries(wire.entries, &candidates, pages.len() as u32);
        entries = validated;
        rejected = rej;
    }

    let summary = build_summary(candidates_total, &entries, rejected);
    Ok(GlossaryReport {
        entries,
        summary,
        model,
    })
}

/// Convenience: read PDF text from disk, then run `build_glossary`.
pub async fn build_glossary_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    opts: &GlossaryOpts,
) -> Result<GlossaryReport, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    build_glossary(provider, &pages, opts).await
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

    #[test]
    fn parse_llm_glossary_accepts_fenced_response() {
        let raw = "```json\n{\"entries\":[{\"term\":\"RAG\",\"definition\":\"Retrieval-augmented generation.\",\"page\":3,\"confidence\":0.95,\"kind\":\"acronym\"}]}\n```";
        let w = parse_llm_glossary(raw).expect("must parse");
        assert_eq!(w.entries.len(), 1);
        assert_eq!(w.entries[0].term.as_deref(), Some("RAG"));
    }

    #[test]
    fn parse_llm_glossary_tolerates_trailing_chatter() {
        let raw = "Here is the JSON you requested:\n{\"entries\":[{\"term\":\"X\",\"definition\":\"a longer definition that passes\",\"page\":1,\"confidence\":0.9}]}\nlet me know if you need more.";
        let w = parse_llm_glossary(raw).expect("must parse");
        assert_eq!(w.entries.len(), 1);
    }

    #[test]
    fn validate_drops_low_confidence_short_def_and_bad_page() {
        let cands = vec![Candidate {
            term: "Good".into(),
            page: 1,
            kind: CandidateKind::Acronym,
            snippet: "Good is good".into(),
        }];
        let raw = vec![
            LlmGlossaryEntryWire {
                term: Some("Good".into()),
                definition: Some("A solid term with a long enough definition.".into()),
                page: Some(1),
                confidence: Some(0.9),
                kind: Some("acronym".into()),
            },
            LlmGlossaryEntryWire {
                term: Some("LowConf".into()),
                definition: Some("Some definition that passes length.".into()),
                page: Some(1),
                confidence: Some(0.3),
                kind: Some("acronym".into()),
            },
            LlmGlossaryEntryWire {
                term: Some("Short".into()),
                definition: Some("nope".into()),
                page: Some(1),
                confidence: Some(0.9),
                kind: Some("acronym".into()),
            },
            LlmGlossaryEntryWire {
                term: Some("BadPage".into()),
                definition: Some("This definition is long enough.".into()),
                page: Some(99),
                confidence: Some(0.9),
                kind: Some("acronym".into()),
            },
        ];
        let (out, rej) = validate_entries(raw, &cands, 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term, "Good");
        assert_eq!(rej, 3);
    }

    #[test]
    fn validate_dedupes_by_lowercased_term_and_sorts_alpha() {
        let cands: Vec<Candidate> = Vec::new();
        let raw = vec![
            LlmGlossaryEntryWire {
                term: Some("Zebra".into()),
                definition: Some("striped equid common in Africa.".into()),
                page: Some(1),
                confidence: Some(0.8),
                kind: Some("capitalised-phrase".into()),
            },
            LlmGlossaryEntryWire {
                term: Some("apple".into()),
                definition: Some("fruit grown on a tree.".into()),
                page: Some(1),
                confidence: Some(0.8),
                kind: Some("capitalised-phrase".into()),
            },
            LlmGlossaryEntryWire {
                term: Some("APPLE".into()), // dupe of "apple" case-folded
                definition: Some("the trademarked computer co.".into()),
                page: Some(2),
                confidence: Some(0.8),
                kind: Some("capitalised-phrase".into()),
            },
        ];
        let (out, rej) = validate_entries(raw, &cands, 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].term, "apple"); // alphabetical
        assert_eq!(out[1].term, "Zebra");
        assert_eq!(rej, 1);
    }
}
