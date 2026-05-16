# Slab 🍰

> **The PDF toolkit your files never leave.**
> A fast, free, fully offline PDF app for macOS, Windows, and Linux.

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Rust tests](https://img.shields.io/badge/rust%20tests-42%20passing-brightgreen.svg)](src-tauri)

Every other PDF tool wants you to upload your taxes, your contracts, your medical records to a server you've never heard of, then watch a 30‑second ad, then pay $9.99 to remove the watermark. Slab does the opposite: **everything runs locally on your machine.** No accounts. No uploads. No subscriptions. No nonsense.

![Slab Reader with the PDF 1.7 spec open](docs/screenshots/00-hero-reader.png)

## What it does

Eleven full tools in v0.5.0 "Slab Studio" plus **five new tools in v0.6.0 "Slab Forge"** — sixteen tools, a real Adobe‑Acrobat‑replacing toolkit, built one feature at a time and shipped honestly:

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
| **Crop** | Trim margins with percentage edges, optional MediaBox resize | ✅ **NEW in v0.6.0** |
| **Insert Pages** | Splice blank pages (A4/Letter/Legal) or another PDF at any index | ✅ **NEW in v0.6.0** |
| **Headers & Footers** | Templated text bands with `{n}/{total}/{date}/{filename}` | ✅ **NEW in v0.6.0** |
| **Redact** | Paint solid black rectangles over sensitive regions | ✅ **NEW in v0.6.0** |
| **N‑up** | Compose 2/4/6/9 pages onto a single sheet for printing | ✅ **NEW in v0.6.0** |
| **OCR** | Make scans searchable (Tesseract) | 🗺️ next |

**42 Rust tests passing**, clippy‑clean with `-D warnings`, type‑checked Svelte 5 front‑end. OCR queued for v0.7.0.

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

## Install

Pre‑built installers ship with each [release](https://github.com/Sanjays2402/slab/releases): `.dmg` (macOS Apple Silicon + Intel), `.msi` (Windows), `.deb` + `.AppImage` (Linux).

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
cd src-tauri && cargo test          # 31 tests
cargo clippy --all-targets -- -D warnings
cd .. && pnpm exec svelte-check     # type-check the UI
```

## Under the hood

- **Shell:** [Tauri 2](https://tauri.app) — system webview, ~10 MB binaries, native menus.
- **UI:** [SvelteKit](https://svelte.dev) + Svelte 5 runes + TypeScript.
- **PDF core:** [`lopdf`](https://crates.io/crates/lopdf) (pure Rust) for manipulation, [`pdfjs-dist`](https://www.npmjs.com/package/pdfjs-dist) for rendering in the Reader, [`pdf-lib`](https://pdf-lib.js.org) for client‑side composition (stamps, image embedding), [`pdfium-render`](https://crates.io/crates/pdfium-render) + [`tesseract-rs`](https://crates.io/crates/tesseract-rs) queued for OCR.
- **License:** GPL‑3.0 — free as in freedom. Fork it, ship it, just don't close‑source it.

## A small promise

Slab will never ask for an email. Will never call home. Will never gate a feature behind a paywall. If it ever does any of those things, you have my permission to fork it and rip the offending lines out.

Made with 🍰 by [@Sanjays2402](https://github.com/Sanjays2402).
