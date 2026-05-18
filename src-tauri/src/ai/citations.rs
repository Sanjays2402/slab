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

use regex::Regex;

/// Wire shape we ask the LLM to emit. Liberal: every field optional, we
/// validate in `validate_references` next.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmRefsWire {
    #[serde(default)]
    pub(super) entries: Vec<LlmRefEntryWire>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmRefEntryWire {
    #[serde(default)]
    pub(super) key: Option<String>,
    #[serde(default)]
    pub(super) authors: Option<String>,
    #[serde(default)]
    pub(super) year: Option<String>,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) page: Option<u32>,
}

/// Liberal JSON parser for the references payload. Same shape as
/// `outline::parse_llm_outline` — strip fence, find outermost braces,
/// serde_json::from_str.
pub(super) fn parse_llm_references(raw: &str) -> Option<LlmRefsWire> {
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

/// Validate, dedupe, and cap a raw list of LLM-extracted reference entries.
///
/// Rules:
/// - `page` must be `1..=total_pages`; entries without a valid page are
///   dropped. (No page = we can't jump-to-bibliography, which is the
///   feature's primary value.)
/// - Empty `title` AND empty `authors` → drop.
/// - Missing `key` → synthesize from `<first-author-surname-lowercased><year>`.
/// - Dedupe by lowercased key, first occurrence wins.
/// - Cap at `MAX_REFERENCES`.
pub(super) fn validate_references(
    entries: Vec<LlmRefEntryWire>,
    total_pages: u32,
) -> Vec<Reference> {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut out: Vec<Reference> = Vec::new();
    for e in entries {
        let authors = e.authors.unwrap_or_default().trim().to_string();
        let title = e.title.unwrap_or_default().trim().to_string();
        if authors.is_empty() && title.is_empty() {
            continue;
        }
        let year = e.year.unwrap_or_default().trim().to_string();
        let page = match e.page {
            Some(p) if p >= 1 && p <= total_pages => p,
            _ => continue,
        };
        let key_raw = e.key.unwrap_or_default().trim().to_string();
        let key = if key_raw.is_empty() {
            let first = authors
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(',')
                .to_lowercase();
            format!("{first}{year}")
        } else {
            key_raw.to_lowercase()
        };
        if key.is_empty() {
            continue;
        }
        if seen.contains_key(&key) {
            continue;
        }
        seen.insert(key.clone(), ());
        out.push(Reference {
            key,
            authors,
            year,
            title,
            page_in_doc: page,
        });
        if out.len() >= MAX_REFERENCES {
            break;
        }
    }
    out
}

/// Lazily-compiled regexes shared across calls.
fn re_author_year() -> &'static Regex {
    // Matches "(Smith 2024)", "(Smith, 2024)", "(Smith and Jones 2024)",
    // "(Smith et al. 2024)", "(Smith et al., 2024)".
    // First capture: author surname(s) blob. Second: 4-digit year.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\(([A-Z][A-Za-z'\-]+(?:\s+et\s+al\.?)?(?:\s+(?:and|&)\s+[A-Z][A-Za-z'\-]+(?:\s+et\s+al\.?)?)*),?\s+((?:19|20)\d{2})\)",
        )
        .unwrap()
    })
}

fn re_bracket_num() -> &'static Regex {
    // Matches "[12]", "[12, 14]", "[12-15]". We use one regex for the
    // outer "[...]" and parse the content with a small helper.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[(\d+(?:\s*[,\u{2013}-]\s*\d+)*)\]").unwrap())
}

fn re_bracket_key() -> &'static Regex {
    // Matches "[Smith2024]", "[smith-2024-foo]". Avoids matching plain
    // numbers (those are handled by re_bracket_num).
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([A-Za-z][A-Za-z0-9_\-]*\d{4}[A-Za-z0-9_\-]*)\]").unwrap())
}

/// Scan every page for inline citations using regex. Pure, deterministic.
pub fn scan_inline_citations(pages: &[String]) -> Vec<InlineCite> {
    let mut out: Vec<InlineCite> = Vec::new();

    'outer: for (i, page) in pages.iter().enumerate() {
        let page_no = (i as u32) + 1;

        // Pass 1: author-year cites.
        for caps in re_author_year().captures_iter(page) {
            let raw = caps.get(0).unwrap().as_str().to_string();
            let authors_raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let year = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            // First author surname is the head of the authors blob.
            let first = authors_raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(',')
                .to_lowercase();
            let key = format!("{first}{year}");
            out.push(InlineCite {
                page: page_no,
                text: raw,
                key,
                authors_hint: first,
                year_hint: year,
            });
            if out.len() >= MAX_INLINE_CITES {
                break 'outer;
            }
        }

        // Pass 2: bracket-numeric cites (each number in a list becomes
        // its own InlineCite).
        for caps in re_bracket_num().captures_iter(page) {
            let raw = caps.get(0).unwrap().as_str().to_string();
            let body = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            for n in expand_num_list(body) {
                out.push(InlineCite {
                    page: page_no,
                    text: raw.clone(),
                    key: n.to_string(),
                    authors_hint: String::new(),
                    year_hint: String::new(),
                });
                if out.len() >= MAX_INLINE_CITES {
                    break 'outer;
                }
            }
        }

        // Pass 3: bracket-key cites.
        for caps in re_bracket_key().captures_iter(page) {
            let raw = caps.get(0).unwrap().as_str().to_string();
            let key_raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let key = key_raw.to_lowercase();
            // Best-effort: split on the 4-digit year to derive hints.
            let (author_hint, year_hint) = split_key(&key);
            out.push(InlineCite {
                page: page_no,
                text: raw,
                key,
                authors_hint: author_hint,
                year_hint,
            });
            if out.len() >= MAX_INLINE_CITES {
                break 'outer;
            }
        }
    }

    out
}

/// Expand `"12"` or `"14, 15"` or `"12-15"` into a `Vec<u32>`. Ranges with
/// > 20 numbers are clamped to the first 20 (defends against pathological
/// `[1-9999]`-style typos).
fn expand_num_list(s: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        // En-dash (U+2013) or ASCII hyphen as range separator.
        let sep = if part.contains('\u{2013}') {
            '\u{2013}'
        } else {
            '-'
        };
        if part.contains(sep) {
            let mut iter = part.split(sep);
            if let (Some(a), Some(b)) = (iter.next(), iter.next()) {
                if let (Ok(start), Ok(end)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                    let lo = start.min(end);
                    let hi = start.max(end);
                    for n in lo..=hi {
                        out.push(n);
                        if out.len() >= 20 {
                            return out;
                        }
                    }
                    continue;
                }
            }
        }
        if let Ok(n) = part.parse::<u32>() {
            out.push(n);
        }
    }
    out
}

/// Split a `[Smith2024]`-style key into `(author_hint, year_hint)`. Best
/// effort: finds the first 4-digit year inside the key.
fn split_key(key: &str) -> (String, String) {
    let bytes = key.as_bytes();
    for i in 0..bytes.len().saturating_sub(3) {
        let slice = &bytes[i..i + 4];
        if slice.iter().all(|b| b.is_ascii_digit()) {
            let year = std::str::from_utf8(slice).unwrap_or("").to_string();
            // Year must be a plausible publication year.
            if year.starts_with("19") || year.starts_with("20") {
                let head = key[..i].trim_end_matches(|c: char| !c.is_alphabetic());
                return (head.to_lowercase(), year);
            }
        }
    }
    (key.to_string(), String::new())
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

    #[test]
    fn scans_author_year_cites() {
        let pages = vec![
            "The early work (Smith 2024) showed this.".to_string(),
            "More recently (Jones et al., 2025) reported new results.".to_string(),
        ];
        let cites = scan_inline_citations(&pages);
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].page, 1);
        assert_eq!(cites[0].authors_hint, "smith");
        assert_eq!(cites[0].year_hint, "2024");
        assert_eq!(cites[0].key, "smith2024");
        assert_eq!(cites[1].authors_hint, "jones");
        assert_eq!(cites[1].year_hint, "2025");
    }

    #[test]
    fn scans_bracket_numeric_cites() {
        let pages = vec!["See [12] and [14, 15] for details.".to_string()];
        let cites = scan_inline_citations(&pages);
        // [12], [14, 15] → expand the range/list into 3 entries
        assert_eq!(cites.len(), 3);
        assert_eq!(cites[0].text, "[12]");
        assert_eq!(cites[0].key, "12");
        assert_eq!(cites[1].key, "14");
        assert_eq!(cites[2].key, "15");
    }

    #[test]
    fn scans_bracket_key_cites() {
        let pages = vec!["See [Smith2024] for details.".to_string()];
        let cites = scan_inline_citations(&pages);
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].key, "smith2024");
    }

    #[test]
    fn ignores_non_citation_parens() {
        // (Note that...) is prose, not a cite — no 4-digit year.
        let pages = vec!["Some prose (with a parenthetical aside).".to_string()];
        let cites = scan_inline_citations(&pages);
        assert!(cites.is_empty());
    }

    #[test]
    fn caps_at_max_inline_cites() {
        // Build a single page with way too many cites. Use a varying letter
        // suffix so each author name parses (digits aren't allowed in the
        // [A-Z][A-Za-z'\-]+ author regex).
        let blob: String = (0..MAX_INLINE_CITES + 50)
            .map(|i| {
                let suffix: String = (0..((i / 26) + 1))
                    .map(|j| (b'a' + ((i + j) % 26) as u8) as char)
                    .collect();
                format!("(Author{suffix} 2024) ")
            })
            .collect();
        let pages = vec![blob];
        let cites = scan_inline_citations(&pages);
        assert_eq!(cites.len(), MAX_INLINE_CITES);
    }

    #[test]
    fn parses_plain_references_json() {
        let raw = r#"{"entries":[{"key":"smith2024","authors":"Smith, J.","year":"2024","title":"On X","page":42}]}"#;
        let w = parse_llm_references(raw).expect("should parse");
        assert_eq!(w.entries.len(), 1);
        assert_eq!(w.entries[0].key.as_deref(), Some("smith2024"));
        assert_eq!(w.entries[0].page, Some(42));
    }

    #[test]
    fn parses_references_with_fence_and_chatter() {
        let raw = "Sure! ```json\n{\"entries\":[{\"key\":\"j25\",\"authors\":\"Jones\",\"year\":\"2025\",\"title\":\"Y\",\"page\":7}]}\n```\nhope that helps";
        let w = parse_llm_references(raw).expect("should parse");
        assert_eq!(w.entries.len(), 1);
        assert_eq!(w.entries[0].year.as_deref(), Some("2025"));
    }

    #[test]
    fn refs_returns_none_on_garbage() {
        assert!(parse_llm_references("not json at all").is_none());
        assert!(parse_llm_references("").is_none());
    }

    fn ref_entry(key: &str, authors: &str, year: &str, title: &str, page: u32) -> LlmRefEntryWire {
        LlmRefEntryWire {
            key: Some(key.into()),
            authors: Some(authors.into()),
            year: Some(year.into()),
            title: Some(title.into()),
            page: Some(page),
        }
    }

    #[test]
    fn validate_refs_drops_invalid_page() {
        let entries = vec![
            ref_entry("a24", "A", "2024", "T1", 5),
            ref_entry("b25", "B", "2025", "T2", 9999), // out of range
        ];
        let refs = validate_references(entries, 100);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].key, "a24");
    }

    #[test]
    fn validate_refs_dedupes_by_key() {
        let entries = vec![
            ref_entry("smith2024", "Smith", "2024", "On X", 50),
            ref_entry("SMITH2024", "Smith, J.", "2024", "On X (better)", 51), // case insensitive
            ref_entry("smith2024", "Smith", "2024", "dup", 52),
        ];
        let refs = validate_references(entries, 100);
        assert_eq!(refs.len(), 1);
        // First occurrence wins.
        assert_eq!(refs[0].authors, "Smith");
        assert_eq!(refs[0].page_in_doc, 50);
    }

    #[test]
    fn validate_refs_synthesizes_key_when_missing() {
        let mut e = ref_entry("", "Smith", "2024", "T", 10);
        e.key = None;
        let refs = validate_references(vec![e], 100);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].key, "smith2024");
    }

    #[test]
    fn validate_refs_caps_at_max() {
        let entries: Vec<LlmRefEntryWire> = (0..MAX_REFERENCES + 30)
            .map(|i| ref_entry(&format!("k{i}"), "A", "2024", "T", 1))
            .collect();
        let refs = validate_references(entries, 100);
        assert_eq!(refs.len(), MAX_REFERENCES);
    }
}
