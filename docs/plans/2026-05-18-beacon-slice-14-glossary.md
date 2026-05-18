# Beacon Bonus Slice 14 — "Glossary" Implementation Plan

> **For Hermes:** This plan is executed in MODE C by the autonomous cron.
> Walk it task-by-task. After Task 8, run the batched quality gates
> (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --lib`,
> `pnpm check`), push the branch, and flip STATE.md to `STATUS: DONE`.

**Goal:** Mine an opened PDF for domain-specific terminology — acronyms,
italicised technical terms, defined-on-first-use phrases — and emit a
clean, alphabetised glossary with definitions cited back to the source
page. Cached per-PDF in a single sidecar JSON file at
`~/.slab/glossary/<pdf_hash>.json`, so the second visit is instant and the
LLM only runs once per document (or on explicit Refresh).

**Architecture:**
1. **`ai::glossary`** — pure pipeline:
   - Pass 1 (deterministic): regex scan of every page surfacing four
     candidate kinds — `Acronym` (`AI`, `LLM`, `RAG`, `NIST SP 800-53`),
     `DefinedOnFirstUse` (`X (foo bar baz)` and `X — foo bar baz` patterns),
     `Italicised` (`*term*` / `_term_` / `<i>term</i>` — Slab strips
     formatting but caps + position heuristics survive), and `CapitalisedPhrase`
     (low-frequency Title-Case multi-word phrases not at sentence start).
   - Pass 2 (LLM): hand the top-N candidates + their first-use surrounding
     context to the provider, ask for `{entries:[{term,definition,page,
     confidence,kind}]}`, liberal JSON parsed.
   - Validate: drop empty terms, drop definitions < 8 chars, dedupe by
     case-folded term, cap at `MAX_GLOSSARY_ENTRIES`, sort alphabetically.
   - Top-level: `build_glossary(provider, pages, opts) -> GlossaryReport`.
   Mirrors the layering of `ai::citations` (regex first, LLM second,
   liberal parser, validate, link, summary).
2. **`ai::glossary_cache`** — sidecar JSON cache keyed by `pdf_hash`. No
   sqlite needed — glossary is tiny (50-200 entries), read whole/write
   whole. Schema-versioned via a top-level `version: u32` field. Tests use
   an in-memory `Path` via `tempfile`.
3. **Tauri commands** in `src-tauri/src/lib.rs`:
   - `slab_beacon_build_glossary` — generate + persist, return report.
   - `slab_beacon_load_glossary` — return cached report or `None`.
   - `slab_beacon_clear_glossary` — wipe cache for a `pdf_hash`.
4. **`BeaconGlossaryPanel.svelte`** — alphabetical list with letter jump
   nav (A B C D …), search-filter at the top, per-entry: term + definition
   + kind chip + page citation (click → `slab:beacon-goto-page` event +
   `slab:beacon-highlight-term`), header "Build glossary" / "Refresh"
   button, footer entry-count.
5. **Sidebar nav entry** `{ id: "glossary", label: "Glossary", icon: "📖", ready: true }`
   in `src/routes/+page.svelte` between `study` and `merge`, panel mounted
   alongside Study + detached-window branch.

**Tech Stack:** regex 1.x (already a dep), serde, thiserror, async-trait,
existing `ai::AiProvider` trait, `pdf::extract::extract_text`, Svelte 5
runes, `@tauri-apps/api/core`. **No new external crates.**

**Branch:** `feature/v1.8.0-beacon-bonus-14-glossary`

**Pre-flight:**
```bash
cd /Users/sanjay/Projects/slab
git fetch origin && git checkout main && git pull --ff-only
git checkout -b feature/v1.8.0-beacon-bonus-14-glossary
```

---

## High-level shape

```
ai/glossary.rs                                ← new module (regex + 1 LLM call)
  pub struct GlossaryEntry { term, definition, page, confidence, kind, source_snippet }
  pub enum CandidateKind { Acronym, DefinedOnFirstUse, Italicised, CapitalisedPhrase }
  pub struct GlossaryReport { entries, summary, model }
  pub struct GlossarySummary { candidates_total, accepted, rejected, kept_by_kind }
  pub struct GlossaryOpts { include_llm_pass, max_candidates, max_context_chars }
  pub struct Candidate { term, page, kind, snippet }
  pub fn scan_candidates(pages) -> Vec<Candidate>             // regex, no IO
  pub(super) fn parse_llm_glossary(raw) -> Option<…>          // liberal JSON
  pub(super) fn validate_entries(raw, total_pages) -> Vec<GlossaryEntry>
  pub async fn build_glossary(provider, pages, opts) -> Result<GlossaryReport>
  pub async fn build_glossary_from_path(provider, pdf_path, opts) -> Result<GlossaryReport>

ai/glossary_cache.rs                          ← JSON sidecar
  pub fn cache_dir() -> PathBuf  →  ~/.slab/glossary/
  pub fn load(hash, dir) -> io::Result<Option<GlossaryReport>>
  pub fn save(hash, report, dir) -> io::Result<()>
  pub fn clear(hash, dir) -> io::Result<()>

src-tauri/src/lib.rs
  + use ai::glossary::{build_glossary_from_path as do_build, GlossaryReport, GlossaryOpts}
  + use ai::glossary_cache::{cache_dir as glossary_cache_dir, load/save/clear}
  + #[tauri::command] async fn slab_beacon_build_glossary(...)
  + #[tauri::command] async fn slab_beacon_load_glossary(...)
  + #[tauri::command] async fn slab_beacon_clear_glossary(...)

src/lib/panels/BeaconGlossaryPanel.svelte     ← new sidebar panel
src/routes/+page.svelte
  + features.push({ id: "glossary", label: "Glossary", icon: "📖", ready: true })
  + nav routing branch + detached-window branch
```

---

## Task 1: Scaffold `ai::glossary` module with types + module wiring

**Objective:** Create the new module file with public types, register it in
`ai/mod.rs`, write a stub `scan_candidates` returning `Vec::new()` so the
module compiles and feeds subsequent tasks a real signature to fill in.

**Files:**
- Create: `src-tauri/src/ai/glossary.rs`
- Modify: `src-tauri/src/ai/mod.rs` (add `pub mod glossary;` after `pub mod embedding_index;`)

**Step 1: Register the module**

Edit `src-tauri/src/ai/mod.rs`, in the alphabetised `pub mod` block:

```rust
pub mod embedding_index;
pub mod glossary;
pub mod ollama;
```

**Step 2: Create the module with types + scan stub + 2 sanity tests**

Write `src-tauri/src/ai/glossary.rs`:

```rust
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
```

**Step 3: Run tests**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::glossary 2>&1 | tail -8
```
Expected: `2 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/glossary): scaffold ai::glossary module + public types

Slice 14 of Beacon Bonus: Glossary Builder. This commit lands the
new module wired into ai::mod plus the public data types
(Candidate, CandidateKind, GlossaryEntry, GlossarySummary,
GlossaryReport, GlossaryOpts) and a `scan_candidates` stub.
Subsequent commits in this slice fill in candidate detection,
LLM definition extraction, validation, the sidecar JSON cache,
the Tauri commands, and the BeaconGlossaryPanel UI.
EOF
git add src-tauri/src/ai/mod.rs src-tauri/src/ai/glossary.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 2: Candidate detection (regex passes 1-4) + tests

**Objective:** Implement deterministic, pure-Rust regex scanning that
surfaces the four candidate kinds. No LLM. No IO. Heavily tested.

**Files:**
- Modify: `src-tauri/src/ai/glossary.rs` (replace the `scan_candidates`
  stub with full impl + helper regexes + 6 new unit tests)

**Step 1: Implement the four scanner passes**

In `src-tauri/src/ai/glossary.rs`, replace the stub `scan_candidates`
with the full impl. Add `use regex::Regex; use std::sync::OnceLock;` at
the top of the file alongside the existing `use serde::…` line, and add
the scanner block before the `#[cfg(test)]` line:

```rust
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Matches all-caps acronyms 2-10 chars long, optionally with numerics
/// or hyphenated suffixes (e.g. "NIST SP 800-53", "GPT-4o"). The lookahead
/// guards against picking up the first word of an all-caps section
/// header — we require a *lowercase* neighbour within ±20 chars at the
/// linking step (see `passes_acronym_neighbourhood`).
fn re_acronym() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b([A-Z][A-Z0-9]{1,9}(?:[- ][A-Z0-9]{1,8})?)\b").unwrap())
}

/// Matches "Term (expansion goes here)" with the expansion in parens.
/// The capture group 1 is the term-ish bit just before the paren.
fn re_defined_first_use_parens() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"([A-Z][A-Za-z][A-Za-z0-9\- ]{1,40})\s+\(([a-zA-Z][a-zA-Z0-9 ,\-]{8,140})\)",
        )
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
/// (the regex requires a leading lowercase word + space). Captures
/// the phrase only.
fn re_title_case_phrase() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?:[a-z]+ )((?:[A-Z][a-z]{2,15} ){1,4}[A-Z][a-z]{2,15})",
        )
        .unwrap()
    })
}

/// True iff the acronym has at least one lower-case ASCII letter within
/// `radius` chars on either side — proxy for "not in an all-caps header".
fn passes_acronym_neighbourhood(text: &str, match_start: usize, match_end: usize, radius: usize) -> bool {
    let lo = match_start.saturating_sub(radius);
    let hi = text.len().min(match_end + radius);
    text[lo..hi].chars().any(|c| c.is_ascii_lowercase())
}

/// Pull a ≤ 240-char window centred on the match.
fn snippet(text: &str, m_start: usize, m_end: usize) -> String {
    let lo = m_start.saturating_sub(80);
    let hi = text.len().min(m_end + 160);
    // utf-8 safe trim: shift to char boundary
    let safe_lo = (lo..=m_start).find(|i| text.is_char_boundary(*i)).unwrap_or(m_start);
    let safe_hi = (m_end..=hi).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(m_end);
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

        // Pass 1: DefinedOnFirstUse (parens form). Highest signal.
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
```

**Step 2: Add 6 new tests** at the end of the `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn scans_acronym_in_lowercase_neighbourhood() {
        let pages = vec!["RAG is widely used in retrieval pipelines.".into()];
        let cs = scan_candidates(&pages);
        assert!(cs.iter().any(|c| c.term == "RAG" && c.kind == CandidateKind::Acronym));
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
        let pages = vec!["We use Retrieval Augmented Generation (RAG, a hybrid LLM technique) extensively.".into()];
        let cs = scan_candidates(&pages);
        assert!(cs.iter().any(|c| {
            c.term.contains("Retrieval Augmented Generation")
                && c.kind == CandidateKind::DefinedOnFirstUse
        }));
    }

    #[test]
    fn picks_up_em_dash_definition() {
        let pages = vec!["A Transformer \u{2014} a deep learning architecture based on self-attention.".into()];
        let cs = scan_candidates(&pages);
        assert!(cs.iter().any(|c| c.term == "Transformer" && c.kind == CandidateKind::DefinedOnFirstUse));
    }

    #[test]
    fn picks_up_italicised_remnant() {
        let pages = vec!["the *attention mechanism* is central to this model.".into()];
        let cs = scan_candidates(&pages);
        assert!(cs.iter().any(|c| c.term == "attention mechanism" && c.kind == CandidateKind::Italicised));
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
        let pages = vec!["lowercase RAG appears. Now Retrieval Augmented Generation (RAG, a hybrid method) is introduced.".into()];
        let cs = scan_candidates(&pages);
        // First entry should be the DefinedOnFirstUse one.
        assert!(matches!(cs[0].kind, CandidateKind::DefinedOnFirstUse));
    }
```

**Step 3: Verify**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::glossary 2>&1 | tail -12
```
Expected: `9 passed; 0 failed` (2 from Task 1 + 7 new).

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/glossary): regex-based candidate detection across 4 patterns

Implements ai::glossary::scan_candidates — deterministic, pure-Rust,
no LLM, no IO. Four passes:

  1. DefinedOnFirstUse (parens form: "Term (expansion)")
  2. DefinedOnFirstUse (em-dash form: "Term — definition")
  3. Acronym (2-10 chars all-caps, gated on lowercase neighbour to
     reject all-caps headers like "SECTION 4: NIST AND THE ACT")
  4. Italicised remnants (*term* / _term_)
  5. Title-Case multi-word phrases (2-5 words, not sentence-start)

Outputs are deduped by lowercased term and ranked by kind weight,
then frequency, then first-seen page. 7 new unit tests cover the
header rejection, parens + em-dash detectors, italicised pickup,
cross-page dedup, and rank ordering.
EOF
git add src-tauri/src/ai/glossary.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 3: LLM definition extraction + validator + top-level `build_glossary`

**Objective:** Take top-N candidates from `scan_candidates`, build a
single LLM prompt, parse the liberal JSON response, validate + dedupe +
cap, return `GlossaryReport`. Modelled on `ai::citations::find_citations`.

**Files:**
- Modify: `src-tauri/src/ai/glossary.rs`

**Step 1: Add LLM glue + validator + top-level**

Append to `src-tauri/src/ai/glossary.rs` (before `#[cfg(test)] mod tests`):

```rust
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
- definition: 1-3 sentences, plain English, grounded in the snippet. Never invent facts not in the snippet.\n\
- If the term is a common-English word with no domain-specific meaning here (e.g. \"the\", \"figure\", \"section\"), OMIT it entirely.\n\
- If the snippet doesn't actually define the term, set confidence ≤ 0.4 and keep your definition very tentative — the caller will drop low-confidence entries.\n\
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
    let snippets: HashMap<String, &Candidate> = candidates
        .iter()
        .map(|c| (norm(&c.term), c))
        .collect();
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
pub(super) fn build_summary(candidates_total: u32, entries: &[GlossaryEntry], rejected: u32) -> GlossarySummary {
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
fn build_glossary_messages(candidates: &[Candidate], max_chars: usize, max_count: usize) -> Vec<ChatMessage> {
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
```

**Step 2: Add 4 validator + parser tests** at the end of `mod tests`:

```rust
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
```

**Step 3: Verify**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::glossary 2>&1 | tail -14
```
Expected: `13 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/glossary): LLM definition extraction + validator + top-level

Completes the `ai::glossary` backend:

  - Liberal JSON parser `parse_llm_glossary` (mirrors citations).
  - `validate_entries` drops: empty term, definition < 8 chars,
    out-of-range page, confidence < 0.4, case-folded duplicates.
    Sorts alphabetically. Caps at MAX_GLOSSARY_ENTRIES (500).
  - System prompt instructs the model to OMIT common-English words and
    flag uncertain definitions with low confidence so the validator
    drops them.
  - `build_glossary(provider, pages, opts)` runs the full pipeline:
    scan → pack → chat → parse → validate → summary.
  - `build_glossary_from_path` convenience wrapper over pdf::extract.

4 new unit tests: fenced + chatty JSON parsing, low-conf/short/bad-page
drops, case-fold dedupe + alpha sort.
EOF
git add src-tauri/src/ai/glossary.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 4: `ai::glossary_cache` — JSON sidecar (load/save/clear)

**Objective:** Persist `GlossaryReport` per pdf_hash as JSON in
`~/.slab/glossary/<hash>.json`. Schema-versioned via outer envelope.

**Files:**
- Create: `src-tauri/src/ai/glossary_cache.rs`
- Modify: `src-tauri/src/ai/mod.rs` (add `pub mod glossary_cache;`)

**Step 1: Register**

In `src-tauri/src/ai/mod.rs`:
```rust
pub mod glossary;
pub mod glossary_cache;
pub mod ollama;
```

**Step 2: Write `glossary_cache.rs`** — full file:

```rust
// JSON sidecar cache for Beacon Glossary reports.
//
// Why JSON not sqlite? Glossary entries are tiny (≤ 500 per doc).
// One read = one fs::read + serde_json. No migration story needed
// beyond a top-level `version` field. Mirrors how plugin manifests
// are persisted in `plugins::registry`.

use super::glossary::GlossaryReport;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 1;

/// Default cache dir: `~/.slab/glossary/`.
pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("glossary")
}

/// Cache envelope written to disk.
#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    version: u32,
    report: GlossaryReport,
}

fn entry_path(dir: &Path, pdf_hash: &str) -> PathBuf {
    dir.join(format!("{}.json", pdf_hash))
}

/// Load the cached report for a `pdf_hash`, or `Ok(None)` if absent /
/// version-mismatched. Stale-version files are kept on disk so a
/// downgrade still finds them — only NEW reports overwrite.
pub fn load(pdf_hash: &str, dir: &Path) -> io::Result<Option<GlossaryReport>> {
    let path = entry_path(dir, pdf_hash);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)?;
    let env: Envelope = match serde_json::from_str(&raw) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    if env.version != CACHE_VERSION {
        return Ok(None);
    }
    Ok(Some(env.report))
}

/// Save (overwrite) the cached report for a `pdf_hash`.
pub fn save(pdf_hash: &str, report: &GlossaryReport, dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let env = Envelope {
        version: CACHE_VERSION,
        report: report.clone(),
    };
    let path = entry_path(dir, pdf_hash);
    let json = serde_json::to_string_pretty(&env)
        .map_err(|e| io::Error::other(format!("serialize: {e}")))?;
    fs::write(&path, json)
}

/// Remove the cached report for a `pdf_hash`. No-op if absent.
pub fn clear(pdf_hash: &str, dir: &Path) -> io::Result<()> {
    let path = entry_path(dir, pdf_hash);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::glossary::{CandidateKind, GlossaryEntry, GlossarySummary};
    use tempfile::tempdir;

    fn sample_report() -> GlossaryReport {
        GlossaryReport {
            entries: vec![GlossaryEntry {
                term: "RAG".into(),
                definition: "Retrieval-augmented generation.".into(),
                page: 3,
                confidence: 0.92,
                kind: CandidateKind::Acronym,
                source_snippet: "Using RAG, …".into(),
            }],
            summary: GlossarySummary {
                candidates_total: 4,
                accepted: 1,
                rejected: 3,
                kept_acronyms: 1,
                ..Default::default()
            },
            model: "ollama/llama3.1".into(),
        }
    }

    #[test]
    fn load_missing_returns_none() {
        let d = tempdir().unwrap();
        let got = load("deadbeef", d.path()).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let d = tempdir().unwrap();
        let r = sample_report();
        save("hashA", &r, d.path()).unwrap();
        let got = load("hashA", d.path()).unwrap().expect("must be cached");
        assert_eq!(got.entries.len(), 1);
        assert_eq!(got.entries[0].term, "RAG");
    }

    #[test]
    fn clear_removes_the_file() {
        let d = tempdir().unwrap();
        let r = sample_report();
        save("hashB", &r, d.path()).unwrap();
        clear("hashB", d.path()).unwrap();
        assert!(load("hashB", d.path()).unwrap().is_none());
    }

    #[test]
    fn version_mismatch_returns_none() {
        let d = tempdir().unwrap();
        let path = d.path().join("mismatch.json");
        std::fs::write(&path, r#"{"version":999,"report":{"entries":[],"summary":{"candidates_total":0,"accepted":0,"rejected":0,"kept_acronyms":0,"kept_defined_first_use":0,"kept_italicised":0,"kept_capitalised_phrase":0},"model":""}}"#).unwrap();
        assert!(load("mismatch", d.path()).unwrap().is_none());
    }

    #[test]
    fn malformed_json_returns_none() {
        let d = tempdir().unwrap();
        let path = d.path().join("bad.json");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(load("bad", d.path()).unwrap().is_none());
    }
}
```

**Step 3: Verify**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::glossary_cache 2>&1 | tail -10
```
Expected: `5 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/glossary): JSON sidecar cache (load/save/clear)

Persistent per-pdf glossary cache at ~/.slab/glossary/<pdf_hash>.json
with a top-level {version, report} envelope (CACHE_VERSION=1).

Why JSON not sqlite: glossary is ≤ 500 entries — one fs::read +
serde_json round-trip is cheaper than spinning up a connection and
faster to inspect by hand. Mirrors the plugins::registry pattern.

Tests (5): load-missing returns None, save+load round-trip,
clear removes file, version mismatch returns None, malformed JSON
returns None.
EOF
git add src-tauri/src/ai/mod.rs src-tauri/src/ai/glossary_cache.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 5: Tauri commands wired into `lib.rs`

**Objective:** Expose `slab_beacon_build_glossary`,
`slab_beacon_load_glossary`, `slab_beacon_clear_glossary`. Reuse the
existing `do_load_beacon_config` + `make_provider` plumbing. Cache writes
on build, lookup on load.

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add the imports** near the other Beacon imports:

```rust
use ai::glossary::{
    build_glossary_from_path as do_beacon_build_glossary, GlossaryOpts, GlossaryReport,
};
use ai::glossary_cache::{
    cache_dir as glossary_cache_dir, clear as glossary_cache_clear, load as glossary_cache_load,
    save as glossary_cache_save,
};
```

**Step 2: Add the three commands** after `slab_beacon_study_stats`:

```rust
/// Beacon Glossary — mine the PDF for domain-specific terminology and
/// emit a definition-annotated, alphabetised glossary. Persists the
/// report to `~/.slab/glossary/<pdf_hash>.json` so subsequent opens load
/// instantly. v1.8.0 Beacon Bonus Slice 14.
#[tauri::command]
async fn slab_beacon_build_glossary(
    pdf_path: PathBuf,
    opts: Option<GlossaryOpts>,
) -> CmdResult<GlossaryReport> {
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
    let report = match do_beacon_build_glossary(provider, &pdf_path, &opts).await {
        Ok(r) => r,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    // Best-effort cache write.
    if let Ok(hash) = hash_pdf_path(&pdf_path) {
        let _ = glossary_cache_save(&hash, &report, &glossary_cache_dir());
    }
    CmdResult::Ok { value: report }
}

/// Beacon Glossary — load cached report for `pdf_path` (returns None if
/// no cache).
#[tauri::command]
async fn slab_beacon_load_glossary(pdf_path: PathBuf) -> CmdResult<Option<GlossaryReport>> {
    let hash = match hash_pdf_path(&pdf_path) {
        Ok(h) => h,
        Err(e) => {
            return CmdResult::Err {
                message: format!("hashing pdf: {e}"),
            }
        }
    };
    match glossary_cache_load(&hash, &glossary_cache_dir()) {
        Ok(v) => CmdResult::Ok { value: v },
        Err(e) => CmdResult::Err {
            message: format!("load glossary: {e}"),
        },
    }
}

/// Beacon Glossary — clear cache for `pdf_path`.
#[tauri::command]
async fn slab_beacon_clear_glossary(pdf_path: PathBuf) -> CmdResult<()> {
    let hash = match hash_pdf_path(&pdf_path) {
        Ok(h) => h,
        Err(e) => {
            return CmdResult::Err {
                message: format!("hashing pdf: {e}"),
            }
        }
    };
    match glossary_cache_clear(&hash, &glossary_cache_dir()) {
        Ok(_) => CmdResult::Ok { value: () },
        Err(e) => CmdResult::Err {
            message: format!("clear glossary: {e}"),
        },
    }
}
```

**Step 3: Register them** in the `invoke_handler!` list after
`slab_beacon_study_stats`:

```rust
slab_beacon_build_glossary,
slab_beacon_load_glossary,
slab_beacon_clear_glossary,
```

**Step 4: Verify compile**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo check --all-targets 2>&1 | tail -8
```
Expected: clean.

**Step 5: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/glossary): tauri commands (build/load/clear)

Three new commands wired into the Tauri invoke_handler:

  slab_beacon_build_glossary(pdf_path, opts?) -> GlossaryReport
    - Loads beacon config, instantiates provider, runs the full
      glossary pipeline, persists best-effort to cache.
  slab_beacon_load_glossary(pdf_path) -> Option<GlossaryReport>
    - Returns the cached report or None.
  slab_beacon_clear_glossary(pdf_path) -> ()
    - Wipes the cache file.

All three reuse the established hash_pdf_path / do_load_beacon_config /
make_provider plumbing.
EOF
git add src-tauri/src/lib.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 6: `BeaconGlossaryPanel.svelte` + sidebar wiring

**Objective:** New Svelte panel with letter-jump nav, search filter,
per-entry term/definition/kind/page citation, build/refresh button.

**Files:**
- Create: `src/lib/panels/BeaconGlossaryPanel.svelte` (full Svelte 5
  runes-style panel — see citations panel for the established pattern;
  list rendering, `core.invoke` for the Tauri commands, dispatches
  `slab:beacon-goto-page` on entry click)
- Modify: `src/routes/+page.svelte`:
  - Add `import BeaconGlossaryPanel from "$lib/panels/BeaconGlossaryPanel.svelte";`
    next to the other Beacon panel imports.
  - Add `{ id: "glossary", label: "Glossary", icon: "📖", ready: true },`
    in the `features` array between `study` and `merge`.
  - Add `"glossary",` to the `panelOrder` array between `"study"` and
    `"pages"`.
  - Add detached-panel branch `{:else if detachedPanel === "glossary"} <BeaconGlossaryPanel />`.
  - Add active-panel branch `{:else if active === "glossary"} <BeaconGlossaryPanel />`.

**Step 1: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/glossary): BeaconGlossaryPanel + sidebar nav entry

New Svelte panel mounted at sidebar id "glossary" (between Study and
Merge). Letter-jump bar (A B C D …) on the right, search filter at the
top, per-entry: term + kind chip + page citation (click →
slab:beacon-goto-page event), 1-3 sentence definition, source-snippet
in muted small text. Header has a "Build glossary" CTA that becomes
"Refresh" once cached; footer shows entry count + model id.

On mount: invoke('slab_beacon_load_glossary', { pdf_path }) — render
cached if present, else show the CTA.
On build: invoke('slab_beacon_build_glossary', ...) → render result.
On clear: invoke('slab_beacon_clear_glossary', ...) → reset to CTA.
EOF
git add src/lib/panels/BeaconGlossaryPanel.svelte src/routes/+page.svelte
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 7: Batched quality gates + push

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib

cd /Users/sanjay/Projects/slab
pnpm check

TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' '$TOK'; }; f" \
    push -u origin feature/v1.8.0-beacon-bonus-14-glossary
```

If any gate fails → fix in-tick before pushing.

---

## Task 8: Update STATE.md

Flip status to `STATUS: ✦ v1.8.0 "Glossary" 📖 — Slice 14 DONE on
feature/v1.8.0-beacon-bonus-14-glossary`, append a tick log entry, list
new files, set Mode A as next tick's playbook.
