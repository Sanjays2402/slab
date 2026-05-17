# Beacon Vision Q&A — User Docs (v0.13.0 Slice 5)

> **TL;DR:** Beacon can now look at the rendered image of a page (not just the
> extracted text) and answer questions about it. Great for charts, diagrams,
> screenshots, scanned tables, and anything where the layout matters more than
> the words.

## When to use it

The default Beacon chat reads the **text** of your PDF. That's fast and cheap,
but it falls down when the answer lives in:

- A chart or graph (axes, trends, intersections)
- A diagram or flowchart (shapes, arrows, labels)
- A photograph or screenshot embedded in the doc
- A table that didn't survive text extraction cleanly
- A scanned page where OCR is iffy
- Page layout itself ("How is this paper structured?")

For those, switch to **👁 Vision** — Beacon will rasterize the page (or just a
region) and send the image to a vision-capable model.

## How to use it

1. Open a PDF in Beacon (the existing chat panel).
2. Click the **👁 Vision** chip in the Quick row (next to TL;DR / Summarize /
   Detailed).
3. In the drawer that slides down:
   - Pick the **page** you want to ask about.
   - (Optional, coming v0.13.1) Pre-select a region from the Reader to ask
     only about that part of the page.
   - Type your question. Examples:
     - "What does this chart show?"
     - "What's the y-axis labeled?"
     - "Walk me through this flowchart."
     - "Read out the values in this table."
4. **⌘/Ctrl + Enter** to send. **Esc** closes the drawer.

The reply appears as a normal Beacon message with a small **👁 vision** badge
so you can tell vision turns apart from text Q&A in the transcript.

## Requirements

Vision needs a vision-capable provider. In v0.13.0 that means **Ollama with a
multimodal model** — the default is `llava:7b` (≈ 4.5 GB).

```bash
# One-time install (if you don't already have it)
ollama pull llava:7b
```

Beacon will tell you if the model is missing.

You also need **poppler** installed for the page rasterization (`pdftoppm`).
Most Slab users already have it from earlier features.

```bash
# macOS
brew install poppler

# Debian/Ubuntu
sudo apt install poppler-utils
```

## Privacy

Vision Q&A runs **fully local** on the default config. The page image and
your question stay on your machine — neither is sent to any cloud service.

If you switch your Beacon provider to an OpenAI-compatible endpoint, v0.13.0
returns a clear "vision unsupported" error. Multimodal OpenAI-compat support
is planned for v0.13.1.

## Defaults under the hood

You don't need to know these — but if you're curious:

- **Render DPI:** 150
- **Max edge:** 1568 px (longest side; Triangle-filter downscale beyond that)
- **Default model:** `llava:7b`
- **Wire format:** Ollama `/api/chat` with PNG base64 in `messages[].images`
- **Temperature:** 0.2 (so the model is more grounded, less creative)
- **Token budget:** 800

The render budget is sized to fit comfortably inside LLaVA's vision patch
grid (672 × 672 at the 1.0 zoom factor) with a 2.3× safety margin, so a
typical 8.5 × 11 in page lands well within the model's context.

## Known limits in v0.13.0

- **Buffered, not streaming.** The reply appears all at once when the model
  finishes. Streaming arrives in v0.13.1 once the Tauri event channel is
  wired uniformly across all Beacon commands.
- **No rect selection from the Reader yet.** The composer accepts a `RectPts`
  via the `slab:beacon-vision-rect` event, but the Reader UI for drawing
  that rect lands in v0.13.1.
- **OpenAI-compat doesn't do vision yet.** Use Ollama.
- **`pdftoppm` shell-out, not in-process rasterization.** Means Slab is fast
  but depends on poppler being on PATH.

## Troubleshooting

- **"vision unsupported by provider …"** — your Beacon provider isn't
  multimodal. Switch to Ollama in Settings → Beacon and pull `llava:7b`.
- **"pdftoppm not found"** — install poppler (see above).
- **Reply seems hallucinated** — try a tighter region (rect) or a more
  specific question. Vision models do best with bounded, concrete prompts.
