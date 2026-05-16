# Slab 🍰

> **The PDF toolkit your files never leave.**
> A fast, free, fully offline PDF app for macOS, Windows, and Linux.

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Rust tests](https://img.shields.io/badge/rust%20tests-56%20passing-brightgreen.svg)](src-tauri)
[![Latest release](https://img.shields.io/github/v/release/Sanjays2402/slab?label=release)](https://github.com/Sanjays2402/slab/releases/latest)

Every other PDF tool wants you to upload your taxes, your contracts, your medical records to a server you've never heard of, then watch a 30‑second ad, then pay $9.99 to remove the watermark. Slab does the opposite: **everything runs locally on your machine.** No accounts. No uploads. No subscriptions. No nonsense.

![Slab Reader with the PDF 1.7 spec open](docs/screenshots/00-hero-reader.png)

## What it does

**Twenty‑one tools shipping in v0.7.0 "Slab Workshop"** — a real Adobe‑Acrobat‑replacing toolkit, built one feature at a time and shipped honestly:

| Tool | What it does | Status |
| --- | --- | --- |
| **Reader** | Open any PDF, thumbnails, find, outline, zoom | ✅ shipping |
| **Merge** | Drag‑to‑reorder, save anywhere | ✅ shipping |
| **Split** | By page range or every N pages | ✅ shipping |
| **Pages** | Rotate, delete, reorder | ✅ shipping |
| **Compress** | Lossless re‑stream, shows bytes saved | ✅ shipping |
| **Extract** | Page‑by‑page text preview, copy or save .txt | ✅ shipping |
| **Encrypt / Decrypt** | Password‑protect any PDF (RC4‑40) | ✅ shipping |
| **Watermark** | Text, any angle, opacity, gray, live preview | ✅ shipping |
| **Convert** | PDF ↔ PNG/JPG/WebP (both directions) | ✅ shipping |
| **Metadata** | View, edit, or strip every identifying field | ✅ shipping |
| **Page Numbers** | Templates, 6 positions, custom start, skip cover | ✅ shipping |
| **Sign & Stamp** | Drop signature / logo / approval image on any page | ✅ shipping |
| **Crop** | Trim margins with percentage edges, optional MediaBox resize | ✅ shipping |
| **Insert Pages** | Splice blank pages (A4/Letter/Legal) or another PDF at any index | ✅ shipping |
| **Headers & Footers** | Templated text bands with `{n}/{total}/{date}/{filename}` | ✅ shipping |
| **Redact** | Paint solid black rectangles over sensitive regions | ✅ shipping |
| **N‑up** | Compose 2/4/6/9 pages onto a single sheet for printing | ✅ shipping |
| **Markdown → PDF** | Convert Markdown into a clean, tiny PDF — no font embedding | ✅ **NEW in v0.7.0** |
| **Grayscale** | RGB/CMYK → gray inside content streams, vector‑true, BT.601 luminance | ✅ **NEW in v0.7.0** |
| **Page Labels** | Roman, arabic, alpha, prefix labels per range — sets `/PageLabels` | ✅ **NEW in v0.7.0** |
| **Auto‑Redact** | Find & cover emails / SSNs / phones / cards / custom regex | ✅ **NEW in v0.7.0** |
| **Standalone CLI** | A separate `slab` binary — every op from the terminal | ✅ **NEW in v0.7.0** |
| **OCR** | Make scans searchable (Tesseract) | 🗺️ next |

**56 Rust tests passing**, clippy‑clean with `-D warnings`, type‑checked Svelte 5 front‑end. OCR queued for v0.8.0 "Compass."

## Why Slab?

- **Local‑first.** Your documents never touch a server. Air‑gap a laptop and Slab still works.
- **Fast.** Native Rust under the hood. Merging a hundred files takes a second, not a minute.
- **Tiny.** The installer is under 10 MB. Compare that to anything else in the category.
- **Honest.** Free forever. GPL‑3.0. No "Pro tier." No telemetry. No upsells.
- **Beautiful.** A dark‑first interface that doesn't look like a 2008 toolbar exploded.

## The tools, one by one

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

### Metadata — *new in v0.5.0*
View every Info‑dictionary field (Title, Author, Subject, Keywords, Creator, Producer), edit them, or hit one button to strip everything (plus XMP) for a truly anonymous PDF.

![Metadata](docs/screenshots/10-metadata.png)

### Page Numbers — *new in v0.5.0*
Stamp page numbers with a template like `Page {n} of {total}`, in any of 6 positions, with custom font size, gray level, starting number, and a "skip first N pages" option for covers.

![Page numbers](docs/screenshots/11-numbers.png)

### Sign & Stamp — *new in v0.5.0*
Drop a signature scan, company logo, or `APPROVED` stamp onto any page. Position, scale, and opacity all live‑tweakable.

![Sign & Stamp](docs/screenshots/12-sign.png)

### Crop — *new in v0.6.0*
Trim margins by percentage from each edge. Optionally rewrite the MediaBox so downstream tools see the new size, not just a clipped view.

![Crop](docs/screenshots/18-crop.png)

### Insert Pages — *new in v0.6.0*
Splice blank A4/Letter/Legal pages or pages from another PDF at any 1‑indexed position. Insert before or after, in bulk.

![Insert](docs/screenshots/19-insert.png)

### Headers & Footers — *new in v0.6.0*
Stamp templated text bands across every page. `{n}`, `{total}`, `{date}`, `{filename}` tokens, six anchor positions, custom font size + opacity.

![Header / Footer](docs/screenshots/20-headerfooter.png)

### Redact — *new in v0.6.0*
Paint solid black rectangles over sensitive regions. Page‑by‑page, drag to draw, burned into the content stream — no scrubbing reveals the original.

![Redact](docs/screenshots/21-redact.png)

### N‑up — *new in v0.6.0*
Compose 2, 4, 6, or 9 pages onto a single sheet for printing — landscape or portrait, configurable spacing, exact reproductions of each source page.

![N-up](docs/screenshots/17-nup.png)

### Markdown → PDF — *new in v0.7.0*
Write or paste Markdown, click Convert, get a clean PDF. Headings, **bold**, *italic*, `code`, lists, blockquotes, code blocks, horizontal rules — all rendered with standard Helvetica. No font embedding means tiny output files (~1 KB per page).

![Markdown → PDF](docs/screenshots/13-markdown.png)

### Grayscale — *new in v0.7.0*
Convert RGB and CMYK fills and strokes to gray inside PDF content streams. Vector‑true — no rasterization — using ITU‑R BT.601 luminance. Range‑selectable. Embedded raster images are unchanged in this pass.

![Grayscale](docs/screenshots/14-grayscale.png)

### Page Labels — *new in v0.7.0*
Control how PDF readers display page numbers: roman numerals for front matter, arabic for the body, custom prefixes for chapters. Multiple ranges in one shot, live preview of exactly what each style produces. Sets the catalog's `/PageLabels` number tree per the PDF spec.

![Page Labels](docs/screenshots/15-labels.png)

### Auto‑Redact — *new in v0.7.0*
Find and cover sensitive content automatically. Built‑in presets for **emails**, **US SSNs**, **phone numbers**, and **credit cards**. Add your own regex patterns. Adjustable bar color. Line‑level bounding boxes drawn over each match.

![Auto-Redact](docs/screenshots/16-autoredact.png)

### Standalone CLI — *new in v0.7.0*
A separate `slab` binary ships in every bundle alongside the GUI. All 21 ops available from the terminal — no Tauri runtime, no IPC, direct library calls.

```bash
slab md2pdf input.md output.pdf --page-size Letter
slab grayscale input.pdf output.pdf
slab autoredact input.pdf output.pdf --preset email,ssn
slab info report.pdf
```

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

If you'd like to help fund a Developer ID certificate (so this prompt goes away for everyone), see [SIGNING.md](SIGNING.md). The CI is already wired to switch to full Developer ID signing + notarization the moment six GitHub secrets are configured.

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
cd src-tauri && cargo test          # 56 tests
cargo clippy --all-targets -- -D warnings
cd .. && pnpm exec svelte-check     # type-check the UI
```

## Under the hood

- **Shell:** [Tauri 2](https://tauri.app) — system webview, ~10 MB binaries, native menus.
- **UI:** [SvelteKit](https://svelte.dev) + Svelte 5 runes + TypeScript.
- **PDF core:** [`lopdf`](https://crates.io/crates/lopdf) (pure Rust) for manipulation, [`pdfjs-dist`](https://www.npmjs.com/package/pdfjs-dist) for rendering in the Reader, [`pdf-lib`](https://pdf-lib.js.org) for client‑side composition (stamps, image embedding), [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) for the Markdown → PDF tool, [`pdfium-render`](https://crates.io/crates/pdfium-render) + [`tesseract-rs`](https://crates.io/crates/tesseract-rs) queued for OCR.
- **License:** GPL‑3.0 — free as in freedom. Fork it, ship it, just don't close‑source it.

## A small promise

Slab will never ask for an email. Will never call home. Will never gate a feature behind a paywall. If it ever does any of those things, you have my permission to fork it and rip the offending lines out.

Made with 🍰 by [@Sanjays2402](https://github.com/Sanjays2402).
