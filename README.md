# Slab 🍰

> **The PDF toolkit your files never leave.**
> A fast, free, fully offline PDF app for macOS, Windows, and Linux.

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Rust tests](https://img.shields.io/badge/rust%20tests-581%20passing-brightgreen.svg)](src-tauri)
[![Latest release](https://img.shields.io/github/v/release/Sanjays2402/slab?label=release)](https://github.com/Sanjays2402/slab/releases/latest)

Every other PDF tool wants you to upload your taxes, your contracts, your medical records to a server you've never heard of, then watch a 30‑second ad, then pay $9.99 to remove the watermark. Slab does the opposite: **everything runs locally on your machine.** No accounts. No uploads. No subscriptions. No nonsense.

![Slab Reader with the PDF 1.7 spec open](docs/screenshots/00-hero-reader.png)

## What's in v1.3.1 "Foundry Patch"

A real Adobe‑Acrobat‑replacing toolkit, shipped in **20 honest releases**. Eight pillars, all local:

| Pillar | What it does | Highlight versions |
| --- | --- | --- |
| **Read & navigate** | Open PDFs, thumbnails, outline, find, highlights, sticky notes, recents | v0.5 → v0.8 |
| **Manipulate** | Merge / Split / Pages / Compress / Extract / Encrypt / Watermark / Convert / Metadata / Numbers / Sign / Crop / Insert / Headers&Footers / Redact / N‑up | v0.5 → v0.7 |
| **Document tools** | Markdown→PDF · Grayscale · Page Labels · Auto‑Redact · Polyglot (.docx/.xlsx/.epub/.html/...) · Flatten · Sanitize · Repair · Edit Text | v0.7 → v0.11 |
| **OCR & Lens** | Tesseract OCR (v0.8) plus a full Lens panel: table extraction, language packs, batch, preflight (v0.13) | v0.8, v0.13 |
| **AI (local)** | **Beacon** — Chat / Summary / Semantic Search across the open PDF, on‑device only | v0.10 |
| **Productivity** | **Atlas** PDF Library · **Stack** line‑level diff · **Theater** present mode · **Glass** settings + keyboard shortcuts · **Cabinet** detachable panels | v0.12, v0.14, v0.15, v1.0, v1.1 |
| **Reach** | **Vim mode** · WCAG‑level a11y audit + fixes · i18n foundation | v1.2 |
| **Extensible** | **Foundry** — declarative plugin system (themes, locales, commands, AI providers, PDF actions) via TOML manifest | v1.3 |

**581 Rust tests passing**, clippy‑clean with `-D warnings`, type‑checked Svelte 5 front‑end. Cross‑platform CI on macOS, Windows, and Linux.

## Why Slab?

- **Local‑first.** Your documents never touch a server. Air‑gap a laptop and Slab still works — including the AI features.
- **Fast.** Native Rust under the hood. Merging a hundred files takes a second, not a minute.
- **Tiny.** Installers are ~15–25 MB depending on platform. Compare that to anything else in the category.
- **Honest.** Free forever. GPL‑3.0. No "Pro tier." No telemetry. No upsells.
- **Beautiful.** A dark‑first interface with themeable accent colors and three density modes (Glass settings).

## The toolkit, one by one

### ⌘K Command Palette
Jump to any tool from anywhere. Type a few letters and hit Enter.

![Command palette](docs/screenshots/00-hero-palette.png)

### Reader
PDF viewing done right: thumbnails, outline, find, zoom, fit‑width / fit‑page.

![Reader](docs/screenshots/01-reader.png)

### Merge
Drop multiple PDFs, drag to reorder, save the combined file anywhere.

![Merge](docs/screenshots/02-merge.png)

### Split
Pull out a page range, or chop a PDF into chunks of N pages.

![Split](docs/screenshots/03-split.png)

### Pages
Rotate any page 90°/180°/270°, delete pages, reorder by drag.

![Pages](docs/screenshots/04-pages.png)

### Compress
Lossless content‑stream re‑compression with bytes‑saved report.

![Compress](docs/screenshots/05-compress.png)

### Extract
Page‑by‑page text preview. Copy snippets or save the whole thing as `.txt`.

![Extract](docs/screenshots/06-extract.png)

### Encrypt
Lock with a password (RC4‑40, universally compatible). Unlock just as easily.

![Encrypt](docs/screenshots/07-encrypt.png)

### Watermark
Text at any angle, with opacity and gray controls. Whole document or specific pages.

![Watermark](docs/screenshots/08-watermark.png)

### Convert
Two‑way conversion: PDF → PNG/JPG/WebP, or images → PDF.

![Convert](docs/screenshots/09-convert.png)

### Metadata
View every Info‑dictionary field (Title, Author, Subject, Keywords, Creator, Producer), edit them, or hit one button to strip everything (plus XMP) for a truly anonymous PDF.

![Metadata](docs/screenshots/10-metadata.png)

### Page Numbers
Stamp page numbers with a template like `Page {n} of {total}`, in any of 6 positions, with custom font size, gray level, starting number, and a "skip first N pages" option for covers.

![Page numbers](docs/screenshots/11-numbers.png)

### Sign & Stamp
Drop a signature scan, company logo, or `APPROVED` stamp onto any page. Position, scale, and opacity all live‑tweakable.

![Sign & Stamp](docs/screenshots/12-sign.png)

### Crop
Trim margins by percentage from each edge. Optionally rewrite the MediaBox so downstream tools see the new size, not just a clipped view.

![Crop](docs/screenshots/18-crop.png)

### Insert Pages
Splice blank A4/Letter/Legal pages or pages from another PDF at any 1‑indexed position. Insert before or after, in bulk.

![Insert](docs/screenshots/19-insert.png)

### Headers & Footers
Stamp templated text bands across every page. `{n}`, `{total}`, `{date}`, `{filename}` tokens, six anchor positions, custom font size + opacity.

![Header / Footer](docs/screenshots/20-headerfooter.png)

### Redact
Paint solid black rectangles over sensitive regions. Page‑by‑page, drag to draw, burned into the content stream — no scrubbing reveals the original.

![Redact](docs/screenshots/21-redact.png)

### N‑up
Compose 2, 4, 6, or 9 pages onto a single sheet for printing — landscape or portrait, configurable spacing, exact reproductions of each source page.

![N-up](docs/screenshots/17-nup.png)

### Markdown → PDF
Write or paste Markdown, click Convert, get a clean PDF. Headings, **bold**, *italic*, `code`, lists, blockquotes, code blocks, horizontal rules — all rendered with standard Helvetica. No font embedding means tiny output files (~1 KB per page).

![Markdown → PDF](docs/screenshots/13-markdown.png)

### Grayscale
Convert RGB and CMYK fills and strokes to gray inside PDF content streams. Vector‑true — no rasterization — using ITU‑R BT.601 luminance. Range‑selectable. Embedded raster images are unchanged in this pass.

![Grayscale](docs/screenshots/14-grayscale.png)

### Page Labels
Control how PDF readers display page numbers: roman numerals for front matter, arabic for the body, custom prefixes for chapters. Multiple ranges in one shot, live preview of exactly what each style produces. Sets the catalog's `/PageLabels` number tree per the PDF spec.

![Page Labels](docs/screenshots/15-labels.png)

### Auto‑Redact
Find and cover sensitive content automatically. Built‑in presets for **emails**, **US SSNs**, **phone numbers**, and **credit cards**. Add your own regex patterns. Adjustable bar color. Line‑level bounding boxes drawn over each match.

![Auto-Redact](docs/screenshots/16-autoredact.png)

### Standalone CLI
A separate `slab` binary ships in every bundle alongside the GUI. Every op available from the terminal — no Tauri runtime, no IPC, direct library calls.

```bash
slab md2pdf input.md output.pdf --page-size Letter
slab grayscale input.pdf output.pdf
slab autoredact input.pdf output.pdf --preset email,ssn
slab info report.pdf
```

### Compass — annotations & recents *(v0.8.0)*
Highlights and sticky notes with a real annotation layer (saved into `/Annots` per spec). Edit the outline (Table of Contents) directly. Recents grid with thumbnails. Tesseract‑backed OCR makes scanned documents searchable. Drag‑and‑drop file opening everywhere. A dark‑mode polish pass. A keyboard cheatsheet (`?`). And one‑click annotation export to Markdown.

### Polyglot — eat more than PDFs *(v0.8.1)*

Point Slab at:

- **Office docs:** `.docx` `.xlsx` `.pptx` `.xls`
- **Web & structured text:** `.html` `.htm` `.csv` `.json` `.xml` `.rtf` `.odt`
- **Books:** `.epub`
- **Images:** `.png` `.jpg` `.gif` `.bmp` `.tif` `.webp` (EXIF + OCR text)
- **Audio:** `.wav` `.mp3` `.m4a` `.flac` `.ogg` (EXIF + transcription)

Under the hood Slab shells out to Microsoft's [markitdown](https://github.com/microsoft/markitdown) (MIT) to extract Markdown, then renders to PDF through the same `md2pdf` engine. Zero new Rust dependencies; PDF → PDF round‑tripping is deliberately refused (lossy).

**Requires:** `pipx install 'markitdown[all]'` (one‑time, optional — Slab still works without it for PDF input).

```bash
slab polyglot report.docx -o report.pdf
slab polyglot data.xlsx -o data.pdf --page-size Letter
slab polyglot book.epub -o book.pdf
```

### Toolkit: flatten, sanitize, repair *(v0.9.0–v0.9.1)*

The three utilities `pdftk` / `qpdf` users reach for most, native to Slab and pure‑Rust (no external binaries). All three exposed in the sidebar as panels and as Tauri commands.

**Flatten** — bake form fields and annotations into the page so the output PDF has no editable layers. Visual appearance preserved; interactivity gone.

```bash
slab flatten editable.pdf -o flat.pdf
slab flatten editable.pdf -o flat.pdf --no-widgets   # keep form widgets, flatten only annotations
```

**Sanitize** — make a PDF safe to forward. Strips JavaScript, embedded files, launch actions, `/OpenAction`, `/AA`, XFA, and (by default) external URI links. Pixel‑identical output.

```bash
slab sanitize sketchy.pdf -o clean.pdf
slab sanitize sketchy.pdf -o clean.pdf --keep-links  # leave http(s) URI actions intact
```

**Repair** — rebuild the xref table and drop unreachable indirect objects. Fixes most "this PDF won't open" files and shrinks PDFs bloated by incremental edits.

```bash
slab repair busted.pdf -o fixed.pdf
# ✓ repaired: objects 412 → 318 (94 pruned), size 1.2 MB → 980 KB (-18.3%) → fixed.pdf
```

v0.9.1 also adds a **decrypt‑on‑open** prompt so password‑protected PDFs unlock with one click.

### Beacon — local AI you actually own *(v0.10.0)*

Six AI features that run **entirely on your machine** by default. No API keys, no cloud, no telemetry — the same air‑gap promise as every other Slab tool.

- **Beacon Chat** — Q&A against the open PDF. Citations point back to exact pages.
- **Beacon Summary** — TL;DR / Short / Long summaries on demand.
- **Beacon Search** — semantic search across every PDF you've opened, ranked by meaning instead of keyword.
- **Beacon PII Redact** — one‑click prep for safe sharing: AI finds names, emails, addresses, account numbers, etc. and proposes redactions.
- **Selection Actions** — floating LLM bubble appears on text highlight (Explain · Simplify · Translate · Define · custom).
- **Pluggable AI provider** — Ollama is the default; any OpenAI‑compatible endpoint is a config away (LM Studio, vLLM, a remote host, or a Foundry plugin in v1.3).

Powered by local embeddings + an on‑device chat model (configurable). The first model download is the only thing that hits the network; after that, Beacon works offline.

### Lathe — Edit Text, Multi-tabs, Chapter Split *(v0.11.0)*

- **Edit Text** — in‑place PDF text editing. Click a word, change it, save. Slab rewrites the content stream while preserving fonts, positioning, and surrounding layout.
- **Pages (visual)** — drag‑reorder, duplicate, blank inserts (A4/Letter/Legal), rotate.
- **Split by Chapter** — regex‑driven or outline‑driven chapter splitting; one PDF in, one PDF per chapter out.
- **Multi‑PDF tabs** in the Reader — open several documents side‑by‑side, switch with `Cmd-1..9`.

### Atlas — the PDF Library *(v0.12.0)*

A browsable library view across every PDF you've imported. Folders, tags, search‑within‑library, thumbnail grid, and a watched‑folder mode that auto‑indexes new files as they appear. The recents grid grew up.

### Lens — OCR, Vision, Tables, Auto-tag *(v0.13.0–v0.13.1)*

Beyond the v0.8 OCR baseline, Lens is Slab's full visual‑intelligence layer:

- **Batch OCR** with a real job queue (`slab lens ocr-queue`)
- **Table extraction** — ruled and unruled grid detection, export to CSV (`slab lens tables`)
- **Vision Q&A in Beacon** — ask questions about figures, charts, and scanned pages (uses any vision‑capable AI provider).
- **AI Auto‑tag** — per‑card and bulk auto‑tagging across the library (`slab lens auto-tag`).
- **Language packs** — downloadable Tesseract language data, managed in‑app.
- **Audit + preflight** — `slab lens audit` reports which docs are searchable; `slab lens preflight` diagnoses missing dependencies.
- v0.13.1 fixes Windows `pdftotext` flavor detection.

### Stack — text diff for PDFs *(v0.14.0)*

Line‑level text diff between any two PDFs, with optional Beacon‑powered "Explain Changes" plain‑English summaries. Export a **Change Report** as PDF for review workflows. The diff itself, like everything else, never leaves your machine.

### Theater — presenter mode *(v0.15.0)*

Turn any PDF into a deck. Full‑screen slideshow, speaker notes view, **live annotation on top of slides**, **save the annotated deck back out as a new PDF**, and a remote keyboard shortcut layout. Great for decks that started life as a PDF export.

### Glass — settings & shortcuts *(v1.0.0)*

The 1.0 polish layer: a proper Settings system with theme + accent color + density (compact / cozy / comfortable), a customizable keymap, an MRU‑sorted command palette, a `?` shortcut overlay that lists every keybinding, **global toast notifications**, **pinned recent files**, and a **first‑launch onboarding tour**.

### Cabinet — detachable panels *(v1.1.0)*

Any of **11 panels** detaches into its own native window. Drop Beacon on a second monitor while the Reader stays on the first. Open three Library views, each filtered to a different folder. Run Stack diff next to two side‑by‑side Readers. Cross‑window events keep it coherent: add a folder in the main Library, every detached Library refetches in milliseconds. Window geometry + which‑panels‑were‑open survives app restart (`~/.slab/windows.json`). A new sidebar "Detached" section lists every open window, one click to focus.

### Glass II — Vim, a11y, i18n *(v1.2.0)*

A modal **Vim mode** built from a clean pure state machine — `gg`/`G`/`j`/`k`/`Ctrl-d`/`Ctrl-u`, count prefixes (`10j`), `/foo<CR>` + `n`/`N`, `:42<CR>` to jump to a page, `:q` to close. Reader, Library, and Beacon all wired. Cmd/Ctrl shortcuts are reserved for the app — Vim never eats your Cmd‑F.

An **accessibility audit** (`pnpm a11y:audit`, zero deps) flags icon buttons missing labels, unlabelled form inputs, and images without alt. Baseline ran clean across every Svelte file in the tree; the strict variant runs in CI on every push. Plus a global `:focus-visible` ring tied to your accent color, `prefers-reduced-motion` overrides, `prefers-contrast: more` border thickening, and proper `<nav aria-label="Primary">` + `aria-current` on the sidebar.

An **i18n foundation**: every string in the UI passes through a `t(key)` function backed by JSON locale files. English ships today; community translations welcome.

### Foundry — declarative plugin system *(v1.3.0)*

**Slab becomes extensible.** Drop a folder containing a `plugin.toml` manifest into `~/.slab/plugins/`, restart, done. No Rust compile, no native code. Five contribution kinds:

| Kind | What it does | Backed by |
| --- | --- | --- |
| **Theme** | Override CSS variables to restyle Slab. | CSS file in `themes/`. |
| **Locale** | Add or override an interface language. | JSON file in `locales/`. |
| **Command** | Run a shell command or open a URL from the palette. | TOML entry; quoting‑aware tokenizer. |
| **AI provider** | Register any OpenAI‑compatible endpoint. Appears in Beacon. | TOML entry; Chat Completions wire format. |
| **PDF action** | Reader toolbar dropdown that pipes the open PDF through a CLI. | TOML with `{in}` / `{out}` placeholders. |

A **Settings → Plugins** panel lists every plugin with toggle, version, author, contribution counts, expandable drilldown, raw manifest errors, plus a **📁 Open plugins directory** button. Example `hello-slab` plugin ships in `examples/plugins/` exercising all five kinds.

**Honest security framing**: Foundry plugins run with Slab's permissions — there's no sandbox. Treat a plugin like a `bash` script you downloaded. Read the manifest before enabling; the panel shows the on‑disk path so you can `cat` it first. A signed‑marketplace flow is on the roadmap.

📖 [Author guide: `docs/PLUGINS.md`](docs/PLUGINS.md)

### v1.3.1 — Foundry Patch *(latest)*

Three flaky test fixes: Linux shell‑timeout race in `plugins::command_runner`, Windows absolute‑path validation in `plugins::contributions::read_asset` and `plugins::locale_loader`. No user‑facing changes.

## Install

Pre‑built installers ship with each [release](https://github.com/Sanjays2402/slab/releases): `.dmg` (macOS Apple Silicon + Intel), `.msi` + `.exe` (Windows), `.deb` + `.AppImage` + `.rpm` (Linux).

### First launch on macOS

Slab's macOS builds are currently **ad‑hoc signed** (we don't have a $99/year Apple Developer ID yet). The app and its signature are inspectable, but Gatekeeper doesn't trust the signer, so you'll see a security warning on the first launch only:

1. Open the DMG and drag **Slab** to **Applications**.
2. In Finder, **right‑click** (or Control‑click) **Slab.app** → **Open**.
3. Click **Open** in the dialog that appears.
4. That's it — every subsequent launch is a normal double‑click.

To verify the signature integrity yourself:

```bash
codesign -dvv /Applications/Slab.app
```

If you'd like to help fund a Developer ID certificate (so this prompt goes away for everyone), see [SIGNING.md](SIGNING.md). CI is already wired to switch to full Developer ID signing + notarization the moment the six GitHub secrets are configured.

### Build from source

Prereqs: Rust ≥ 1.75, Node ≥ 20, pnpm ≥ 9.

```bash
git clone https://github.com/Sanjays2402/slab
cd slab
pnpm install
pnpm tauri dev          # run in dev mode
pnpm tauri build        # produce an installer / app bundle for your platform
```

## Tests

```bash
cd src-tauri && cargo test          # 581 tests
cargo clippy --all-targets -- -D warnings
cd .. && pnpm exec svelte-check     # type-check the UI
```

## Extending Slab (plugins)

Slab v1.3.0 "Foundry" introduces a declarative plugin system. Drop a folder into `~/.slab/plugins/` containing a `plugin.toml` manifest and Slab can pick up custom themes, additional UI languages, shell/URL commands, OpenAI-compatible AI providers, and CLI-backed PDF actions — no compilation, no native code.

- **Author guide:** [`docs/PLUGINS.md`](docs/PLUGINS.md) — full manifest reference, contribution kinds, troubleshooting.
- **Working example:** [`examples/plugins/hello-slab/`](examples/plugins/hello-slab/) — copy-paste-edit reference plugin that exercises all five contribution kinds.
- **Manage plugins:** Settings → Plugins (or `Cmd-K` → "Open Settings → Plugins").

## Under the hood

- **Shell:** [Tauri 2](https://tauri.app) — system webview, ~15–25 MB binaries, native menus.
- **UI:** [SvelteKit](https://svelte.dev) + Svelte 5 runes + TypeScript.
- **PDF core:** [`lopdf`](https://crates.io/crates/lopdf) (pure Rust) for manipulation, [`pdfjs-dist`](https://www.npmjs.com/package/pdfjs-dist) for rendering in the Reader, [`pdf-lib`](https://pdf-lib.js.org) for client‑side composition (stamps, image embedding), [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) for Markdown → PDF, [`pdfium-render`](https://crates.io/crates/pdfium-render) + [`tesseract-rs`](https://crates.io/crates/tesseract-rs) for OCR and Lens.
- **AI:** local embeddings + on‑device chat model for Beacon (configurable, network only touched during first model download).
- **License:** GPL‑3.0 — free as in freedom. Fork it, ship it, just don't close‑source it.

## A small promise

Slab will never ask for an email. Will never call home. Will never gate a feature behind a paywall. If it ever does any of those things, you have my permission to fork it and rip the offending lines out.

Made with 🍰 by [@Sanjays2402](https://github.com/Sanjays2402).
