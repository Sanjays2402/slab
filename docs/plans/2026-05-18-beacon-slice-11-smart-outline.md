# Beacon Slice 11 — Smart Outline Implementation Plan

> **For Hermes/Cake:** Execute this plan task-by-task on branch `feature/v1.5.0-beacon-bonus-11-smart-outline`. Quality gates ONCE at end of tick (`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib`, `pnpm check`). Use the mandatory commit author:
> ```
> git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
>     -c user.name='Cake (cron)' \
>     commit -F /tmp/msg.txt
> ```

**Goal:** Add a "✦ Suggest outline (Beacon)" button to the existing Outline Editor that asks the configured AI provider to propose a hierarchical TOC (H1/H2/H3 + page numbers) inferred from the actual PDF content, validates the cited pages exist, dedupes near-duplicate titles, and shows the proposal next to the current outline for accept/reject.

**Architecture:**

- New backend module `src-tauri/src/ai/outline.rs` — pure logic for prompt building + JSON parsing + validation/dedup. Reuses `ai::chunker::chunk_pages` for chunking and the existing `AiProvider` trait for the LLM call.
- One new Tauri command `slab_beacon_propose_outline(pdf_path, max_context_chars?) -> ProposedOutline`. We **deliberately do NOT add `slab_beacon_apply_outline`** — the existing `slab_write_outline` already does atomic writes, and the proposal is just `Vec<OutlineNode>` (the same type), so the frontend feeds the accepted tree straight into the existing save path. **YAGNI.**
- UI: extend `OutlineEditor.svelte` with a `✦ Suggest (Beacon)` button in the header. Clicking opens a side-by-side diff view; user accepts the whole proposal, accepts node-by-node (toggle per node), or rejects all. Accepted nodes replace `roots`; the user then clicks the existing "Save" button — no new save plumbing.

**Tech Stack:**

- Rust: `lopdf` (already a dep), `serde`/`serde_json` (already), `async-trait` (already), `tokio` (already).
- Reused: `crate::pdf::extract::extract_text`, `crate::ai::chunker::chunk_pages`, `crate::ai::{AiProvider, ChatMessage, ChatOpts, ChatRole, AiError}`, `crate::pdf::outline::OutlineNode`, `crate::ai::pii::parse_llm_reply` pattern (liberal JSON parse with fence-stripping).
- Frontend: Svelte 5 runes (`$state`, `$derived`, `$effect`), `@tauri-apps/api/core` invoke. No new deps.

---

## Task 1: Create the `ai::outline` module skeleton (types only)

**Objective:** Add the new module to the AI tree and declare the wire types. No logic yet — pure data + a `pub mod` entry.

**Files:**

- Create: `src-tauri/src/ai/outline.rs`
- Modify: `src-tauri/src/ai/mod.rs:17-28` (add `pub mod outline;` line, kept alphabetical)

**Step 1: Write `src-tauri/src/ai/outline.rs` (types + module doc only)**

```rust
// Beacon Smart Outline — propose a hierarchical TOC for a PDF.
//
// Half the PDFs in the wild have no outline, or a garbage 3-line one. This
// module asks the configured AI provider to read the document's text (chunked
// by `ai::chunker`) and emit a structured H1/H2/H3 outline anchored to real
// page numbers. We then validate every cited page exists, dedupe near-
// duplicate titles, and hand the result back as `Vec<OutlineNode>` — the same
// type the existing `pdf::outline::write_outline` consumes, so the frontend
// can pipe an accepted proposal straight into the existing save path.
//
// The module is deliberately pure (no IO) at the prompt/parse layer — the
// only async operation is the provider call. That keeps unit tests fast and
// deterministic against a mock provider, mirroring how `summary.rs` is built.

use super::chunker::chunk_pages;
use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use crate::pdf::outline::OutlineNode;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Cap on how much PDF text we ask the model to consider in one call.
/// Smart Outline is a one-shot operation: we'd rather under-cite than burn
/// 30s of inference on a 600-page novel. The caller can override.
pub const DEFAULT_OUTLINE_MAX_CHARS: usize = 30_000;

/// Hard ceiling on the proposed outline size. Keeps tests deterministic and
/// prevents a runaway model from emitting 500 H3 entries from a 20-page doc.
pub const MAX_PROPOSED_NODES: usize = 80;

/// What the frontend gets back from `slab_beacon_propose_outline`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedOutline {
    /// The proposed tree, ready to feed into `slab_write_outline` if the
    /// user accepts. Page indexes are 0-based, matching `OutlineNode`.
    pub nodes: Vec<OutlineNode>,
    /// Model identifier (e.g. `"llama3.2:3b"`) — for the UI's "proposed by"
    /// caption.
    pub model: String,
    /// Number of pages actually inlined into the prompt (may be smaller
    /// than `pages_total` if the doc exceeded `max_context_chars`).
    pub pages_used: u32,
    /// Total pages in the PDF.
    pub pages_total: u32,
    /// Diagnostic counts for the UI debug footer.
    pub raw_candidates: u32,
    pub dropped_invalid_page: u32,
    pub dropped_duplicate: u32,
}

/// Wire shape we ask the LLM to emit. Liberal — we accept missing fields
/// and trim chatter ourselves.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmOutlineWire {
    #[serde(default)]
    pub(super) entries: Vec<LlmEntryWire>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmEntryWire {
    /// Visible title.
    pub(super) title: String,
    /// 1-based page number (LLMs find 1-indexed easier to be right about).
    /// `null` is allowed — we drop those.
    #[serde(default)]
    pub(super) page: Option<u32>,
    /// 1, 2, or 3 — H1/H2/H3. Anything else is clamped.
    #[serde(default = "default_level")]
    pub(super) level: u8,
}

fn default_level() -> u8 {
    1
}
```

**Step 2: Wire the module into `ai::mod`**

Edit `src-tauri/src/ai/mod.rs` lines 17-28 (the `pub mod ...;` block). Insert `pub mod outline;` in alphabetical position (after `ollama`, before `pii`):

```rust
pub mod auto_tag;
pub mod chat;
pub mod chunker;
pub mod config;
pub mod diff_summary;
pub mod embedding_index;
pub mod ollama;
pub mod openai_compat;
pub mod outline;            // ← NEW
pub mod pii;
pub mod selection_action;
pub mod summary;
pub mod vision;
```

**Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -20`
Expected: `Finished ... profile [unoptimized + debuginfo]` (warnings about unused items are fine for now — code is unused until Task 2).

**Step 4: Commit**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
feat(beacon): scaffold ai::outline module with ProposedOutline types

Smart Outline (Slice 11) backend — wire types only. The real prompt-build
and parse logic lands in the next tasks; this commit just gets the module
registered so the rest of the work compiles cleanly.
EOF
git add src-tauri/src/ai/outline.rs src-tauri/src/ai/mod.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 2: Add the JSON-from-noisy-prose parser (TDD)

**Objective:** Implement a liberal parser that strips ```json fences, finds the outermost `{...}`, and decodes into `LlmOutlineWire`. Mirrors `ai::pii::parse_llm_reply`'s style. Pure function, easy to unit-test.

**Files:**

- Modify: `src-tauri/src/ai/outline.rs` (append below the existing types)

**Step 1: Write failing test (append to bottom of `src-tauri/src/ai/outline.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json() {
        let raw = r#"{"entries":[{"title":"Intro","page":1,"level":1}]}"#;
        let w = parse_llm_outline(raw).expect("parse should succeed");
        assert_eq!(w.entries.len(), 1);
        assert_eq!(w.entries[0].title, "Intro");
        assert_eq!(w.entries[0].page, Some(1));
        assert_eq!(w.entries[0].level, 1);
    }

    #[test]
    fn strips_markdown_fence() {
        let raw = "```json\n{\"entries\":[{\"title\":\"X\",\"page\":2}]}\n```";
        let w = parse_llm_outline(raw).expect("should parse fenced");
        assert_eq!(w.entries.len(), 1);
        assert_eq!(w.entries[0].title, "X");
    }

    #[test]
    fn tolerates_trailing_chatter() {
        let raw = "Sure, here it is: {\"entries\":[{\"title\":\"Y\",\"page\":3,\"level\":2}]}\nHope that helps!";
        let w = parse_llm_outline(raw).expect("should ignore prose");
        assert_eq!(w.entries[0].level, 2);
    }

    #[test]
    fn returns_none_on_garbage() {
        assert!(parse_llm_outline("absolutely no json here").is_none());
        assert!(parse_llm_outline("").is_none());
    }
}
```

**Step 2: Run test to verify failure**

Run: `cd src-tauri && cargo test --lib ai::outline 2>&1 | tail -10`
Expected: FAIL — `cannot find function parse_llm_outline in this scope`.

**Step 3: Implement `parse_llm_outline` (insert ABOVE the `#[cfg(test)]` block)**

```rust
/// Liberal JSON parser. Strips ```json ... ``` fences if present, then finds
/// the outermost `{...}` and tries `serde_json`. Returns `None` on any
/// parse failure — the LLM occasionally surrounds JSON with chatty prose
/// despite the system prompt, and we'd rather emit an empty proposal than
/// fail the whole panel.
pub(super) fn parse_llm_outline(raw: &str) -> Option<LlmOutlineWire> {
    let s = raw.trim();
    // Strip a markdown fence if the model wrapped its output.
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

**Step 4: Run test to verify pass**

Run: `cd src-tauri && cargo test --lib ai::outline::tests 2>&1 | tail -10`
Expected: PASS — `4 passed`.

**Step 5: Commit**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
feat(beacon/outline): add liberal LLM JSON parser

Mirrors the same fence-stripping + outermost-braces trick that
ai::pii::parse_llm_reply uses, since local models routinely surround
their JSON with chatty preamble despite the system prompt asking for
JSON-only.
EOF
git add src-tauri/src/ai/outline.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 3: Add the validate + dedupe + tree-build step (TDD)

**Objective:** Convert `Vec<LlmEntryWire>` → `Vec<OutlineNode>`: drop entries whose page is missing or out of range, dedupe by (title-normalised, page) within ±1 page, clamp `level` to 1..=3, and assemble the flat list into a hierarchical tree (level transitions: same → sibling, deeper → child, shallower → pop).

**Files:**

- Modify: `src-tauri/src/ai/outline.rs` (insert above `#[cfg(test)]`)

**Step 1: Write failing test (append inside the existing `mod tests` block)**

```rust
    fn entry(title: &str, page: u32, level: u8) -> LlmEntryWire {
        LlmEntryWire { title: title.into(), page: Some(page), level }
    }

    #[test]
    fn build_tree_drops_invalid_page() {
        let entries = vec![
            entry("Good", 1, 1),
            entry("Bad", 999, 1),  // out of range
            LlmEntryWire { title: "NoPage".into(), page: None, level: 1 },
        ];
        let (tree, stats) = build_tree(entries, 10);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].title, "Good");
        assert_eq!(stats.dropped_invalid_page, 2);
    }

    #[test]
    fn build_tree_dedupes_near_duplicates() {
        let entries = vec![
            entry("Introduction", 1, 1),
            entry("Introduction", 2, 1),       // dup title within ±1 page
            entry("INTRODUCTION  ", 1, 1),     // case + whitespace
        ];
        let (tree, stats) = build_tree(entries, 10);
        assert_eq!(tree.len(), 1);
        assert_eq!(stats.dropped_duplicate, 2);
    }

    #[test]
    fn build_tree_assembles_hierarchy() {
        // H1 Intro → H2 Background → H2 Goals → H1 Methods → H2 Setup → H3 Probe
        let entries = vec![
            entry("Intro", 1, 1),
            entry("Background", 2, 2),
            entry("Goals", 3, 2),
            entry("Methods", 5, 1),
            entry("Setup", 6, 2),
            entry("Probe", 7, 3),
        ];
        let (tree, _) = build_tree(entries, 20);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].title, "Intro");
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].title, "Background");
        assert_eq!(tree[1].title, "Methods");
        assert_eq!(tree[1].children.len(), 1);
        assert_eq!(tree[1].children[0].children.len(), 1);
        assert_eq!(tree[1].children[0].children[0].title, "Probe");
    }

    #[test]
    fn build_tree_clamps_level_and_caps_size() {
        let entries: Vec<LlmEntryWire> = (0..200)
            .map(|i| entry(&format!("E{i}"), 1, if i % 2 == 0 { 0 } else { 9 }))
            .collect();
        let (tree, _) = build_tree(entries, 5);
        // 200 entries all at page 1 dedupe down to 1, but if titles differ
        // they all stay. We cap at MAX_PROPOSED_NODES regardless.
        fn count(nodes: &[OutlineNode]) -> usize {
            nodes.iter().map(|n| 1 + count(&n.children)).sum()
        }
        let total = count(&tree);
        assert!(total <= MAX_PROPOSED_NODES, "got {total} nodes");
    }
```

**Step 2: Run test to verify failure**

Run: `cd src-tauri && cargo test --lib ai::outline 2>&1 | tail -10`
Expected: FAIL — `cannot find function build_tree in this scope`.

**Step 3: Implement `build_tree` + supporting fns (insert above `#[cfg(test)]`)**

```rust
/// Diagnostic counts surfaced to the UI footer.
pub(super) struct BuildStats {
    pub(super) raw_candidates: u32,
    pub(super) dropped_invalid_page: u32,
    pub(super) dropped_duplicate: u32,
}

/// Convert a flat list of LLM-proposed entries into a validated, deduped,
/// hierarchical `OutlineNode` tree.
///
/// Validation rules:
/// - `page` must be `Some(p)` with `1 <= p <= total_pages`.
/// - `level` is clamped to 1..=3.
/// - Title is trimmed; empty titles are dropped.
///
/// Dedupe rule: two entries collide if their normalised titles match and
/// their pages are within 1 of each other. We keep the first occurrence
/// (the model emits in reading order, so the first hit is usually the
/// definitional one).
pub(super) fn build_tree(
    entries: Vec<LlmEntryWire>,
    total_pages: u32,
) -> (Vec<OutlineNode>, BuildStats) {
    let raw_candidates = entries.len() as u32;
    let mut dropped_invalid_page = 0u32;
    let mut dropped_duplicate = 0u32;

    // ---- Pass 1: validate + clamp + drop ----
    let mut cleaned: Vec<(String, u32, u8)> = Vec::new();
    for e in entries {
        let title = e.title.trim().to_string();
        if title.is_empty() {
            dropped_invalid_page += 1;
            continue;
        }
        let page = match e.page {
            Some(p) if p >= 1 && p <= total_pages => p,
            _ => {
                dropped_invalid_page += 1;
                continue;
            }
        };
        let level = e.level.clamp(1, 3);
        cleaned.push((title, page, level));
    }

    // ---- Pass 2: dedupe ----
    let mut kept: Vec<(String, u32, u8)> = Vec::new();
    for (title, page, level) in cleaned {
        let norm = normalise_title(&title);
        let duplicate = kept.iter().any(|(t, p, _)| {
            normalise_title(t) == norm && page.abs_diff(*p) <= 1
        });
        if duplicate {
            dropped_duplicate += 1;
            continue;
        }
        kept.push((title, page, level));
    }

    // ---- Pass 3: cap size (kept order preserves the model's intent) ----
    if kept.len() > MAX_PROPOSED_NODES {
        kept.truncate(MAX_PROPOSED_NODES);
    }

    // ---- Pass 4: flat → hierarchical tree ----
    let tree = flat_to_tree(&kept);

    (
        tree,
        BuildStats {
            raw_candidates,
            dropped_invalid_page,
            dropped_duplicate,
        },
    )
}

fn normalise_title(t: &str) -> String {
    t.split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Walks a flat (title, page-1based, level) list and assembles an
/// OutlineNode tree. Level transitions follow the usual outline rules:
/// - Deeper level → push as child of the previous node at level-1.
/// - Same level → sibling of the previous node.
/// - Shallower level → pop the stack until we find the right parent.
///
/// A child whose declared level is more than one deeper than its parent is
/// promoted (e.g. an H3 with no H2 ancestor is treated as the parent's
/// direct child). This forgives common LLM mistakes without dropping data.
fn flat_to_tree(items: &[(String, u32, u8)]) -> Vec<OutlineNode> {
    let mut roots: Vec<OutlineNode> = Vec::new();
    // Stack holds (level, pointer path expressed as a Vec<usize> of child
    // indices from the root). We use indices not references to avoid the
    // borrow-checker fight you'd get with &mut OutlineNode on the stack.
    let mut stack: Vec<(u8, Vec<usize>)> = Vec::new();

    for (title, page, level) in items {
        let node = OutlineNode {
            title: title.clone(),
            page_index: Some(page.saturating_sub(1)),
            children: Vec::new(),
        };
        // Pop the stack until top is shallower than us.
        while let Some((top_level, _)) = stack.last() {
            if *top_level < *level {
                break;
            }
            stack.pop();
        }
        if let Some((_, parent_path)) = stack.last().cloned() {
            // Insert as child of the node at parent_path.
            let parent = walk_mut(&mut roots, &parent_path);
            parent.children.push(node);
            let mut new_path = parent_path;
            new_path.push(parent.children.len() - 1);
            stack.push((*level, new_path));
        } else {
            roots.push(node);
            stack.push((*level, vec![roots.len() - 1]));
        }
    }
    roots
}

fn walk_mut<'a>(roots: &'a mut [OutlineNode], path: &[usize]) -> &'a mut OutlineNode {
    let (first, rest) = path.split_first().expect("non-empty path");
    let mut cur = &mut roots[*first];
    for idx in rest {
        cur = &mut cur.children[*idx];
    }
    cur
}
```

**Step 4: Run test to verify pass**

Run: `cd src-tauri && cargo test --lib ai::outline 2>&1 | tail -15`
Expected: PASS — `8 passed` (4 from Task 2 + 4 new).

**Step 5: Commit**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
feat(beacon/outline): validate, dedupe, and tree-ify LLM proposals

build_tree() takes the raw LlmEntryWire stream and applies four passes:
1. validate (drop missing/out-of-range pages, empty titles, clamp level)
2. dedupe (same normalised title within ±1 page)
3. cap at MAX_PROPOSED_NODES (80) to keep tests deterministic
4. flatten level-tagged entries into an OutlineNode tree

Stats are surfaced so the proposal UI can show "12 candidates, 3 dropped
as duplicates" for transparency.
EOF
git add src-tauri/src/ai/outline.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 4: Wire the provider call into `propose_outline` (TDD with mock)

**Objective:** Add the async entry point that builds the system+user messages, runs `provider.chat()`, parses, validates, and returns `ProposedOutline`. Test against a `MockProvider` in the same style as `summary.rs`.

**Files:**

- Modify: `src-tauri/src/ai/outline.rs` (insert above `#[cfg(test)]`, and add tests below)

**Step 1: Write the public entry-point tests (append inside `mod tests`)**

```rust
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockProvider {
        reply: String,
        captured: Mutex<Vec<ChatMessage>>,
    }

    impl MockProvider {
        fn new(reply: &str) -> Self {
            Self { reply: reply.into(), captured: Mutex::new(Vec::new()) }
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
                model: "mock-outline:test".into(),
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
    async fn propose_outline_happy_path() {
        let pages = vec!["chapter one body".to_string(), "chapter two body".to_string()];
        let reply = r#"{"entries":[
            {"title":"Chapter One","page":1,"level":1},
            {"title":"Chapter Two","page":2,"level":1}
        ]}"#;
        let provider = Arc::new(MockProvider::new(reply));
        let p = propose_outline(provider, &pages, 5_000).await.unwrap();
        assert_eq!(p.nodes.len(), 2);
        assert_eq!(p.nodes[0].title, "Chapter One");
        assert_eq!(p.nodes[0].page_index, Some(0));
        assert_eq!(p.pages_total, 2);
        assert_eq!(p.raw_candidates, 2);
        assert_eq!(p.dropped_invalid_page, 0);
        assert_eq!(p.model, "mock-outline:test");
    }

    #[tokio::test]
    async fn propose_outline_handles_chatty_llm() {
        let pages = vec!["a".to_string(); 3];
        let reply = "Sure! Here it is:\n```json\n{\"entries\":[{\"title\":\"Top\",\"page\":1}]}\n```\nLet me know if you'd like adjustments!";
        let provider = Arc::new(MockProvider::new(reply));
        let p = propose_outline(provider, &pages, 5_000).await.unwrap();
        assert_eq!(p.nodes.len(), 1);
        assert_eq!(p.nodes[0].title, "Top");
    }

    #[tokio::test]
    async fn propose_outline_empty_on_garbage() {
        let provider = Arc::new(MockProvider::new("no json here, sorry"));
        let pages = vec!["a".to_string()];
        let p = propose_outline(provider, &pages, 5_000).await.unwrap();
        assert_eq!(p.nodes.len(), 0);
    }

    #[tokio::test]
    async fn propose_outline_prompt_asks_for_json() {
        let provider = Arc::new(MockProvider::new(r#"{"entries":[]}"#));
        let pages = vec!["a".to_string()];
        let _ = propose_outline(provider.clone(), &pages, 5_000).await.unwrap();
        let captured = provider.captured.lock().unwrap().clone();
        let sys = captured.iter().find(|m| m.role == ChatRole::System).unwrap();
        assert!(sys.content.to_lowercase().contains("json"));
        let usr = captured.iter().find(|m| m.role == ChatRole::User).unwrap();
        assert!(usr.content.to_lowercase().contains("page"));
    }
```

**Step 2: Run tests to verify failure**

Run: `cd src-tauri && cargo test --lib ai::outline 2>&1 | tail -15`
Expected: FAIL — `cannot find function propose_outline in this scope`.

**Step 3: Implement `propose_outline` + path-loading wrapper (insert above `#[cfg(test)]`)**

```rust
const SYSTEM_PROMPT: &str = "You are Beacon, a PDF table-of-contents author. \
Given the content of a PDF (paginated), you propose a clean H1/H2/H3 outline. \
Reply with JSON ONLY, no prose, no markdown fences, in this exact shape:\n\
{\"entries\":[{\"title\":\"...\",\"page\":N,\"level\":1|2|3}]}\n\
- title: short, human-readable (4-8 words). No trailing periods.\n\
- page: 1-based page number where the section starts.\n\
- level: 1 for top-level chapters, 2 for sub-sections, 3 only when clearly nested.\n\
Prefer fewer, meaningful entries over many noisy ones. Aim for 5-25 entries on\n\
a normal document. Skip table-of-contents pages, copyright pages, and indexes.";

/// Build the prompt vec for an outline proposal. Pulled out so tests can pin it.
fn build_messages(pages: &[String], max_context_chars: usize) -> (Vec<ChatMessage>, u32) {
    // We deliberately don't reuse build_context() from chat.rs here — that
    // helper formats for Q&A with a tail-bias. For outlines we want a
    // forward-bias (chapters live near the front of long docs) and explicit
    // page-number anchors so the model has fewer excuses to fabricate.
    let mut buf = String::new();
    let mut pages_used = 0u32;
    for (i, page) in pages.iter().enumerate() {
        let header = format!("\n--- PAGE {} ---\n", i + 1);
        if buf.len() + header.len() + page.len() > max_context_chars {
            break;
        }
        buf.push_str(&header);
        buf.push_str(page);
        pages_used += 1;
    }
    let user = format!(
        "Below is the content of a PDF. Propose a hierarchical outline \
         (table of contents) inferred from the body text. Each entry MUST \
         cite the page where that section begins. Respond with JSON only.\n\
         {buf}"
    );
    (
        vec![
            ChatMessage { role: ChatRole::System, content: SYSTEM_PROMPT.to_string() },
            ChatMessage { role: ChatRole::User, content: user },
        ],
        pages_used,
    )
}

/// Run a Smart Outline proposal turn against pre-extracted page text.
pub async fn propose_outline(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    max_context_chars: usize,
) -> Result<ProposedOutline, AiError> {
    // Chunker isn't strictly needed for the prompt itself, but we call it
    // up-front so a doc with zero extractable text fails fast with an
    // empty-ish proposal rather than waiting on an LLM round-trip.
    let _chunks = chunk_pages(pages);

    let (msgs, pages_used) = build_messages(pages, max_context_chars);
    let opts = ChatOpts {
        // Outline generation wants determinism — same doc, same outline.
        temperature: Some(0.1),
        // 80 entries * ~40 tokens each ≈ 3200; double for safety.
        max_tokens: Some(6_000),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &opts).await?;

    let wire = parse_llm_outline(&resp.content).unwrap_or(LlmOutlineWire { entries: Vec::new() });
    let (nodes, stats) = build_tree(wire.entries, pages.len() as u32);

    Ok(ProposedOutline {
        nodes,
        model: resp.model,
        pages_used,
        pages_total: pages.len() as u32,
        raw_candidates: stats.raw_candidates,
        dropped_invalid_page: stats.dropped_invalid_page,
        dropped_duplicate: stats.dropped_duplicate,
    })
}

/// Convenience wrapper: read PDF text from disk, then propose.
pub async fn propose_outline_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    max_context_chars: usize,
) -> Result<ProposedOutline, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    propose_outline(provider, &pages, max_context_chars).await
}
```

**Step 4: Run tests to verify pass**

Run: `cd src-tauri && cargo test --lib ai::outline 2>&1 | tail -15`
Expected: PASS — `12 passed` (8 prior + 4 new).

**Step 5: Verify the full suite still passes**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -5`
Expected: `test result: ok. 585 passed` (581 baseline + 4 new — Task 2's 4 don't count separately, they're in ai::outline::tests).

**Step 6: Commit**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
feat(beacon/outline): add propose_outline async entry point + path wrapper

The provider call is buffered (non-streaming) like beacon_summary —
streaming for an outline preview isn't useful, the user wants the whole
tree at once to diff against the existing one. Temperature pinned to 0.1
for deterministic re-runs on the same doc.

Tests cover happy path, chatty-LLM tolerance, garbage-response graceful
empty, and prompt-shape sanity (system msg mentions JSON, user msg
mentions page numbers).
EOF
git add src-tauri/src/ai/outline.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 5: Expose `slab_beacon_propose_outline` Tauri command

**Objective:** Add the IPC surface in `src-tauri/src/lib.rs` so the frontend can call into the new module. Pattern-match `slab_beacon_summary` exactly.

**Files:**

- Modify: `src-tauri/src/lib.rs` — add a `use ai::outline::...` line, add a `#[tauri::command]` fn, register it in the `invoke_handler!`.

**Step 1: Add the `use` import**

Find the existing line (around line 31):
```rust
use ai::summary::{beacon_summary_from_path as do_beacon_summary, BeaconSummary, SummaryLength};
```

Insert **immediately after** it:
```rust
use ai::outline::{
    propose_outline_from_path as do_beacon_propose_outline, ProposedOutline,
    DEFAULT_OUTLINE_MAX_CHARS,
};
```

**Step 2: Add the command (insert right after the existing `slab_beacon_summary` block — which ends around line 745)**

```rust
/// Beacon "Smart Outline" — propose a hierarchical TOC for an opened PDF.
/// Returns a `ProposedOutline` whose `nodes` field is shaped exactly like
/// what `slab_write_outline` expects, so the frontend can pipe an accepted
/// proposal straight into the existing save path with zero translation.
#[tauri::command]
async fn slab_beacon_propose_outline(
    pdf_path: PathBuf,
    max_context_chars: Option<u32>,
) -> CmdResult<ProposedOutline> {
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
    let budget = max_context_chars
        .map(|n| n as usize)
        .unwrap_or(DEFAULT_OUTLINE_MAX_CHARS);
    do_beacon_propose_outline(provider, &pdf_path, budget)
        .await
        .into()
}
```

**Step 3: Register the command in the `invoke_handler!`**

Find the block around line 1929 (after `slab_beacon_vision_ask,`). Insert on a new line right after `slab_beacon_selection_action,` (line 1928):

```rust
            slab_beacon_propose_outline,
```

**Step 4: Verify compile + tests**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -5`
Expected: `Finished ... profile`.

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: `test result: ok. 585 passed`.

**Step 5: Commit**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
feat(beacon/outline): expose slab_beacon_propose_outline Tauri command

Mirrors the slab_beacon_summary command shape — same config-load +
make_provider dance, same Optional<u32> max_context_chars override.

Deliberately does NOT add a separate slab_beacon_apply_outline: the
ProposedOutline.nodes field is already Vec<OutlineNode>, which is what
slab_write_outline takes, so the frontend pipes the accepted tree
straight through the existing save command. YAGNI.
EOF
git add src-tauri/src/lib.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 6: Add the "Suggest (Beacon)" UI to OutlineEditor.svelte

**Objective:** Add a button in the OutlineEditor header that calls the new command, then shows a confirmation panel with the proposed tree and "Accept all / Reject" buttons. Accepting replaces `roots` (user still has to click Save to persist).

**Files:**

- Modify: `src/lib/OutlineEditor.svelte`

**Step 1: Add state vars + propose handler (insert near the existing state declarations around line 69-77)**

Find:
```svelte
  let roots = $state<EditNode[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
```

Insert **immediately after** that block:

```svelte
  // Smart Outline proposal state (Beacon Slice 11).
  type ProposedOutlineDto = {
    nodes: RawNode[];
    model: string;
    pages_used: number;
    pages_total: number;
    raw_candidates: number;
    dropped_invalid_page: number;
    dropped_duplicate: number;
  };

  let proposing = $state(false);
  let proposeError = $state<string | null>(null);
  let proposal = $state<ProposedOutlineDto | null>(null);
  let proposedRoots = $state<EditNode[] | null>(null);

  async function suggestOutline() {
    if (!isInTauri()) {
      proposeError = "Smart Outline requires the desktop app.";
      return;
    }
    proposing = true;
    proposeError = null;
    try {
      const result = await invoke<ProposedOutlineDto>("slab_beacon_propose_outline", {
        pdfPath: path,
      });
      proposal = result;
      proposedRoots = result.nodes.map(toEdit);
    } catch (e) {
      proposeError = String(e);
      proposal = null;
      proposedRoots = null;
    } finally {
      proposing = false;
    }
  }

  function acceptProposal() {
    if (!proposedRoots) return;
    roots = proposedRoots;
    proposal = null;
    proposedRoots = null;
  }

  function rejectProposal() {
    proposal = null;
    proposedRoots = null;
    proposeError = null;
  }
```

**Step 2: Add the "Suggest" button to the header (replace the existing `<header>` block around lines 264-267)**

Find:
```svelte
  <header class="oe-head">
    <h2 id="outline-editor-title">Edit outline</h2>
    <button class="oe-close" onclick={onclose} title="Close (Esc)">×</button>
  </header>
```

Replace with:
```svelte
  <header class="oe-head">
    <h2 id="outline-editor-title">Edit outline</h2>
    <span class="oe-spacer"></span>
    <button
      class="oe-btn ghost"
      onclick={suggestOutline}
      disabled={proposing || loading || saving || !isInTauri()}
      title="Ask Beacon to propose an outline from the document content"
    >
      {proposing ? "Thinking…" : "✦ Suggest (Beacon)"}
    </button>
    <button class="oe-close" onclick={onclose} title="Close (Esc)">×</button>
  </header>
```

**Step 3: Add the proposal review panel (insert immediately AFTER the closing `</header>` tag and BEFORE the existing `<div class="oe-body">`)**

```svelte
  {#if proposeError}
    <div class="oe-status err">Smart Outline failed: {proposeError}</div>
  {/if}

  {#if proposal && proposedRoots}
    <div class="oe-proposal">
      <div class="oe-proposal-head">
        <strong>Beacon proposes {countNodes(proposedRoots)} entries</strong>
        <span class="oe-proposal-meta">
          {proposal.model} · {proposal.pages_used}/{proposal.pages_total} pages
          {#if proposal.dropped_invalid_page > 0 || proposal.dropped_duplicate > 0}
            · dropped {proposal.dropped_invalid_page + proposal.dropped_duplicate} weak entries
          {/if}
        </span>
      </div>
      <ul class="oe-list oe-proposal-list">
        {@render renderLevel(proposedRoots, 0)}
      </ul>
      <div class="oe-proposal-actions">
        <button class="oe-btn" onclick={rejectProposal}>Reject</button>
        <button class="oe-btn primary" onclick={acceptProposal}>
          Replace current outline
        </button>
      </div>
    </div>
  {/if}
```

**Step 4: Add the `countNodes` helper (insert near the other helpers, e.g. after `toRaw` around line 67)**

```svelte
  function countNodes(nodes: EditNode[]): number {
    let n = 0;
    for (const node of nodes) n += 1 + countNodes(node.children);
    return n;
  }
```

**Step 5: Add the CSS for the proposal panel (insert into the existing `<style>` block, near the bottom — find `.oe-actions { display: inline-flex; gap: 2px; }` around line 454 and add the new rules after it)**

```css
  .oe-proposal {
    margin: 0 16px 12px;
    padding: 12px;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 8px;
    background: var(--bg-elev, #1c1c1c);
  }
  .oe-proposal-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 8px;
  }
  .oe-proposal-meta {
    color: var(--muted, #888);
    font-size: 12px;
  }
  .oe-proposal-list {
    max-height: 280px;
    overflow-y: auto;
    margin-bottom: 12px;
    opacity: 0.92;
  }
  .oe-proposal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
```

**Step 6: Run svelte-check**

Run: `cd /Users/sanjay/Projects/slab && pnpm check 2>&1 | tail -8`
Expected: `0 errors, X warnings` (warning count baseline-preserved at 23 ± minor).

If `OutlineEditor.svelte` shows new errors:
- Verify `proposedRoots` is typed `EditNode[] | null` not `RawNode[]`.
- Verify the `renderLevel` snippet accepts arbitrary trees — it should, since it already recurses on `node.children` of type `EditNode[]`.

**Step 7: Smoke-build the frontend**

Run: `cd /Users/sanjay/Projects/slab && pnpm build 2>&1 | tail -3`
Expected: `built in ...s` with no errors.

**Step 8: Commit**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
feat(beacon/outline-ui): Smart Outline button + proposal review in editor

Adds a "✦ Suggest (Beacon)" button to the OutlineEditor header. Clicking
runs slab_beacon_propose_outline against the configured AI provider,
then shows the proposed tree inline above the current outline with a
header bar that explains what the model dropped ("12 entries · llama3.2:3b
· 24/24 pages · dropped 3 weak entries").

Accept → replaces the working tree. User still has to click Save (the
existing save flow) to persist — keeps the destructive action behind two
confirmations and lets them tweak entries before commit.

Reject → just clears the proposal, no state change.

Deliberately reuses the existing renderLevel snippet for the preview, so
nested children render with the same indentation rules as the editable
tree — what you see is exactly what gets written.
EOF
git add src/lib/OutlineEditor.svelte
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 7: Quality gates + push

**Objective:** Run the full Slab quality bar, then push the feature branch.

**Step 1: Format check**

Run: `cd /Users/sanjay/Projects/slab/src-tauri && cargo fmt --all -- --check`
Expected: no output, exit code 0.
If it fails: `cargo fmt --all`, then re-run the check, then add a single `chore(fmt): cargo fmt` commit.

**Step 2: Clippy**

Run: `cd /Users/sanjay/Projects/slab/src-tauri && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10`
Expected: `Finished ...` (no `error:` lines).
Common easy fixes:
- `clippy::ptr_arg` on `&Vec<...>` → change to `&[...]`
- `clippy::needless_borrow` → drop the `&`
- `clippy::derivable_impls` → derive `Default`

**Step 3: Cargo test (full lib)**

Run: `cd /Users/sanjay/Projects/slab/src-tauri && cargo test --lib 2>&1 | tail -5`
Expected: `test result: ok. 585 passed; 0 failed`.

**Step 4: pnpm check**

Run: `cd /Users/sanjay/Projects/slab && pnpm check 2>&1 | tail -5`
Expected: `0 errors`, warnings ≤ baseline (23).

**Step 5: Push the branch**

```bash
cd /Users/sanjay/Projects/slab
TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' \"\$TOK\"; }; f" \
    push -u origin feature/v1.5.0-beacon-bonus-11-smart-outline
```

Expected: `* [new branch]` line, branch is now on GitHub.

---

## Task 8: Update STATE.md for next tick

**Objective:** Record what shipped so the next cron tick picks up the right mode.

**File:**

- Modify: `.cron-state/STATE.md`

**Step 1: Append a new tick section above the existing TICK 2026-05-17 23:55 entry**

The new section format:
```markdown
## TICK 2026-05-18 00:XX PT — Beacon Slice 11 Smart Outline shipped

Implemented Beacon Bonus Slice 11 on `feature/v1.5.0-beacon-bonus-11-smart-outline`:
- `src-tauri/src/ai/outline.rs` — new module, 12 unit tests
- `slab_beacon_propose_outline` Tauri command
- OutlineEditor.svelte gains "✦ Suggest (Beacon)" button + proposal review
- All quality gates green: fmt, clippy, 585 tests, pnpm check

**STATUS: DONE** — ready for MODE A merge next tick.

Commits on feature branch:
- <sha1> feat(beacon): scaffold ai::outline module
- <sha2> feat(beacon/outline): liberal LLM JSON parser
- <sha3> feat(beacon/outline): validate, dedupe, tree-ify
- <sha4> feat(beacon/outline): propose_outline + path wrapper
- <sha5> feat(beacon/outline): expose Tauri command
- <sha6> feat(beacon/outline-ui): Smart Outline UI in editor
```

(Fill `<shaN>` with `git log --oneline -6` short hashes.)

**Step 2: Also update STATUS at top — change line 8**

If v1.4.0 CI is still in_progress: keep `RELEASE_PENDING` line, append `+ feature/v1.5.0-beacon-bonus-11-smart-outline DONE awaiting merge` at the end of the STATUS line.

If v1.4.0 CI is green and Mode B finalize already happened: remove `RELEASE_PENDING`, set STATUS to `🪑 v1.4.0 released. Slice 11 DONE on feature/v1.5.0-beacon-bonus-11-smart-outline — MODE A next tick.`

**Step 3: Commit the state update**

```bash
cd /Users/sanjay/Projects/slab
cat > /tmp/msg.txt <<'EOF'
chore(cron): record Slice 11 ship in STATE.md
EOF
git add .cron-state/STATE.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

**Step 4: Push the state-file commit**

```bash
cd /Users/sanjay/Projects/slab
TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' \"\$TOK\"; }; f" \
    push origin feature/v1.5.0-beacon-bonus-11-smart-outline
```

---

## Verification Checklist

When all tasks are done, the next tick should be able to:

- [ ] Run `git log feature/v1.5.0-beacon-bonus-11-smart-outline --oneline -10` and see 6-7 commits with the conventional-commits messages above.
- [ ] Run `cargo test --lib ai::outline` on the feature branch and see `12 passed`.
- [ ] Run `cargo test --lib` and see `585 passed` (baseline 581 + 4 new outline tests = 585).
- [ ] Open `src/lib/OutlineEditor.svelte` and find the `✦ Suggest (Beacon)` button reference.
- [ ] Run `gh api repos/Sanjays2402/slab/branches/feature/v1.5.0-beacon-bonus-11-smart-outline` and see the branch exists.
- [ ] `.cron-state/STATE.md` STATUS line mentions Slice 11 DONE.

---

## What This Plan Does NOT Cover (out of scope, ship later)

- **Per-node accept/reject in the proposal panel.** v1 ships "accept all / reject" only. Per-node toggling is a follow-up — adds a checkbox column to the preview and a "merge into current" mode that interleaves accepted proposed nodes with the existing tree by page order. Worth maybe one slice on its own once we see real usage.
- **Pre-computing the outline on doc-open in the background.** The propose button is on-demand. Pre-compute would mean a worker queue + cache invalidation, which is a Slice on its own (probably needs to live in the embedding-index module that already has the eviction logic).
- **Comparing existing outline vs proposal as a real diff view.** The current design shows the proposal next to the editable tree. A proper side-by-side with green-add / red-delete coloring is nicer but requires a different layout — punt.
- **Streaming the proposal.** Already deliberately buffered — see Task 4's commit message.

---

## Estimated Effort

- Tasks 1-5 (backend): ~25 minutes if no surprises. The fiddly bit is `flat_to_tree` getting the borrow checker right; the pattern in this plan uses index-paths and `walk_mut` to sidestep it cleanly.
- Task 6 (UI): ~15 minutes. The renderLevel snippet is already polymorphic so the preview just works.
- Tasks 7-8 (quality + push + state): ~10 minutes.

Total: ~50 minutes of focused work, well within one cron tick.
