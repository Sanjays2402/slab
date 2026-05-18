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
