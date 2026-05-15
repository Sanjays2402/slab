# Slab 🍰

> **The PDF toolkit your files never leave.**
> A fast, free, fully offline PDF app for macOS, Windows, and Linux.

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

Every other PDF tool wants you to upload your taxes, your contracts, your medical records to a server you've never heard of, then watch a 30‑second ad, then pay $9.99 to remove the watermark. Slab does the opposite: **everything runs locally on your machine.** No accounts. No uploads. No subscriptions. No nonsense.

## What it does

A real Adobe‑Acrobat‑replacing toolkit, built one feature at a time and shipped honestly:

| Feature | Status |
| --- | --- |
| **Merge PDFs** — drag‑to‑reorder, save anywhere | ✅ shipping |
| **Split** — by page range or every N pages | ✅ shipping |
| **Page editor** — rotate, delete, reorder | ✅ shipping |
| **Compress** — lossless re‑stream, shows bytes saved | ✅ shipping |
| **Extract text** — page‑by‑page preview, copy or save .txt | ✅ shipping |
| **Encrypt / decrypt** — password‑protect any PDF (RC4‑40) | ✅ shipping |
| **Watermark** — text, any angle, opacity, gray, live preview | ✅ shipping |
| **OCR** — make scans searchable (Tesseract) | 🗺️ on the roadmap |
| **PDF ↔ images** — export to PNG/JPG, build from images | 🗺️ on the roadmap |
| **Edit · Annotate · Redact · Sign · Fill forms** | 🗺️ on the roadmap |
| **⌘K command palette + CLI mode** | 🗺️ on the roadmap |

Seven full features shipping, 26 Rust tests green, AES encryption and OCR queued for the next round.

## Why Slab?

- **Local‑first.** Your documents never touch a server. Air‑gap a laptop and Slab still works.
- **Fast.** Native Rust under the hood. Merging a hundred files takes a second, not a minute.
- **Tiny.** The installer is under 10 MB. Compare that to anything else in the category.
- **Honest.** Free forever. GPL‑3.0. No "Pro tier." No telemetry. No upsells.
- **Beautiful.** A dark‑first interface that doesn't look like a 2008 toolbar exploded.

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
cd src-tauri && cargo test
```

## Under the hood

- **Shell:** [Tauri 2](https://tauri.app) — system webview, ~10 MB binaries, native menus.
- **UI:** [SvelteKit](https://svelte.dev) + TypeScript.
- **PDF core:** [`lopdf`](https://crates.io/crates/lopdf) (pure Rust) for manipulation, [`pdfium-render`](https://crates.io/crates/pdfium-render) for rendering (incoming), [`tesseract-rs`](https://crates.io/crates/tesseract-rs) for OCR (incoming).
- **License:** GPL‑3.0 — free as in freedom. Fork it, ship it, just don't close‑source it.

## A small promise

Slab will never ask for an email. Will never call home. Will never gate a feature behind a paywall. If it ever does any of those things, you have my permission to fork it and rip the offending lines out.

Made with 🍰 by [@Sanjays2402](https://github.com/Sanjays2402).
