# Auto-Tag (Library)

Auto-tag asks your configured Beacon AI provider to suggest **3–5 topical
tags** for any document in your Slab library, then materialises them as
real `library_tags` rows and attaches them.

> **Additive by design.** Auto-tag never removes a tag you set by hand.
> The new tag set is always `existing ∪ suggested`.

## What you need

- A configured Beacon provider in `~/.slab/config.toml`. The default is
  Ollama running locally — pull `llama3.2:3b` (or whatever chat model
  you've set as `[beacon.chat_model]`) before first use.
- For very large PDFs: tags are derived from the first ~6 000 characters
  of extractable text. Scanned PDFs need to be OCR'd first (see
  `docs/lens-ocr.md`) — auto-tag won't run on image-only pages.

## How to use it

### From the toolbar (bulk)

In Library mode, the **🏷️ Auto-tag N** toolbar button runs auto-tag
across every visible document. It asks once before doing anything, then
processes docs sequentially. Per-doc failures (provider down, no
extractable text, …) don't abort the run — you'll see a summary at the
end like:

> ✓ Auto-tag: 18 tagged, 2 failed (of 20)

If you've filtered the library by folder/tag/search before clicking,
only the filtered set is processed.

### Per-card

Each document card has a **🏷️ Auto-tag** button. It runs auto-tag on
that one doc only. While running, the button shows "Tagging…" and is
disabled.

### Context menu

Right-click (or ⋯ menu) any document for the **🏷️ Auto-tag** entry.

## Limits / tuning

- Max tags per run: 5 (hard-clamped to 1–10 by the backend).
- Context window: first 6 000 chars (≈ 1–2 pages of dense text).
- Temperature: 0.1 — replies are deterministic and short.

These knobs are exposed in the TS API as `AutoTagOpts { max_tags,
max_context_chars }` but the UI currently uses backend defaults. A
future settings panel will surface them.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| "All N auto-tag attempts failed" | Beacon provider unreachable | Check Ollama is running (`curl localhost:11434`). For OpenAI-compat: verify `api_key_env` is set in the shell that launched Slab. |
| Single doc shows "auto-tag failed: no extractable text" | Scanned PDF | Run OCR on that doc first. |
| Tags look weird ("untitled", "doc 1") | Doc has thin metadata + no body text | OCR + retry, or hand-edit. |

## Backend reference

- AI module: `src-tauri/src/ai/auto_tag.rs`
- Orchestrator: `src-tauri/src/pdf/library/auto_tagger.rs`
- Tauri commands: `slab_library_auto_tag_one`, `slab_library_auto_tag_many`
- TS bindings: `src/lib/library.ts` — `autoTagRunOne`, `autoTagRunMany`
- Shipped in v0.13.0 "Lens" Slice 6.
