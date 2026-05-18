// Beacon Citations — scan a PDF for inline citations and extract a structured
// References table from end-matter.
//
// Workflow (Slice 12 of Beacon Bonus):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Regex-scan every page for inline citations in three flavours:
//      - Author-year:  `(Smith 2024)`, `(Smith and Jones, 2024)`,
//                      `(Smith et al. 2024)`
//      - Bracket-num:  `[12]`, `[12, 14]`, `[12-15]`
//      - Bracket-key:  `[Smith2024]`, `[smith-2024-foo]`
//   3. Ask the configured AiProvider to parse the bibliography from the last
//      ~10% of pages and emit JSON `{entries:[{key,authors,year,title,page}]}`.
//      Liberal parser tolerates fences + trailing chatter.
//   4. Link inline → reference by canonical key (lowercased "first-author"
//      + year for author-year cites; raw number for bracket-num; raw key
//      for bracket-key).
//   5. Return `CitationReport` to the front-end. Pure types are unit-tested
//      against deterministic strings — no real LLM in CI.
//
// The detector does NOT mutate the PDF. The UI then either:
//   - Shows the references list with mention counts.
//   - Lets the user click a mention to jump to its source page.

use super::chunker::chunk_pages;
use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Cap on how much end-matter text we feed the LLM for references extraction.
/// Bigger than the outline budget — bibliographies can be long.
pub const DEFAULT_CITATIONS_MAX_CHARS: usize = 40_000;

/// Max references we accept in one report. Defends against runaway models.
pub const MAX_REFERENCES: usize = 500;

/// Max inline cites we surface. A 600-page novel can dwarf this, but if you
/// have > 2000 cites you almost certainly want a different tool than Slab.
pub const MAX_INLINE_CITES: usize = 2_000;

/// One inline citation found in body text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InlineCite {
    /// 1-indexed page where the cite appears.
    pub page: u32,
    /// The cite as it appeared in the text (e.g. `"(Smith 2024)"`).
    pub text: String,
    /// Canonical key for linking to a Reference. Lowercased "first-author"
    /// + year for author-year cites; raw number for bracket-num
    /// (e.g. `"12"`); raw key for bracket-key (e.g. `"smith2024"`).
    pub key: String,
    /// Best-effort author surname hint (empty for bracket-num cites).
    pub authors_hint: String,
    /// Best-effort year hint as a 4-digit string (empty for bracket-num).
    pub year_hint: String,
}

/// One bibliography entry extracted from end-matter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reference {
    /// Canonical key for matching against inline cites.
    pub key: String,
    /// Free-form authors string ("Smith, J. and Jones, K.").
    pub authors: String,
    /// 4-digit year ("2024") or empty if absent.
    pub year: String,
    /// Title or first sentence of the entry.
    pub title: String,
    /// 1-indexed page where this reference is printed (lets the UI jump
    /// to the bibliography line).
    pub page_in_doc: u32,
}

/// Diagnostic counts for the UI footer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CitationSummary {
    pub inline_total: u32,
    pub references_total: u32,
    /// Number of inline cites that matched a reference by key.
    pub linked: u32,
    /// Inline cites that had no matching reference.
    pub orphans: u32,
}

/// What the frontend gets back from `slab_beacon_find_citations`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationReport {
    pub inline: Vec<InlineCite>,
    pub references: Vec<Reference>,
    pub summary: CitationSummary,
    /// Model identifier for the references extraction call. Empty if
    /// `include_llm_pass=false`.
    pub model: String,
}

/// Knobs for `find_citations`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationOpts {
    /// If true, ask the AI provider to extract structured references from
    /// end-matter. Disabled = regex-only inline scan (still useful, but no
    /// linking). Default: true.
    #[serde(default = "default_true")]
    pub include_llm_pass: bool,
    /// Hard ceiling on LLM context (chars of end-matter text).
    #[serde(default = "default_budget")]
    pub max_context_chars: u32,
}

fn default_true() -> bool {
    true
}
fn default_budget() -> u32 {
    DEFAULT_CITATIONS_MAX_CHARS as u32
}

impl Default for CitationOpts {
    fn default() -> Self {
        Self {
            include_llm_pass: true,
            max_context_chars: DEFAULT_CITATIONS_MAX_CHARS as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_serializable() {
        let r = CitationReport {
            inline: vec![],
            references: vec![],
            summary: CitationSummary::default(),
            model: String::new(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"inline\":[]"));
        assert!(j.contains("\"references\":[]"));
    }

    #[test]
    fn opts_default_enables_llm() {
        let o = CitationOpts::default();
        assert!(o.include_llm_pass);
        assert_eq!(o.max_context_chars, DEFAULT_CITATIONS_MAX_CHARS as u32);
    }
}
