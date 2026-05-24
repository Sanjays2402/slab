# ADR 0014 — Reflow: Offline PDF → Word (.docx)

**Status:** Accepted (2026-05-23)
**Version:** v3.14.0 "Reflow"

## Context

"Export PDF to Word" is the single most-used Acrobat Pro feature according to
every UX study Adobe has published. It's also the #1 reason people pay the
$239/yr Acrobat subscription. PDF Expert ships a cloud-only equivalent. Foxit
charges. macOS Preview doesn't ship it at all.

Slab's wedge is **local-first, free, cross-platform** — exactly what's missing
from the PDF→Word market.

## Decision

Ship `pdf::reflow` as a three-layer pipeline:

1. **Extraction** — Walk PDF content streams (PDF 1.7 §9.4 text-state machine)
   via `lopdf`, emit `Vec<TextRun>` with `(x, y, text, font, size, bold, italic)`.
2. **Layout reconstruction** — Cluster runs into lines (y-cluster), lines into
   paragraphs (gap heuristic), classify each paragraph as
   `Heading{1..3} | Body | ListItem | TableRow`.
3. **DOCX writer** — Emit OOXML inside a ZIP via `quick-xml` + `zip` 2.x.
   Open in Word, LibreOffice, Google Docs without "repair" prompt.

## Why not pdf2docx (Python) or pdf-lib?

- `pdf2docx` is Python + has known fidelity bugs with complex tables; we'd
  bundle a Python runtime. No.
- `pdf-lib` is JS; we want this in Rust so it runs in the Atelier batch
  driver + CLI + headless server.
- Hand-rolled in Rust = same toolchain as the rest of Slab, no new runtime,
  works on the server build (no Tauri required).

## Out of scope for v3.14.0

- Inline image extraction (v3.15.0 if requested).
- Font embedding in the docx (Word substitutes automatically).
- Multi-column newspaper layouts (heuristic flags them, falls back to flat body).
- RTL languages full bidi (basic Unicode bidi is in; full BiDi-Algorithm later).

## Risks

- **Fidelity** vs. Acrobat: Acrobat has 25 years of edge-case handling. We
  won't beat them on weird scanned-PDF inputs. Strategy: ship a clear UI
  message — "Best on text-based PDFs; for scanned PDFs, run OCR first."
- **DOCX validation**: OOXML is huge. We ship the minimal subset (paragraphs,
  styles, lists, tables) that opens cleanly in Word 365. Validate against
  Word's own validator before tagging.

## Buy-Button verdict

- Pay-for-it ✅ — Acrobat's #1 paid feature.
- Notice-it ✅ — New `Reflow` panel + Atelier step + Cmd+Shift+W shortcut.
- Pick-us ✅ — No competitor ships free + offline + cross-platform.
- Tell-a-friend ✅ — One-line tweet: "Drop PDF, get Word. Free. Offline."
