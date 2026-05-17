# Slab — Roadmap to v1.0 "Glass"

After v0.10.0 Beacon lands, here are the 6 versions that take Slab from "great PDF
reader + AI sidekick" to "the only PDF tool a Mac user ever needs" — and then to a
stable 1.0.

Pitch evolution:
> *"Slab — Stirling-PDF features locally with Liquid Glass UI, you can chat with your
> PDFs without sending them anywhere, edit them like Word docs, and use them like a
> personal research library."*

Each version is sized to ~1 week of autonomous cron work at current pace.

---

## v0.11.0 "Lathe" — Edit Mode

**The killer-missing feature:** in-place PDF text editing.

- **Text editing** — click any text span, edit it like a normal text input, font/size/
  position preserved. Backed by `lopdf` content-stream rewriting (the skill already
  has the muscle memory for this).
- **Page reorder/insert/delete** — drag thumbnails to reorder. Right-click for
  insert blank / delete / duplicate / rotate.
- **Page split-by-pattern** — split a long PDF into chapters via regex on page text,
  or by detected H1 headings (reusing Smart Outline from Beacon).
- **Multi-PDF tabs** — open N PDFs in tabs in one window. Switch with cmd-{1..9}.
- **Image insert + crop + replace** — overlay an image, drag/resize, replace existing
  images.

Slices: 8. New deps: none (lopdf already wired).

---

## v0.12.0 "Atlas" — Library Mode

**Beacon × 100:** chat with your whole library, not one doc at a time.

- **Library view** — Finder-style grid of all PDFs in indexed folders. Tags,
  collections, smart-folders by metadata or content match.
- **Cross-doc Beacon** — `slab_beacon_chat_library` queries the sqlite-vec index
  across all indexed PDFs. Citations include doc+page.
- **Saved searches** — pin queries, get notified when new docs match.
- **Indexed-folder watcher** — point Slab at `~/Documents/Papers/`, it auto-indexes
  new arrivals.
- **Smart bookmarks** — Beacon proposes meaningful bookmarks for each PDF on first
  open.

Slices: 7. New deps: `notify = "6.1"` (fs watcher).

---

## v0.13.0 "Lens" — OCR + Vision

**Image-only PDFs become first-class.**

- **Local OCR** — surya-ocr (Python sidecar) or tesseract for scanned PDFs. Auto-OCR
  on open if doc has no extractable text. Searchable text layer overlay.
- **Table extraction → CSV** — detect tables, extract with structure intact, copy
  as CSV / markdown / TSV.
- **Equation extraction → LaTeX** — detect math regions, OCR to LaTeX (pix2tex
  sidecar). Copy as `$...$` or `\\begin{equation}`.
- **Vision Q&A in Beacon** — multi-modal provider (`llava` via Ollama). Ask "what's
  in this chart?" — Beacon passes the rendered page region to vision model.
- **Auto-tag** — Beacon proposes tags for each doc on import (uses Library Mode tags).

Slices: 9. New deps: `tokio-process` for sidecar management, vendored `tesseract` binary.

---

## v0.14.0 "Stack" — Diff & Compare

**Contract review / paper revision / spec evolution — without Word.**

- **Visual diff** — open 2 PDFs side-by-side with synchronized scrolling. Highlight
  added / removed / modified regions.
- **Text diff** — line-level diff in a third pane.
- **Track changes** — annotate diffs with comments, export as PDF report.
- **Patch / merge** — apply diff from PDF A onto PDF B (think `git apply` for PDFs).
- **Beacon diff summary** — "what changed?" — AI explanation of the diff in plain English.

Slices: 6. New deps: `similar = "2.6"` (diff engine).

---

## v0.15.0 "Theater" — Presenter Mode

**Slab as a Keynote-killer for PDF slide decks.**

- **Slides view** — auto-detect slide-style PDFs. Manual override toggle.
- **Presenter window** — current slide + next slide + notes + timer on internal display;
  audience view on external.
- **Live drawing** — annotate during presentation (pen, highlighter, laser pointer,
  spotlight, hide cursor).
- **Auto-advance** — per-slide timing or click-to-advance.
- **Persistent annotations** — drawings made during present are saved back to the PDF
  if you want them.
- **Stream Deck profile** — exported `.streamDeckProfile` for hardware control.

Slices: 5. New deps: none (uses tauri-multi-window).

---

## v1.0.0 "Glass" — Stable Release

**Polish, not features. The pillars are set; sharpen the edges.**

- **Floating panels** — detach Beacon, Outline, Library, etc. into separate windows
  (drag tab out, like browsers).
- **Multi-window** — open multiple Slab windows, drag PDFs between them.
- **Command palette (⌘K)** — fuzzy-search every command in the app. Heavy inspiration
  from Linear and Raycast.
- **Vim/Helix bindings** — opt-in keyboard-driven mode. `j/k` scroll, `gg/G` first/last
  page, `/` search, `:s` selection action, etc.
- **Customizable shortcuts** — every action remappable in Settings.
- **Onboarding tour** — 90-second tour for first-time users.
- **Stable API** — `slab_*` Tauri commands frozen as v1 surface for any future
  plugin/extension system.
- **Performance pass** — open <500ms for 100-page PDFs, render <16ms per page on
  M-series.
- **Accessibility pass** — VoiceOver, dynamic text size, high-contrast mode.
- **Localization** — en, es, fr, de, ja, zh-Hans, hi (Beacon already speaks all of
  these, so the chat-translate is free).

Slices: 10. No new deps. Frozen public API.

---

## Total Plan

| Ver | Codename | Slices | Theme |
|-----|----------|--------|-------|
| v0.10.0 | Beacon | 15 (10+5 bonus) | Local AI |
| v0.11.0 | Lathe | 8 | Edit Mode |
| v0.12.0 | Atlas | 7 | Library Mode |
| v0.13.0 | Lens | 9 | OCR + Vision |
| v0.14.0 | Stack | 6 | Diff & Compare |
| v0.15.0 | Theater | 5 | Presenter Mode |
| v1.0.0 | Glass | 10 | Polish + Stable API |

**Total**: 60 slices ≈ 6-8 weeks at current 15-min-cron pace.

When v1.0 lands, Slab is the answer to: *"what's the local Adobe-free Mac PDF tool?"*
