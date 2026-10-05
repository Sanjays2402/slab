<div align="center">

<img src="src-tauri/icons/icon.png" alt="Slab logo" width="80" height="80">

# Slab

**Free, open-source PDF tools for macOS, Windows, and Linux.**

Read, merge, split, convert, sign, and redact PDFs.
Core tools run locally; optional AI uses your configured provider.

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/Sanjays2402/slab?label=release)](https://github.com/Sanjays2402/slab/releases/latest)

<a href="https://github.com/Sanjays2402/slab/releases/latest"><img src="docs/screenshots/00-hero-reader.png" alt="Slab Reader with a PDF open" width="800"></a>

**[Download Slab](https://github.com/Sanjays2402/slab/releases/latest)** ·
[Website](https://sanjays2402.github.io/slab/) ·
[Report an issue](https://github.com/Sanjays2402/slab/issues)

**A quick first run:** open a PDF, press `⌘K`, and choose Merge, Sign, or Redact. The feature tour below previews the workflow.

</div>

---

## Features

| Feature | What you can do |
|---|---|
| Merge & split | Combine PDFs, split by page ranges or chapters, or divide into balanced files. |
| Arrange pages | Reorder, rotate, delete, duplicate, or insert blank pages, with undo and redo. |
| Convert | Convert PDF content to Word, Excel, Markdown, or HTML; create PDFs from other formats. |
| OCR & extraction | Make scans searchable and extract text, images, or tables to CSV. |
| Edit & format | Edit text, crop pages, add watermarks, headers, footers, page labels, and Bates numbers. |
| Sign & redact | Place signatures, remove sensitive content, and sanitize hidden data. |
| Compare | Inspect text and visual differences between document versions. |
| Forms | Create, fill, and flatten interactive PDF forms. |
| Automate | Process watched folders with reusable recipes and batch workflows. |
| AI assistance | Ask questions, find citations, create summaries, flashcards, and glossaries. |
| Create & present | Turn Markdown into PDFs and present PDFs as slides or in a focused reader. |
| CLI & server | Script PDF tasks from the command line or run a self-hosted HTTP API. |
| Appearance | Choose Light, Dark, OLED, White, or Glass, with nine accent colors. |

OCR requires Poppler and Tesseract. AI uses your configured local or cloud provider.

## Why Slab

| | |
|---|---|
| 🔒 **Local-first** | Core PDF tools run on your device. AI can also run locally with Ollama; cloud processing depends on the provider you configure. |
| ⚡ **Fast** | Native Rust core. A hundred-file merge finishes before your coffee does. |
| 🪶 **Tiny** | ~15–25 MB installers. No Electron bloat. |
| 💸 **Honest** | Free forever, GPL-3.0. No accounts, no telemetry, no "Pro" tier. |

## Take the tour

**Beacon — AI with your choice of provider.** Chat with citations, semantic search across your library, one-click PII redaction, summaries, study flashcards, glossary, and voice. Use Ollama for on-device AI, or configure an OpenAI-compatible endpoint.

![Beacon: chat, semantic search, PII redaction](docs/screenshots/showcase-ai.png)

**The everyday craft.** Merge a hundred files in a second. Drop a signature or an `APPROVED` stamp on any page. Burn redactions into the content stream. OCR a scanned stack into searchable text.

![Merge, Sign, Redact, Auto-Redact](docs/screenshots/showcase-craft.png)

**Find anything, instantly.** A browsable library across every PDF you've imported, line-level diff between any two documents, and one `⌘K` palette to reach all 65 tools.

![Library, Pages, Command palette](docs/screenshots/showcase-flow.png)

## OLED theme — true black

A fifth appearance option for emissive displays: true-black `#000000` surfaces, so pixels switch off instead of glowing dark grey. Pick it in Settings, or press `⌘K` and type "theme".

| | |
|---|---|
| ![Toolbox in the OLED theme](docs/screenshots/oled-home.png) | ![Command palette switching to the OLED theme](docs/screenshots/oled-palette.png) |
| ![Beacon in the OLED theme](docs/screenshots/oled-beacon.png) | ![Settings with the OLED theme selected](docs/screenshots/oled-settings.png) |

## Glass theme — make it yours

Translucent panels over a softly tinted background, with nine accent colors: Orange, Cobalt, Iris, Emerald, Coral, Teal, Amber, Lime, and Slate. Choose Glass and your accent in Settings.

These screenshots show the current `main` branch. The Glass theme and PDF workflow polish will ship in the next release.

<img src="docs/screenshots/glass-settings.png" alt="Slab Settings with the Glass theme, Teal accent, and all nine accent colors visible" width="800">

<details>
<summary><strong>More PDF workflow screenshots</strong> — Split previews and Merge results</summary>

The compact panels keep page ranges, output filenames, and saved-file actions easy to reach.

| Split — preview every output | Merge — arrange and save |
|---|---|
| <img src="docs/screenshots/split-output-preview.png" alt="Split PDF with three page ranges, output filenames and page counts, and a saved-files confirmation" width="390"> | <img src="docs/screenshots/merge-saved-files.png" alt="Merge PDFs with ordered input files, page selection fields, and a Show in folder action after saving" width="390"> |

</details>

## Hopper — folders that file themselves

Point Hopper at a folder — `~/Downloads`, a scanner, a shared inbox — and attach a recipe. New PDFs get flattened, OCR'd, redacted, watermarked, renamed by Beacon, and filed away, with a live log streaming every run.

Six predicate kinds route each file (filename, regex, text contents, page count, size, catch-all), and a live preview pane shows what each rule would catch against your *actual* recent files. `⇧⌘H` to open it.

Hazel charges $42 and can't read PDFs. Adobe's automation needs an enterprise license and phones home. Hopper is free and local.

## Slab Server — the same engine, in 80 megabytes

Headless Slab: an HTTP API plus a drag-and-drop web UI in a single Docker image. Put it on your NAS, script it, hide it behind a reverse proxy.

```bash
docker run --rm -p 8080:8080 ghcr.io/sanjays2402/slab:latest
```

Open [http://localhost:8080](http://localhost:8080), drop a PDF on the page. Full API + Compose example: [docs/server.md](docs/server.md).

## 65 tools, one palette

<details>
<summary><strong>See every tool</strong> — press <kbd>⌘K</kbd> anywhere in the app to jump to any of them</summary>

| Group | Tools |
|---|---|
| Read & organize | Reader · Library · Search Library · Pages · Pages (list) · Slides · Theater |
| Assemble | Merge · Split (balanced N files, live preview, overlap validation) · Split by Chapter · Insert · N-up · Flatten · Bind (PDF → EPUB) |
| Convert | Convert · Reflow (PDF → Word) · Tabulate (PDF → Excel) · Markdown (PDF → MD/HTML) · Markdown → PDF · Compress · Compact · Streamline (Fast Web View) · Archive (PDF/A) · Press (PDF/X-4) · Grayscale |
| Edit | Edit Text · Crop · Watermark · Header/Footer · Page Labels · Numbers · Bates · Legal Stamp · Metadata |
| Protect | Encrypt · Redact · Auto-Redact · Veil · Sanitize · Sign · Signet · Batch Sign |
| Analyze | OCR · Extract · Tables → CSV · Diff · Compare · Compare 3-way · Repair |
| Beacon AI | Beacon AI · Beacon Search · PII Redact · Citations · Study · Glossary · Voice |
| Automate | Hopper (Watched Folders) · Atelier (Recipes) · Quill Batch · Quill Designer · Quill Auto-Detect |
| Create | Forms · Toolbox · Loom (PDF/UA) · Loupe (PDF/A check) |

</details>

## Also in the box

- **Standalone CLI** — a separate `slab` binary ships in every bundle: `slab autoredact in.pdf out.pdf --preset email,ssn`, no GUI needed.
- **Polyglot** — point Slab at `.docx` `.xlsx` `.pptx` `.epub` `.csv` `.json` `.html` `.rtf` `.odt`, images (EXIF + OCR text), even audio (EXIF + transcription) and get PDFs out.
- **Detachable panels** — pop any panel into its own window. Beacon on the second monitor, two Readers side by side.
- **Vim mode** — modal keybindings (`gg`/`G`/`j`/`k`, counts, `/` search, `:42`) in Reader, Library, and Beacon.
- **A11y & i18n** — built-in accessibility audit, full keyboard control, and a JSON locale system.

## Install

Pre-built installers ship with every [release](https://github.com/Sanjays2402/slab/releases):

| Platform | Files |
|---|---|
| macOS (Apple Silicon + Intel) | `.dmg` |
| Windows | `.msi` + `.exe` |
| Linux | `.deb` + `.AppImage` + `.rpm` |

**First launch on macOS:** the builds are ad-hoc signed (no $99/year Apple Developer ID yet), so Gatekeeper warns once. Right-click **Slab.app** → **Open** → **Open** in the dialog. Every launch after is a normal double-click. Verify the signature yourself with `codesign -dvv /Applications/Slab.app`. To help fund a Developer ID so this prompt goes away for everyone, see [SIGNING.md](SIGNING.md).

## Build from source

Prereqs: Rust stable, Node ≥ 22.12, pnpm ≥ 9.

```bash
git clone https://github.com/Sanjays2402/slab
cd slab
pnpm install
pnpm tauri dev          # run in dev mode
pnpm tauri build        # installer / app bundle for your platform
```

Optional runtime deps for the AI side: [Ollama](https://ollama.com) + `ollama pull llama3.2:3b` for Beacon chat; `whisper-cpp` for voice dictation. The settings panel tells you exactly what's missing.

### Tests

```sh
pnpm check
pnpm test               # TypeScript, split planning, browser PDF helpers, i18n
pnpm exec playwright install chromium
pnpm test:e2e           # navigation and PDF workflows in Chromium
pnpm test:website       # published website, mobile, policies and screenshots
cargo test --locked --lib --bins --features server --manifest-path src-tauri/Cargo.toml
cargo build --locked --features server --bin slab --bin slab-server --manifest-path src-tauri/Cargo.toml
pnpm test:native        # actual CLI and HTTP API PDF round-trips
```

`test:e2e` starts Vite automatically. Native tests generate their own PDFs, use a temporary output folder and start the server on a local ephemeral port. Set `SLAB_CLI` / `SLAB_SERVER` for binaries in another location, `SLAB_SITE_URL` for another website deployment, or `SLAB_CHROMIUM_PATH` for an installed Chromium executable. Native dialog interactions in browser tests use a test bridge; `test:native` exercises the real Rust commands and HTTP server. OCR integrations need Poppler and Tesseract; AI integrations need configured providers.

## Under the hood

```mermaid
flowchart LR
    You --> UI["Slab UI\nSvelte 5 + TypeScript"]
    UI --> Core["Rust core\nlopdf"]
    Core --> Disk[("Your disk")]
    UI <-.-> Beacon["Beacon AI\non-device model"]
    Beacon <-.-> Disk
    Core <-.-> Server["Slab Server\nDocker, 80 MB"]
```

Tauri 2 shell, Svelte 5 front-end, pure-Rust PDF core (`lopdf`), `pdfjs-dist` rendering, Tesseract OCR, local embeddings + on-device chat for Beacon. 730+ Rust tests, clippy-clean, cross-platform CI.

## A small promise

Slab will never ask for an email. Will never call home. Will never gate a feature behind a paywall. If it ever does any of those things, you have my permission to fork it and rip the offending lines out.

Made with 🍰 by [@Sanjays2402](https://github.com/Sanjays2402).
