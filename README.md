# Slab 🍰

> **The PDF toolkit your files never leave.**
> A fast, free, fully offline PDF app for macOS, Windows, and Linux — plus a self-hostable Docker server for headless and homelab use.

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/Sanjays2402/slab?label=release)](https://github.com/Sanjays2402/slab/releases/latest)

Slab is a desktop PDF app that runs **entirely on your machine.** No accounts. No uploads. No cloud round‑trips. Open a PDF, do something useful with it, save the result — same as any other app on your computer.

![Slab Reader with a PDF open](docs/screenshots/00-hero-reader.png)

## Highlights

- **Local‑first.** Your documents never touch a server. Air‑gap a laptop and Slab still works — including the AI features.
- **Fast.** Native Rust under the hood. Merging a hundred files takes a second, not a minute.
- **Tiny.** Installers are ~15–25 MB depending on platform.
- **Honest.** Free forever. GPL‑3.0. No "Pro tier." No telemetry. No upsells.
- **Beautiful.** A dark‑first interface with themeable accent colors and three density modes.
- **Extensible.** A declarative plugin system (Foundry) lets you add themes, locales, commands, AI providers, and PDF actions without writing Rust. A typed TypeScript SDK (Workshop, v2.0+) ships three example plugins right in the binary — fully working, sandboxed, source-readable — so you can poke at the plugin system the moment you open the app.
- **Self-hostable.** v2.1.0 adds **Slab Server** — the same Rust PDF core wrapped in an HTTP API + drag-drop web UI, distributed as a 80MB Docker image (`ghcr.io/sanjays2402/slab`). Run it on a NAS, in Compose, behind a reverse proxy. See [docs/server.md](docs/server.md).

## Run it as a server

`docker run --rm -p 8080:8080 ghcr.io/sanjays2402/slab:latest` — open http://localhost:8080, drop a PDF on the page. Same Rust core as the desktop app, headless and scriptable. Full HTTP API + Compose example: [docs/server.md](docs/server.md).

## What Slab does

Eight pillars, all local:

| Pillar | What it does |
| --- | --- |
| **Read & navigate** | Open PDFs, thumbnails, outline, find, highlights, sticky notes, recents |
| **Manipulate** | Merge / Split / Pages / Compress / Extract / Encrypt / Watermark / Convert / Metadata / Numbers / Sign / Crop / Insert / Headers&Footers / Redact / N‑up |
| **Document tools** | Markdown→PDF · Grayscale · Page Labels · Auto‑Redact · Polyglot (.docx/.xlsx/.epub/.html/...) · Flatten · Sanitize · Repair · Edit Text |
| **OCR & vision** | Tesseract OCR plus batch queue, table extraction, language packs, preflight, and vision Q&A in Beacon |
| **AI (local)** | **Beacon** — Chat · Summary · Semantic Search · PII Redact · Smart Outline · Citations · Study Mode · Glossary · Voice Mode (TTS + STT), on‑device only |
| **Productivity** | PDF Library · line‑level diff · presenter mode · settings + keyboard shortcuts · detachable panels |
| **Reach** | Modal Vim mode · WCAG‑level a11y audit + fixes · i18n foundation |
| **Extensible** | **Foundry** + **Bench** — declarative plugin system with a signed, in‑app marketplace |

A full Rust test suite (730+ tests) passing, clippy‑clean with `-D warnings`, type‑checked Svelte 5 front‑end. Cross‑platform CI on macOS, Windows, and Linux.

## The toolkit, one by one

### ⌘K Command Palette
Jump to any tool from anywhere. Type a few letters and hit Enter.

![Command palette](docs/screenshots/39-command-palette.png)

### Reader
PDF viewing done right: thumbnails, outline, find, zoom, fit‑width / fit‑page.

![Reader](docs/screenshots/01-reader.png)

### Library
A browsable library view across every PDF you've imported — folders, tags, search‑within‑library, thumbnail grid, watched‑folder auto‑indexing.

![Library](docs/screenshots/02-library.png)

### Beacon — local AI

Eleven AI features that run **entirely on your machine** by default. No API keys, no cloud, no telemetry — the same air‑gap promise as every other Slab tool.

**Core (since v0.10):**

- **Beacon Chat** — Q&A against the open PDF. Citations point back to exact pages.
- **Beacon Summary** — TL;DR / Short / Long summaries on demand.
- **Beacon Search** — semantic search across every PDF you've opened, ranked by meaning instead of keyword.
- **Beacon PII Redact** — one‑click prep for safe sharing: AI finds names, emails, addresses, account numbers, etc. and proposes redactions.
- **Selection Actions** — floating LLM bubble appears on text highlight (Explain · Simplify · Translate · Define · custom).

**Reading & study (post‑Bench):**

- **✦ Smart Outline** *(v1.5)* — Beacon proposes a clean H1/H2/H3 table of contents from the body text. Every page number is validated against the document; near‑duplicates are collapsed. Accept the whole proposal, edit inline, or reject. Save replaces the working outline.
- **📑 Citations** *(v1.6)* — scans the body for author‑year, bracket‑number `[12]`, and bracket‑key `[smith2024]` inline cites, extracts the end‑matter bibliography with the LLM, and links every mention to its reference. Mention‑count badges, expandable inline‑cite chips, one‑click jump‑to‑page. Regex‑only fallback works offline.
- **🎓 Study Mode** *(v1.7)* — turns the PDF into a flashcard deck. LLM extracts Q&A pairs from the body, you review one card at a time with a 4‑button ease scale, and an SM‑2‑lite scheduler decides when each card is due next. Cards persist in `~/.slab/study.sqlite`. Click a card's page number to jump straight back to the source.
- **📖 Glossary** *(v1.8)* — high‑precision detector for jargon, ALL‑CAPS acronyms, *italic* term‑spans, and parenthetical acronym definitions. The LLM writes a one‑line definition for each candidate; the result is cached as a JSON sidecar next to the PDF so the second open is instant. Filterable, copyable, with a **Clear cache** button to rebuild.

**Voice Mode (v1.9):**

- **🔊 Speak (TTS)** *(v1.9.0)* — highlight a summary, an answer, or any selection and let your machine read it aloud through the native engine: **`say`** on macOS, **`espeak-ng`** on Linux, **PowerShell SpeechSynthesizer** on Windows. Voice + words‑per‑minute slider. Pressing speak twice cancels the in‑flight utterance — single‑slot, no queue.
- **🎙 Listen (STT)** *(v1.9.1)* — tap the mic in Beacon Chat, dictate, and an on‑device **whisper.cpp** transcribes the audio straight into the composer. macOS uses `sox` to capture, Linux uses `arecord`, Windows uses PowerShell `SoundRecorder`; the WAV is unlinked unconditionally on every code path. Nothing is uploaded, nothing lingers on disk.
- **🎙 Listen Polish** *(v1.9.2)* — three sharp UX wins on top of v1.9.1: press **Esc** (or right‑click the mic) to **cancel** a recording — the in‑flight WAV is unlinked immediately and no transcript is produced; **voice‑to‑send** trigger phrase (default `send it`) auto‑submits your dictated prompt for true hands‑free chat; and a **Whisper model picker** (tiny.en / base.en / small.en, plus any custom `ggml-*.bin` dropped into `~/.slab/models/`) so you can swap models per‑recording or set a default.

**Pluggable AI provider** — Ollama is the default; any OpenAI‑compatible endpoint is a config away (LM Studio, vLLM, a remote host, or a Foundry plugin).

![Beacon AI Chat](docs/screenshots/03-beacon-ai.png)

![Beacon Search](docs/screenshots/04-beacon-search.png)

![Beacon PII Redact](docs/screenshots/05-pii-redact.png)

Powered by local embeddings + an on‑device chat model (configurable). The first model download is the only thing that hits the network; after that, Beacon works offline.

### Merge
Drop multiple PDFs, drag to reorder, save the combined file anywhere.

![Merge](docs/screenshots/06-merge.png)

### Split
Pull out a page range, or chop a PDF into chunks of N pages.

![Split](docs/screenshots/07-split.png)

### Split by Chapter
Regex‑driven or outline‑driven chapter splitting; one PDF in, one PDF per chapter out.

![Split by chapter](docs/screenshots/08-split-by-chapter.png)

### Pages
Rotate any page 90°/180°/270°, delete pages, reorder by drag, duplicate, blank inserts (A4/Letter/Legal).

![Pages](docs/screenshots/09-pages.png)

![Pages list](docs/screenshots/10-pages-list.png)

### Edit Text
In‑place PDF text editing. Click a word, change it, save. Slab rewrites the content stream while preserving fonts, positioning, and surrounding layout.

![Edit text](docs/screenshots/11-edit-text.png)

### Compress
Lossless content‑stream re‑compression with bytes‑saved report.

![Compress](docs/screenshots/12-compress.png)

### Extract
Page‑by‑page text preview. Copy snippets or save the whole thing as `.txt`.

![Extract](docs/screenshots/13-extract.png)

### Encrypt
Lock with a password (RC4‑40, universally compatible). Unlock just as easily.

![Encrypt](docs/screenshots/14-encrypt.png)

### Watermark
Text at any angle, with opacity and gray controls. Whole document or specific pages.

![Watermark](docs/screenshots/15-watermark.png)

### Convert
Two‑way conversion: PDF → PNG/JPG/WebP, or images → PDF.

![Convert](docs/screenshots/16-convert.png)

### Metadata
View every Info‑dictionary field (Title, Author, Subject, Keywords, Creator, Producer), edit them, or hit one button to strip everything (plus XMP) for a truly anonymous PDF.

![Metadata](docs/screenshots/17-metadata.png)

### Page Numbers
Stamp page numbers with a template like `Page {n} of {total}`, in any of 6 positions, with custom font size, gray level, starting number, and a "skip first N pages" option for covers.

![Page numbers](docs/screenshots/18-numbers.png)

### Sign & Stamp
Drop a signature scan, company logo, or `APPROVED` stamp onto any page. Position, scale, and opacity all live‑tweakable.

![Sign & Stamp](docs/screenshots/19-sign.png)

### Crop
Trim margins by percentage from each edge. Optionally rewrite the MediaBox so downstream tools see the new size, not just a clipped view.

![Crop](docs/screenshots/20-crop.png)

### Insert Pages
Splice blank A4/Letter/Legal pages or pages from another PDF at any 1‑indexed position. Insert before or after, in bulk.

![Insert](docs/screenshots/21-insert.png)

### Headers & Footers
Stamp templated text bands across every page. `{n}`, `{total}`, `{date}`, `{filename}` tokens, six anchor positions, custom font size + opacity.

![Header / Footer](docs/screenshots/22-header-footer.png)

### Redact
Paint solid black rectangles over sensitive regions. Page‑by‑page, drag to draw, burned into the content stream — no scrubbing reveals the original.

![Redact](docs/screenshots/23-redact.png)

### Auto‑Redact
Find and cover sensitive content automatically. Built‑in presets for **emails**, **US SSNs**, **phone numbers**, and **credit cards**. Add your own regex patterns. Adjustable bar color. Line‑level bounding boxes drawn over each match.

![Auto-Redact](docs/screenshots/24-auto-redact.png)

### N‑up
Compose 2, 4, 6, or 9 pages onto a single sheet for printing — landscape or portrait, configurable spacing, exact reproductions of each source page.

![N-up](docs/screenshots/25-n-up.png)

### Markdown → PDF
Write or paste Markdown, click Convert, get a clean PDF. Headings, **bold**, *italic*, `code`, lists, blockquotes, code blocks, horizontal rules — all rendered with standard Helvetica. No font embedding means tiny output files (~1 KB per page).

### Bind — PDF → EPUB 3 *(v3.18.0)*

Drop a research paper, novel, or long-form article. Get a reflowable EPUB 3 file your Kindle, Apple Books, Kobo, or Calibre opens natively. Adobe Acrobat doesn't ship EPUB export. Calibre's PDF→EPUB is a 2008 GUI. Slab's is offline, free, and ships on every platform.

| Format | Slab | Acrobat | PDF Expert | Foxit | Calibre |
| --- | --- | --- | --- | --- | --- |
| EPUB 3 | ✅ Free, offline, 1-click | ❌ none | ❌ none | ❌ none | ⚠️ 2008 GUI |

Bind splits on H1 headings by default (one chapter per section), emits semantic XHTML5 with `<h1>/<p>/<ul>/<table>` markup and a reflow-friendly stylesheet, and produces spec-compliant containers (mimetype-first / Stored compression / nav.xhtml / OPF). 13 e-reader languages out of the box.

![Markdown → PDF](docs/screenshots/26-markdown-pdf.png)

### Grayscale
Convert RGB and CMYK fills and strokes to gray inside PDF content streams. Vector‑true — no rasterization — using ITU‑R BT.601 luminance. Range‑selectable. Embedded raster images are unchanged in this pass.

![Grayscale](docs/screenshots/27-grayscale.png)

### Page Labels
Control how PDF readers display page numbers: roman numerals for front matter, arabic for the body, custom prefixes for chapters. Multiple ranges in one shot, live preview of exactly what each style produces. Sets the catalog's `/PageLabels` number tree per the PDF spec.

![Page Labels](docs/screenshots/28-page-labels.png)

### Flatten
Bake form fields and annotations into the page so the output PDF has no editable layers. Visual appearance preserved; interactivity gone.

![Flatten](docs/screenshots/29-flatten.png)

### Sanitize
Make a PDF safe to forward. Strips JavaScript, embedded files, launch actions, `/OpenAction`, `/AA`, XFA, and (by default) external URI links. Pixel‑identical output.

![Sanitize](docs/screenshots/30-sanitize.png)

### Repair
Rebuild the xref table and drop unreachable indirect objects. Fixes most "this PDF won't open" files and shrinks PDFs bloated by incremental edits.

![Repair](docs/screenshots/31-repair.png)

### OCR
Tesseract‑backed OCR turns scanned documents into searchable, copyable text. Language packs managed in‑app. Batch queue, preflight diagnostics, and vision Q&A in Beacon for figures and charts.

![OCR](docs/screenshots/32-ocr.png)

### Tables → CSV
Ruled and unruled grid detection across the open PDF. Pick a page, pick a region, export to CSV.

![Tables to CSV](docs/screenshots/33-tables-csv.png)

### Diff
Line‑level text diff between any two PDFs, with optional Beacon‑powered "Explain Changes" plain‑English summaries. Export a **Change Report** as PDF for review workflows. The diff itself, like everything else, never leaves your machine.

![Diff](docs/screenshots/34-diff.png)

### Slides — presenter mode
Turn any PDF into a deck. Full‑screen slideshow, speaker notes view, **live annotation on top of slides**, **save the annotated deck back out as a new PDF**, and a remote keyboard shortcut layout.

![Slides](docs/screenshots/35-slides.png)

### Settings
A proper Settings system with theme + accent color + density (compact / cozy / comfortable), a customizable keymap, global toast notifications, pinned recent files.

![Settings](docs/screenshots/36-settings.png)

### Plugins (Foundry + Bench)

**Foundry** is the declarative plugin system: drop a folder containing a `plugin.toml` manifest into `~/.slab/plugins/`, restart, done. No Rust compile, no native code. Five contribution kinds:

| Kind | What it does | Backed by |
| --- | --- | --- |
| **Theme** | Override CSS variables to restyle Slab. | CSS file in `themes/`. |
| **Locale** | Add or override an interface language. | JSON file in `locales/`. |
| **Command** | Run a shell command or open a URL from the palette. | TOML entry; quoting‑aware tokenizer. |
| **AI provider** | Register any OpenAI‑compatible endpoint. Appears in Beacon. | TOML entry; Chat Completions wire format. |
| **PDF action** | Reader toolbar dropdown that pipes the open PDF through a CLI. | TOML with `{in}` / `{out}` placeholders. |

**Bench** *(v1.4)* is the in‑app marketplace on top of Foundry: a curated, **Ed25519‑signed** index of plugins users can browse and install in one click — no terminal needed. Every entry is signed by the maintainer; before install Slab verifies the entry signature against the embedded public key, then verifies the tarball's SHA‑256 matches the signed `manifest_sha256`. If any step fails, install aborts and nothing is written. **Provenance** end‑to‑end.

A **Settings → Plugins** panel lists every installed plugin with toggle, version, author, contribution counts, expandable drilldown, raw manifest errors, plus a **📁 Open plugins directory** button. A **Browse** tab surfaces the Bench index with one‑click install, update badges, and an uninstall affordance.

![Plugins — Installed](docs/screenshots/37-plugins.png)

![Plugins — Browse](docs/screenshots/37b-plugins-browse.png)

**Honest security framing**: Foundry plugins run with Slab's permissions — there's no sandbox. Bench gives you **who shipped it**, not **what it can do**. Treat a plugin like a `bash` script you downloaded. Read the manifest before enabling; the panel shows the on‑disk path so you can `cat` it first.

📖 [Author guide: `docs/PLUGINS.md`](docs/PLUGINS.md)

### Keyboard Shortcuts
A customizable keymap and a `?` overlay that lists every keybinding.

![Shortcuts settings](docs/screenshots/38-shortcuts.png)

![Shortcuts overlay](docs/screenshots/40-shortcuts-overlay.png)

### Standalone CLI
A separate `slab` binary ships in every bundle alongside the GUI. Every op available from the terminal — no Tauri runtime, no IPC, direct library calls.

```bash
slab md2pdf input.md output.pdf --page-size Letter
slab grayscale input.pdf output.pdf
slab autoredact input.pdf output.pdf --preset email,ssn
slab info report.pdf
```

### Polyglot — beyond PDFs

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

### Detachable panels

Any sidebar panel detaches into its own native window. Drop Beacon on a second monitor while the Reader stays on the first. Open three Library views, each filtered to a different folder. Run Diff next to two side‑by‑side Readers. Cross‑window events keep it coherent. Window geometry + which‑panels‑were‑open survives app restart (`~/.slab/windows.json`).

### Vim mode

A modal **Vim mode** built from a clean pure state machine — `gg`/`G`/`j`/`k`/`Ctrl-d`/`Ctrl-u`, count prefixes (`10j`), `/foo<CR>` + `n`/`N`, `:42<CR>` to jump to a page, `:q` to close. Reader, Library, and Beacon all wired. Cmd/Ctrl shortcuts are reserved for the app — Vim never eats your Cmd‑F.

### Accessibility & i18n

A built‑in **accessibility audit** (`pnpm a11y:audit`, zero deps) flags icon buttons missing labels, unlabelled form inputs, and images without alt. The strict variant runs in CI on every push. Plus a global `:focus-visible` ring tied to your accent color, `prefers-reduced-motion` overrides, `prefers-contrast: more` border thickening, and proper `<nav aria-label="Primary">` + `aria-current` on the sidebar.

An **i18n foundation**: every string in the UI passes through a `t(key)` function backed by JSON locale files. English ships today; community translations welcome.

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

**Optional runtime deps** for the AI side:

- **Beacon Chat/Search/Summary/etc.** — install [Ollama](https://ollama.com), then `ollama pull llama3.2:3b` (or any chat model). Slab also accepts any OpenAI‑compatible endpoint via Settings.
- **Voice — Speak (TTS)** — `say` ships with macOS; Linux: `apt install espeak-ng`; Windows: built‑in PowerShell SAPI.
- **Voice — Listen (STT)** — `brew install whisper-cpp sox` (macOS), `apt install whisper-cpp alsa-utils` (Linux). The capability probe in Beacon Voice settings tells you exactly what's missing.

## Tests

```bash
cd src-tauri && cargo test
cargo clippy --all-targets -- -D warnings
cd .. && pnpm exec svelte-check     # type-check the UI
```

## Extending Slab (plugins)

Foundry is Slab's declarative plugin system. Drop a folder into `~/.slab/plugins/` containing a `plugin.toml` manifest and Slab can pick up custom themes, additional UI languages, shell/URL commands, OpenAI-compatible AI providers, and CLI-backed PDF actions — no compilation, no native code.

- **Author guide:** [`docs/PLUGINS.md`](docs/PLUGINS.md) — full manifest reference, contribution kinds, troubleshooting.
- **Working example:** [`examples/plugins/hello-slab/`](examples/plugins/hello-slab/) — copy-paste-edit reference plugin that exercises all five contribution kinds.
- **Manage plugins:** Settings → Plugins (or `Cmd-K` → "Open Settings → Plugins").
- **Discover plugins:** Settings → Plugins → **Browse** (the Bench marketplace) — every entry signed by the maintainer, hash‑verified on install.

## Under the hood

- **Shell:** [Tauri 2](https://tauri.app) — system webview, ~15–25 MB binaries, native menus.
- **UI:** [SvelteKit](https://svelte.dev) + Svelte 5 runes + TypeScript.
- **PDF core:** [`lopdf`](https://crates.io/crates/lopdf) (pure Rust) for manipulation, [`pdfjs-dist`](https://www.npmjs.com/package/pdfjs-dist) for rendering in the Reader, [`pdf-lib`](https://pdf-lib.js.org) for client‑side composition (stamps, image embedding), [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) for Markdown → PDF, [`pdfium-render`](https://crates.io/crates/pdfium-render) + [`tesseract-rs`](https://crates.io/crates/tesseract-rs) for OCR.
- **AI:** local embeddings + on‑device chat model for Beacon (configurable, network only touched during first model download). **Voice Mode** uses native OS engines (`say` / `espeak-ng` / PowerShell SAPI) for TTS and **whisper.cpp** for on‑device STT — Slab itself never holds your audio on disk longer than the transcription call.
- **License:** GPL‑3.0 — free as in freedom. Fork it, ship it, just don't close‑source it.

## A small promise

Slab will never ask for an email. Will never call home. Will never gate a feature behind a paywall. If it ever does any of those things, you have my permission to fork it and rip the offending lines out.

Made with 🍰 by [@Sanjays2402](https://github.com/Sanjays2402).
