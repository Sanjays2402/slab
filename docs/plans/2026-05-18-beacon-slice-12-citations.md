# Beacon Bonus Slice 12 — Citations Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task. Or for a cron tick: execute end-to-end on `feature/v1.6.0-beacon-bonus-12-citations` and merge via MODE A.

**Goal:** Scan the open PDF for inline citations like `(Smith 2024)` / `[Smith et al., 2024]` / `[12]`, link them to a built References / Bibliography table extracted from the doc's end-matter, and expose the whole thing as a sidebar panel with a one-click "jump to source" affordance.

**Architecture:** Pure-Rust regex + lightweight LLM enrichment, mirroring the shape of `ai::pii` (regex first, optional LLM second) and `ai::outline` (validated, deduped, JSON-shaped result). Citation detection is **deterministic regex** — fast, no provider needed. References extraction is **LLM-assisted** (the model parses the last 10% of pages, returns structured JSON of `{key, authors, year, title, page}` entries). Inline citations are then **matched** against the references table by key (author/year). Everything is wired through one new Tauri command `slab_beacon_find_citations` returning `CitationReport { inline: Vec<InlineCite>, references: Vec<Reference>, summary: CitationSummary }`. UI is a new `BeaconCitationsPanel.svelte` sidebar entry (`📑 Citations` nav row) that lists references with expand/collapse and an "X mentions" badge per reference, plus a `slab:beacon-goto-page` event when the user clicks a mention chip.

**Tech Stack:** Rust 1.x (regex, serde, async-trait, tokio), existing `ai::AiProvider` trait, `pdf::extract::extract_text`, Tauri 2 commands, Svelte 5 (runes), TypeScript.

---

## High-level shape

```
ai/citations.rs                           ←  new module (pure Rust + 1 LLM call)
  pub struct InlineCite { page, text, key, authors_hint, year_hint }
  pub struct Reference  { key, authors, year, title, page_in_doc }
  pub struct CitationReport { inline, references, summary }
  pub struct CitationSummary { inline_total, references_total, linked, orphans }
  pub fn scan_inline_citations(pages) -> Vec<InlineCite>          // regex, no IO
  pub(super) fn parse_llm_references(raw) -> Option<…>            // liberal JSON
  pub(super) fn validate_references(entries, pages_total) -> Vec<Reference>
  pub(super) fn link(inline: Vec<InlineCite>, refs: &[Reference]) -> (linked, orphans)
  pub async fn find_citations(provider, pages, opts) -> Result<CitationReport>
  pub async fn find_citations_from_path(provider, pdf_path, opts) -> Result<CitationReport>

src-tauri/src/lib.rs
  + use ai::citations::{find_citations_from_path as do_beacon_find_citations,
                        CitationReport, CitationOpts, DEFAULT_CITATIONS_MAX_CHARS}
  + #[tauri::command] async fn slab_beacon_find_citations(...)

src/lib/panels/BeaconCitationsPanel.svelte                       ← new sidebar panel
src/routes/+page.svelte
  + features.push({ id: "citations", label: "Citations", icon: "📑", ready: true })
  + nav routing branch
```

---

## Task 1: Scaffold `ai::citations` module with types

**Objective:** Create the empty module file with public types and module declaration; no logic yet, just compilable scaffolding.

**Files:**
- Create: `src-tauri/src/ai/citations.rs`
- Modify: `src-tauri/src/ai/mod.rs:25-28` (add `pub mod citations;` next to `outline`)

**Step 1: Write the new module file**

Create `src-tauri/src/ai/citations.rs` with this exact content:

```rust
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
```

**Step 2: Register the module**

Edit `src-tauri/src/ai/mod.rs` around line 25 — add `pub mod citations;` alphabetically (between `chunker` and `config`, or between `chat` and `chunker` — pick the spot that keeps alphabetical order):

```rust
pub mod chat;
pub mod chunker;
pub mod citations;        // ← add this line
pub mod config;
```

**Step 3: Verify it compiles + tests pass**

Run: `cd src-tauri && cargo test --lib ai::citations -- --nocapture`
Expected: 2 tests pass.

**Step 4: Commit**

```bash
git checkout -b feature/v1.6.0-beacon-bonus-12-citations
cat > /tmp/msg.txt <<'EOF'
feat(beacon): scaffold ai::citations module with public types

Slice 12 of the Beacon Bonus track. Introduces the types we'll fill in
over the next 7 tasks: InlineCite, Reference, CitationReport,
CitationSummary, CitationOpts. No logic yet — pure scaffolding so the
upcoming TDD passes have a stable type surface to land on.

Mirrors the layout of ai::outline and ai::pii: small constants up top,
public types, default impls, two smoke tests for serde round-trip and
defaults.
EOF
git add src-tauri/src/ai/citations.rs src-tauri/src/ai/mod.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 2: Inline citation regex scanner

**Objective:** Implement `scan_inline_citations(pages: &[String]) -> Vec<InlineCite>` — pure regex, no IO, no LLM. Three patterns: author-year, bracket-num, bracket-key.

**Files:**
- Modify: `src-tauri/src/ai/citations.rs` (append fn + tests inside existing `tests` module)

**Step 1: Add failing tests first** at the bottom of the existing `tests` module (above its closing `}`):

```rust
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
        // Build a single page with way too many cites.
        let blob: String = (0..MAX_INLINE_CITES + 50)
            .map(|i| format!("(Author{i} 2024) "))
            .collect();
        let pages = vec![blob];
        let cites = scan_inline_citations(&pages);
        assert_eq!(cites.len(), MAX_INLINE_CITES);
    }
```

**Step 2: Run tests to verify failure**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: tests fail because `scan_inline_citations` is undefined.

**Step 3: Add the regex scanner**

Insert this above the `#[cfg(test)]` block:

```rust
use regex::Regex;

/// Lazily-compiled regexes shared across calls.
fn re_author_year() -> &'static Regex {
    // Matches "(Smith 2024)", "(Smith, 2024)", "(Smith and Jones 2024)",
    // "(Smith et al. 2024)", "(Smith et al., 2024)".
    // First capture: author surname(s) blob. Second: 4-digit year.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\(([A-Z][A-Za-z'\-]+(?:\s+(?:and|et\s+al\.?|&)\s+[A-Z][A-Za-z'\-]+)*),?\s+((?:19|20)\d{2})\)").unwrap()
    })
}

fn re_bracket_num() -> &'static Regex {
    // Matches "[12]", "[12, 14]", "[12-15]". We use one regex for the
    // outer "[...]" and parse the content with a small helper.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[(\d+(?:\s*[,\u2013-]\s*\d+)*)\]").unwrap())
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
```

**Step 4: Run tests to verify pass**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: all 7 tests pass (2 from Task 1 + 5 new ones).

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations): add inline citation regex scanner

Handles three citation styles:
- Author-year: (Smith 2024), (Smith et al., 2025), (Smith and Jones 2024)
- Bracket-numeric: [12], [14, 15], [12-15] (range/list expansion)
- Bracket-key: [Smith2024], [smith-2024-foo]

Pure, deterministic, no IO. Bracket-numeric ranges are clamped at 20
expansions per bracket so a typo like [1-9999] doesn't poison the report.
A hard MAX_INLINE_CITES (2000) prevents runaway scans on hostile docs.

Five new unit tests pin behaviour against fixture strings — no PDF, no
provider, just deterministic regex.
EOF
git add src-tauri/src/ai/citations.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 3: LLM references parser (liberal JSON)

**Objective:** Add `parse_llm_references` — same pattern as `outline::parse_llm_outline`: strip markdown fences, find outermost `{...}`, attempt serde decode. Returns `Option<LlmRefsWire>`.

**Files:**
- Modify: `src-tauri/src/ai/citations.rs`

**Step 1: Write failing tests** (append to the `tests` module):

```rust
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
```

**Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 3 new tests fail to compile (undefined symbol).

**Step 3: Add the parser** — insert above the existing `pub fn scan_inline_citations`:

```rust
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
```

**Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 10 tests pass.

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations): liberal LLM references JSON parser

Mirrors ai::outline::parse_llm_outline — strips ```json fences, finds
the outermost {...} block, then tries serde_json. Returns None on any
parse failure so the UI can degrade gracefully (empty references but
keep the regex-derived inline cites).

Three new tests cover plain JSON, fenced-with-chatter, and the
garbage-input case.
EOF
git add src-tauri/src/ai/citations.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 4: Validate + dedupe references

**Objective:** Convert the `Vec<LlmRefEntryWire>` into a clean `Vec<Reference>`, dropping invalid entries and deduplicating by canonical key. Enforce `MAX_REFERENCES` cap.

**Files:**
- Modify: `src-tauri/src/ai/citations.rs`

**Step 1: Write failing tests** (append to `tests` module):

```rust
    fn ref_entry(
        key: &str,
        authors: &str,
        year: &str,
        title: &str,
        page: u32,
    ) -> LlmRefEntryWire {
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
```

**Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 4 new tests fail (undefined `validate_references`).

**Step 3: Add `validate_references`** — insert above the existing `scan_inline_citations`:

```rust
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
```

**Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 14 tests pass.

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations): validate, dedupe, and cap references

validate_references converts the liberal LLM payload into a clean
Vec<Reference>:
- Drops entries with out-of-range page or empty authors+title.
- Synthesizes a key from "<first-author><year>" when missing.
- Dedupes by lowercased key, first-occurrence wins.
- Caps at MAX_REFERENCES (500) to defend against runaway models.

Four new tests cover invalid pages, dedupe (case-insensitive), key
synthesis, and the cap.
EOF
git add src-tauri/src/ai/citations.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 5: Link inline cites to references + build summary

**Objective:** Implement `link(inline, references) -> (linked_count, orphans_count)` and a small helper that produces a `CitationSummary`. Linking is by lowercased key equality.

**Files:**
- Modify: `src-tauri/src/ai/citations.rs`

**Step 1: Write failing tests** (append):

```rust
    #[test]
    fn links_inline_to_references_by_key() {
        let inline = vec![
            InlineCite {
                page: 1,
                text: "(Smith 2024)".into(),
                key: "smith2024".into(),
                authors_hint: "smith".into(),
                year_hint: "2024".into(),
            },
            InlineCite {
                page: 2,
                text: "(Jones 2023)".into(),
                key: "jones2023".into(),
                authors_hint: "jones".into(),
                year_hint: "2023".into(),
            },
        ];
        let refs = vec![Reference {
            key: "smith2024".into(),
            authors: "Smith".into(),
            year: "2024".into(),
            title: "T".into(),
            page_in_doc: 80,
        }];
        let (linked, orphans) = count_links(&inline, &refs);
        assert_eq!(linked, 1);
        assert_eq!(orphans, 1);
    }

    #[test]
    fn summary_from_components_is_consistent() {
        let inline = vec![InlineCite {
            page: 1,
            text: "[12]".into(),
            key: "12".into(),
            authors_hint: String::new(),
            year_hint: String::new(),
        }];
        let refs = vec![Reference {
            key: "12".into(),
            authors: "X".into(),
            year: "2024".into(),
            title: "T".into(),
            page_in_doc: 80,
        }];
        let s = build_summary(&inline, &refs);
        assert_eq!(s.inline_total, 1);
        assert_eq!(s.references_total, 1);
        assert_eq!(s.linked, 1);
        assert_eq!(s.orphans, 0);
    }
```

**Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 2 new tests fail (undefined `count_links`, `build_summary`).

**Step 3: Add the helpers** — insert above `scan_inline_citations`:

```rust
/// Count how many inline cites have a matching reference (by lowercased key)
/// vs. how many are orphans.
pub(super) fn count_links(inline: &[InlineCite], refs: &[Reference]) -> (u32, u32) {
    use std::collections::HashSet;
    let ref_keys: HashSet<&str> = refs.iter().map(|r| r.key.as_str()).collect();
    let mut linked = 0u32;
    let mut orphans = 0u32;
    for c in inline {
        if ref_keys.contains(c.key.as_str()) {
            linked += 1;
        } else {
            orphans += 1;
        }
    }
    (linked, orphans)
}

/// Build a `CitationSummary` from already-validated components.
pub(super) fn build_summary(inline: &[InlineCite], refs: &[Reference]) -> CitationSummary {
    let (linked, orphans) = count_links(inline, refs);
    CitationSummary {
        inline_total: inline.len() as u32,
        references_total: refs.len() as u32,
        linked,
        orphans,
    }
}
```

**Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 16 tests pass.

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations): link inline cites to references + summary

count_links iterates inline cites and counts hits/misses against the
references' key set. build_summary wraps it with totals for the UI
footer.

Two new tests pin the linking math.
EOF
git add src-tauri/src/ai/citations.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 6: Async `find_citations` entry point + path wrapper

**Objective:** Wire everything together: extract text, regex-scan inline, slice end-matter, call the AI provider, parse JSON, validate, build summary, return `CitationReport`. Plus a `find_citations_from_path` wrapper that reads the PDF first.

**Files:**
- Modify: `src-tauri/src/ai/citations.rs`

**Step 1: Add the system prompt + helpers** — insert above `scan_inline_citations`:

```rust
const SYSTEM_PROMPT: &str = "You are Beacon, a bibliography extractor. \
The user shows you the last pages of an academic or technical PDF, which \
contains a References / Bibliography section. Reply with JSON ONLY, no \
prose, no markdown fences:\n\
{\"entries\":[{\"key\":\"...\",\"authors\":\"...\",\"year\":\"YYYY\",\"title\":\"...\",\"page\":N}]}\n\
- key: short stable id like \"smith2024\" or \"12\". If the entry has a \
visible number prefix (e.g. \"[12] Smith, J. ...\") use that as the key.\n\
- authors: comma-separated authors as printed.\n\
- year: 4-digit year, or empty string if absent.\n\
- title: the work's title (or first sentence if no title).\n\
- page: the 1-based page where the entry is printed.\n\
Skip headers (\"References\", \"Bibliography\"), page numbers, and \
footers. Limit to entries that look like real bibliography lines.";

/// Pick the last ~10% of pages (min 3, max 25) as candidate end-matter.
fn end_matter_pages(total: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let tail = ((total as f32 * 0.1).ceil() as usize).clamp(3, 25);
    let tail = tail.min(total);
    let start = total - tail;
    (start, total)
}

/// Build prompt for references extraction. Returns (msgs, pages_used).
fn build_refs_messages(pages: &[String], max_chars: usize) -> (Vec<ChatMessage>, u32) {
    let (lo, hi) = end_matter_pages(pages.len());
    let mut buf = String::new();
    let mut used = 0u32;
    for i in lo..hi {
        let page = &pages[i];
        let header = format!("\n--- PAGE {} ---\n", i + 1);
        if buf.len() + header.len() + page.len() > max_chars {
            break;
        }
        buf.push_str(&header);
        buf.push_str(page);
        used += 1;
    }
    let user = format!(
        "Below are the last pages of a PDF. Extract every bibliography \
         entry you can identify. Respond with JSON only.\n{buf}"
    );
    (
        vec![
            ChatMessage {
                role: ChatRole::System,
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: user,
            },
        ],
        used,
    )
}

/// Top-level: run the full citation pipeline against pre-extracted pages.
pub async fn find_citations(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    opts: &CitationOpts,
) -> Result<CitationReport, AiError> {
    // Pre-flight chunker (fail fast on truly empty docs).
    let _chunks = chunk_pages(pages);

    let inline = scan_inline_citations(pages);
    let mut references: Vec<Reference> = Vec::new();
    let mut model = String::new();

    if opts.include_llm_pass && !pages.is_empty() {
        let (msgs, _pages_used) =
            build_refs_messages(pages, opts.max_context_chars as usize);
        let chat_opts = ChatOpts {
            temperature: Some(0.1),
            max_tokens: Some(8_000),
            ..Default::default()
        };
        let resp = provider.chat(&msgs, &chat_opts).await?;
        model = resp.model;
        let wire = parse_llm_references(&resp.content).unwrap_or(LlmRefsWire {
            entries: Vec::new(),
        });
        references = validate_references(wire.entries, pages.len() as u32);
    }

    let summary = build_summary(&inline, &references);
    Ok(CitationReport {
        inline,
        references,
        summary,
        model,
    })
}

/// Convenience: read PDF text from disk, then run `find_citations`.
pub async fn find_citations_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    opts: &CitationOpts,
) -> Result<CitationReport, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    find_citations(provider, &pages, opts).await
}
```

**Step 2: Add the integration test using a MockProvider** (append to `tests` module — reuse the same MockProvider pattern as `outline.rs`):

```rust
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockProvider {
        reply: String,
        captured: Mutex<Vec<ChatMessage>>,
    }

    impl MockProvider {
        fn new(reply: &str) -> Self {
            Self {
                reply: reply.into(),
                captured: Mutex::new(Vec::new()),
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
                model: "mock-citations:test".into(),
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
    async fn find_citations_happy_path() {
        // Page 1: body with cites. Page 2: bibliography.
        let pages = vec![
            "Foundational result (Smith 2024) and [12].".to_string(),
            "[12] Smith, J. (2024). On X. Journal of X, 1(1).".to_string(),
        ];
        let reply = r#"{"entries":[
            {"key":"smith2024","authors":"Smith, J.","year":"2024","title":"On X","page":2},
            {"key":"12","authors":"Smith, J.","year":"2024","title":"On X","page":2}
        ]}"#;
        let provider = Arc::new(MockProvider::new(reply));
        let opts = CitationOpts::default();
        let report = find_citations(provider, &pages, &opts).await.unwrap();
        // Two inline cites: (Smith 2024) on page 1, [12] on page 1.
        assert_eq!(report.inline.len(), 2);
        // Two references parsed.
        assert_eq!(report.references.len(), 2);
        // Both inline cites should link.
        assert_eq!(report.summary.linked, 2);
        assert_eq!(report.summary.orphans, 0);
        assert_eq!(report.model, "mock-citations:test");
    }

    #[tokio::test]
    async fn find_citations_without_llm_pass_skips_references() {
        let pages = vec!["See (Smith 2024).".to_string()];
        let provider = Arc::new(MockProvider::new("should-not-be-called"));
        let opts = CitationOpts {
            include_llm_pass: false,
            max_context_chars: 1000,
        };
        let report = find_citations(provider, &pages, &opts).await.unwrap();
        assert_eq!(report.inline.len(), 1);
        assert!(report.references.is_empty());
        assert_eq!(report.summary.orphans, 1);
        assert_eq!(report.model, "");
    }
```

**Step 3: Run to verify pass**

Run: `cd src-tauri && cargo test --lib ai::citations`
Expected: 18 tests pass.

**Step 4: Run the full quality gates**

Run: `cd src-tauri && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --lib`
Expected: fmt clean, no clippy errors, all tests pass.

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations): async find_citations entry point + path wrapper

Wires together the full pipeline:
1. extract page text (already done by the path wrapper),
2. regex-scan inline cites (deterministic, no provider),
3. slice the last ~10% of pages (min 3, max 25) as end-matter,
4. ask the AI provider for structured references JSON (skipped when
   include_llm_pass=false — degraded mode for offline use),
5. parse + validate + dedupe,
6. build CitationSummary, return CitationReport.

Includes a MockProvider-backed happy-path test plus a skip-llm-pass
test to cover the offline path. The end-matter slice picks the trailing
10% of pages clamped to [3, 25] — small papers still get their full
back matter, monster books don't blow the context budget.
EOF
git add src-tauri/src/ai/citations.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 7: Expose `slab_beacon_find_citations` Tauri command

**Objective:** Add a new Tauri command following the exact shape of `slab_beacon_propose_outline`. Registered in the `invoke_handler` macro.

**Files:**
- Modify: `src-tauri/src/lib.rs` (add use at top ~ line 24, command ~ line 783 next to propose_outline, register ~ line 1958)

**Step 1: Add the use statement** — extend the existing `ai::outline` use block (line ~24):

```rust
use ai::citations::{
    find_citations_from_path as do_beacon_find_citations, CitationOpts, CitationReport,
    DEFAULT_CITATIONS_MAX_CHARS,
};
```

Place it alphabetically between the `chunker` (if present) and `config` imports, or wherever it fits naturally — just keep the file's existing organisation.

**Step 2: Add the command** — insert right after `slab_beacon_propose_outline` (the `}` at line ~782):

```rust
/// Beacon Citations — scan the PDF for inline citations and extract a
/// structured References list from end-matter. Returns a `CitationReport`
/// that the front-end can render as a sidebar panel with mention chips
/// and "jump to bibliography" links. v1.6.0 Beacon Bonus Slice 12.
#[tauri::command]
async fn slab_beacon_find_citations(
    pdf_path: PathBuf,
    opts: Option<CitationOpts>,
) -> CmdResult<CitationReport> {
    let cfg = match do_load_beacon_config() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let provider = match ai::config::make_provider(&cfg.beacon) {
        Ok(p) => p,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let opts = opts.unwrap_or_default();
    do_beacon_find_citations(provider, &pdf_path, &opts).await.into()
}
```

**Step 3: Register the handler** — add `slab_beacon_find_citations,` to the `invoke_handler!` macro list near where `slab_beacon_propose_outline` is registered (line ~1958):

```rust
            slab_beacon_propose_outline,
            slab_beacon_find_citations,    // ← add this line
            slab_beacon_diff_summary,
```

**Step 4: Verify compilation + quality gates**

Run:
- `cd src-tauri && cargo build --lib`
- `cd src-tauri && cargo fmt --all -- --check`
- `cd src-tauri && cargo clippy --all-targets -- -D warnings`
- `cd src-tauri && cargo test --lib`
- `cd .. && pnpm check`

Expected: all green.

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations): expose slab_beacon_find_citations Tauri command

New IPC entry point with the same shape as slab_beacon_propose_outline:
load config → make provider → call into ai::citations::find_citations_from_path
→ return CitationReport. Optional CitationOpts lets the UI pass
include_llm_pass=false for an offline (regex-only) scan.

Registered in the invoke_handler! block alongside the other Beacon
commands. No new dependencies — reuses the existing AiProvider plumbing.
EOF
git add src-tauri/src/lib.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt
```

---

## Task 8: Front-end — BeaconCitationsPanel + sidebar nav entry

**Objective:** Add a new sidebar panel that lets the user scan the current PDF (or pick one) and renders the report: references list with mention badges + page-jump on click.

**Files:**
- Create: `src/lib/panels/BeaconCitationsPanel.svelte`
- Modify: `src/routes/+page.svelte` — `features` array (~line 67), the long routing branch (~line 686-689), the detached-shell branch (~line 517-520), and the imports block (~line 32-34).

**Step 1: Create the panel** at `src/lib/panels/BeaconCitationsPanel.svelte`:

```svelte
<script lang="ts">
  // Beacon Citations panel — scans the current PDF for inline citations,
  // extracts a structured References table, and links inline mentions to
  // their bibliography entries. Workflow:
  //
  //   1. User picks (or inherits via `slab:open-recent`) a PDF.
  //   2. Click "Scan citations" → `slab_beacon_find_citations`.
  //   3. Render references list with mention-count badges. Expanding a
  //      reference shows the inline mentions, each as a chip that fires
  //      `slab:beacon-goto-page` on click.
  //   4. Footer shows totals: "N references · M mentions · K orphans".
  //
  // Design notes:
  // - Same friendly-error mapping as BeaconChatPanel / BeaconSearchPanel.
  // - Offline toggle: "Skip LLM (regex inline only)" disables the
  //   references extraction so users without Ollama still get value.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type InlineCite = {
    page: number;
    text: string;
    key: string;
    authors_hint: string;
    year_hint: string;
  };
  type Reference = {
    key: string;
    authors: string;
    year: string;
    title: string;
    page_in_doc: number;
  };
  type CitationSummary = {
    inline_total: number;
    references_total: number;
    linked: number;
    orphans: number;
  };
  type CitationReport = {
    inline: InlineCite[];
    references: Reference[];
    summary: CitationSummary;
    model: string;
  };

  let pdfPath = $state<string | null>(null);
  let report = $state<CitationReport | null>(null);
  let includeLlm = $state(true);
  let expanded = $state<Set<string>>(new Set());
  let status = $state<Status>(idle);

  onMount(() => {
    const onOpenRecent = (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        report = null;
      }
    };
    window.addEventListener("slab:open-recent", onOpenRecent);
    return () => window.removeEventListener("slab:open-recent", onOpenRecent);
  });

  async function pickPdf() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    pdfPath = picked;
    report = null;
    status = idle;
  }

  async function scan() {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = { kind: "working", msg: "Scanning citations…" };
    try {
      const res = await invoke<CmdResult<CitationReport>>(
        "slab_beacon_find_citations",
        {
          pdfPath,
          opts: { include_llm_pass: includeLlm, max_context_chars: 40000 },
        },
      );
      if (res.kind === "ok") {
        report = res.value;
        status = { kind: "ok", msg: `Found ${report.summary.inline_total} cites, ${report.summary.references_total} refs.` };
      } else {
        status = { kind: "err", msg: friendly(res.message) };
      }
    } catch (e) {
      status = { kind: "err", msg: friendly(String(e)) };
    }
  }

  function friendly(raw: string): string {
    const m = raw.toLowerCase();
    if (m.includes("provider unavailable") || m.includes("connect")) {
      return "Beacon provider not reachable. Start Ollama or pick a different provider in Settings.";
    }
    if (m.includes("rate limited") || m.includes("429")) {
      return "Beacon rate-limited the request. Try again in a moment.";
    }
    return raw;
  }

  function toggle(key: string) {
    if (expanded.has(key)) {
      expanded.delete(key);
    } else {
      expanded.add(key);
    }
    expanded = new Set(expanded);
  }

  function mentionsOf(key: string): InlineCite[] {
    if (!report) return [];
    return report.inline.filter((c) => c.key === key);
  }

  function gotoPage(page: number) {
    if (!pdfPath) return;
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", {
        detail: { path: pdfPath, page },
      }),
    );
  }
</script>

<section class="panel citations">
  <header>
    <h2>📑 Citations</h2>
    <p class="hint">Find inline citations and link them to the bibliography.</p>
  </header>

  <div class="pdf-row">
    <button onclick={pickPdf}>{pdfPath ? basename(pdfPath) : "Pick PDF…"}</button>
    <label class="llm-toggle">
      <input type="checkbox" bind:checked={includeLlm} />
      Extract bibliography (LLM)
    </label>
    <button class="primary" onclick={scan} disabled={!pdfPath || status.kind === "working"}>
      {status.kind === "working" ? "Scanning…" : "Scan citations"}
    </button>
  </div>

  {#if status.kind === "err"}
    <p class="err">{status.msg}</p>
  {/if}

  {#if report}
    <p class="summary">
      <strong>{report.summary.references_total}</strong> references ·
      <strong>{report.summary.inline_total}</strong> inline ·
      <span class="linked">{report.summary.linked} linked</span> ·
      <span class="orphans">{report.summary.orphans} orphans</span>
      {#if report.model}<span class="model">via {report.model}</span>{/if}
    </p>

    {#if report.references.length === 0 && report.inline.length === 0}
      <p class="empty">No citations or references detected.</p>
    {/if}

    {#if report.references.length > 0}
      <ul class="refs">
        {#each report.references as r (r.key)}
          {@const mentions = mentionsOf(r.key)}
          <li>
            <button class="ref-row" onclick={() => toggle(r.key)} aria-expanded={expanded.has(r.key)}>
              <span class="badge">{mentions.length}</span>
              <span class="ref-text">
                <strong>{r.authors || "Unknown"}</strong>
                {#if r.year}<span class="year">({r.year})</span>{/if}
                <span class="title">{r.title}</span>
              </span>
              <button
                class="jump"
                onclick={(e) => { e.stopPropagation(); gotoPage(r.page_in_doc); }}
                title="Jump to bibliography page">
                p.{r.page_in_doc} →
              </button>
            </button>
            {#if expanded.has(r.key)}
              <ul class="mentions">
                {#each mentions as m (`${m.page}-${m.text}`)}
                  <li>
                    <button class="chip" onclick={() => gotoPage(m.page)}>
                      <code>{m.text}</code> · p.{m.page}
                    </button>
                  </li>
                {/each}
                {#if mentions.length === 0}
                  <li class="no-mentions">no inline mentions found</li>
                {/if}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if report.summary.orphans > 0}
      <details class="orphans-block">
        <summary>{report.summary.orphans} orphan inline cite(s) — no matching reference</summary>
        <ul class="mentions">
          {#each report.inline.filter((c) => !report.references.some((r) => r.key === c.key)) as c (`${c.page}-${c.text}`)}
            <li>
              <button class="chip orphan" onclick={() => gotoPage(c.page)}>
                <code>{c.text}</code> · p.{c.page}
              </button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {/if}
</section>

<style>
  .panel.citations {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
  }
  header h2 {
    margin: 0;
    font-size: 18px;
  }
  .hint {
    color: var(--text-2, #888);
    font-size: 13px;
    margin: 4px 0 0;
  }
  .pdf-row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .llm-toggle {
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .primary {
    font-weight: 600;
  }
  .err {
    color: var(--err, #c33);
    font-size: 13px;
    margin: 0;
  }
  .summary {
    font-size: 13px;
    color: var(--text-2, #666);
    margin: 0;
  }
  .summary .linked { color: var(--ok, #2a8); }
  .summary .orphans { color: var(--warn, #c80); }
  .summary .model { opacity: 0.6; margin-left: 6px; }
  .empty {
    color: var(--text-2, #888);
    font-size: 13px;
    margin-top: 12px;
  }
  ul.refs {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ref-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: transparent;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
  }
  .ref-row:hover { background: var(--hover, #1a1a1a); }
  .badge {
    background: var(--accent, #4af);
    color: white;
    border-radius: 9999px;
    font-size: 11px;
    padding: 2px 8px;
    flex-shrink: 0;
    font-weight: 600;
  }
  .ref-text {
    flex: 1;
    font-size: 13px;
  }
  .ref-text .year {
    color: var(--text-2, #888);
    margin-left: 4px;
  }
  .ref-text .title {
    display: block;
    color: var(--text-2, #aaa);
    font-size: 12px;
    margin-top: 2px;
  }
  .jump {
    font-size: 11px;
    padding: 2px 8px;
    background: var(--hover, #222);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    color: inherit;
  }
  .jump:hover { background: var(--accent, #4af); color: white; }
  ul.mentions {
    list-style: none;
    margin: 6px 0 6px 32px;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    background: var(--hover, #222);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 4px;
    padding: 4px 8px;
    cursor: pointer;
    font-size: 12px;
    color: inherit;
  }
  .chip:hover { background: var(--accent, #4af); color: white; }
  .chip.orphan { border-color: var(--warn, #c80); }
  .no-mentions {
    font-size: 12px;
    color: var(--text-2, #888);
    padding: 4px 0;
  }
  .orphans-block summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--warn, #c80);
  }
</style>
```

**Step 2: Wire it into `src/routes/+page.svelte`**

Edit 4 spots:

1. Import (~line 34, alphabetically after `BeaconChatPanel`):
```svelte
  import BeaconCitationsPanel from "$lib/panels/BeaconCitationsPanel.svelte";
```

2. Features array (~line 72, after the `pii` row):
```svelte
    { id: "citations", label: "Citations", icon: "📑", ready: true },
```

3. The `reader-set` array (~line 141, after `"pii"`):
```svelte
    "citations",
```

4. The detached-shell branch (~line 519, after `"pii"`):
```svelte
    {:else if detachedPanel === "citations"}
      <BeaconCitationsPanel />
```

5. The main routing block (~line 689, after `"pii"`):
```svelte
  {:else if active === "citations"}
    <BeaconCitationsPanel />
```

**Step 3: Verify svelte-check + build**

Run:
- `pnpm check`        (expect 0 errors)
- `pnpm build`        (expect successful build)

If `pnpm check` warns about a `@const` block where types are loose, those are pre-existing warnings — verify warning count hasn't increased.

**Step 4: Final cross-stack quality gates**

Run all five from the repo root:

```bash
cd src-tauri && cargo fmt --all -- --check
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test --lib
cd .. && pnpm check
cd .. && pnpm build
```

Expected: all green. `cargo test --lib` count should be `(prior baseline + 18 new citation tests)`.

**Step 5: Commit + push branch**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/citations-ui): Citations panel + sidebar nav entry

Adds the 📑 Citations row to the sidebar nav and a new
BeaconCitationsPanel that:
- picks a PDF (or inherits via the slab:open-recent channel),
- offers an "Extract bibliography (LLM)" toggle for offline mode,
- runs slab_beacon_find_citations,
- renders a references list with expand/collapse mention chips,
- shows an orphans block for inline cites with no matching reference,
- emits slab:beacon-goto-page on click so the Reader can jump.

Reuses the friendly-error mapping pattern from BeaconChatPanel /
BeaconSearchPanel. No new dependencies, no new IPC plumbing beyond the
command added in the previous task.
EOF
git add src/lib/panels/BeaconCitationsPanel.svelte src/routes/+page.svelte
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' commit -F /tmp/msg.txt

# Push branch
TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' '$TOK'; }; f" \
    push -u origin feature/v1.6.0-beacon-bonus-12-citations
```

---

## Done — promotion to MODE A

After Task 8's push, this branch is `STATUS: DONE` and ready for MODE A merge.

Update `.cron-state/STATE.md` (top section) with:

```markdown
## STATUS: ✦ v1.6.0 Beacon Bonus Slice 12 "Citations" DONE on feature/v1.6.0-beacon-bonus-12-citations — MERGE next tick

**Feature branch**: `feature/v1.6.0-beacon-bonus-12-citations`
**Commits**: 8 (scaffold → regex → parser → validate → link → entry-point → command → UI)
**New tests**: 18 unit/integration in `ai::citations`
**Quality gates**: cargo fmt/clippy/test + pnpm check + pnpm build all green
**Next tick**: MODE A merge → bump to v1.6.0 → tag → push → kick CI → next tick finalize.
```

---

## Plan summary

```
8 tasks, ~50 minutes of focused work
4 files touched on the Rust side  (mod.rs registers, citations.rs is the work, lib.rs adds command, plus the existing pdf::extract::extract_text reuse)
2 files touched on the front-end  (+page.svelte 4-line surgery, new BeaconCitationsPanel.svelte)
18 new unit/integration tests
1 new Tauri command: slab_beacon_find_citations
0 new dependencies (regex + serde + tokio + Arc are all already in tree)

Estimated total LoC added: ~1100 (Rust ~700, Svelte ~400)
```

**Architectural fit:** identical layering to ai::outline (regex → liberal parser → validate → async entry → path wrapper → Tauri command → Svelte panel). A maintainer reading both modules side-by-side will see the family resemblance immediately.

**YAGNI checks:**
- No persistence — citations are computed on-demand. (If users want them cached, a future slice can add a `~/.slab/citations.db` keyed by content hash.)
- No "export bibliography to BibTeX" button — out of scope; user can copy the rendered list.
- No cross-doc citation graphs — out of scope; would need the embedding index, that's a separate slice.

**DRY checks:**
- `parse_llm_references` deliberately mirrors `outline::parse_llm_outline`. Could be extracted into a helper later but they're different shapes; leaving the parallel intentional.
- `end_matter_pages` is local because there's no other consumer.
- Friendly-error mapping in the panel is local — three Beacon panels now have their own copy. If a 4th one lands, lift it into `$lib/types.ts`.
