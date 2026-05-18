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
        let duplicate = kept
            .iter()
            .any(|(t, p, _)| normalise_title(t) == norm && page.abs_diff(*p) <= 1);
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
            ChatMessage {
                role: ChatRole::System,
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: user,
            },
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

    let wire = parse_llm_outline(&resp.content).unwrap_or(LlmOutlineWire {
        entries: Vec::new(),
    });
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
        let raw =
            "Sure, here it is: {\"entries\":[{\"title\":\"Y\",\"page\":3,\"level\":2}]}\nHope that helps!";
        let w = parse_llm_outline(raw).expect("should ignore prose");
        assert_eq!(w.entries[0].level, 2);
    }

    #[test]
    fn returns_none_on_garbage() {
        assert!(parse_llm_outline("absolutely no json here").is_none());
        assert!(parse_llm_outline("").is_none());
    }

    fn entry(title: &str, page: u32, level: u8) -> LlmEntryWire {
        LlmEntryWire {
            title: title.into(),
            page: Some(page),
            level,
        }
    }

    #[test]
    fn build_tree_drops_invalid_page() {
        let entries = vec![
            entry("Good", 1, 1),
            entry("Bad", 999, 1), // out of range
            LlmEntryWire {
                title: "NoPage".into(),
                page: None,
                level: 1,
            },
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
            entry("Introduction", 2, 1),   // dup title within ±1 page
            entry("INTRODUCTION  ", 1, 1), // case + whitespace
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
        let pages = vec![
            "chapter one body".to_string(),
            "chapter two body".to_string(),
        ];
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
        let _ = propose_outline(provider.clone(), &pages, 5_000)
            .await
            .unwrap();
        let captured = provider.captured.lock().unwrap().clone();
        let sys = captured
            .iter()
            .find(|m| m.role == ChatRole::System)
            .unwrap();
        assert!(sys.content.to_lowercase().contains("json"));
        let usr = captured.iter().find(|m| m.role == ChatRole::User).unwrap();
        assert!(usr.content.to_lowercase().contains("page"));
    }
}
