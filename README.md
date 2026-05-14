# Slab

> A fast, free, offline PDF tool for macOS, Windows, and Linux. Local-first — your files never leave your machine. 🍰

[![Build](https://github.com/Sanjays2402/slab/actions/workflows/build.yml/badge.svg)](https://github.com/Sanjays2402/slab/actions/workflows/build.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

Slab is a no-bullshit PDF tool. No accounts, no uploads, no subscriptions, no "convert online" garbage. You drop in PDFs, you do the thing, you get a file back. Everything runs locally.

## Features

Status as of v0.0.1 — everything else is on the roadmap and being implemented.

| Feature       | v0.0.1 | Status                |
| ------------- | ------ | --------------------- |
| Merge PDFs    | ✅     | drag-to-reorder, save |
| Split         | —      | next                  |
| Reorder pages | —      | next                  |
| Rotate pages  | —      | next                  |
| Delete pages  | —      | next                  |
| Compress      | —      | planned               |
| OCR           | —      | planned (tesseract)   |
| Extract text  | —      | planned               |
| Encrypt       | —      | planned               |
| Watermark     | —      | planned               |
| Edit / Sign   | —      | planned               |

## Stack

- **Shell**: [Tauri 2](https://tauri.app) — ~10 MB binaries, native menus
- **UI**: [SvelteKit](https://svelte.dev) + TypeScript
- **PDF core**: Pure-Rust ([`lopdf`](https://crates.io/crates/lopdf)) for manipulation, [`pdfium-render`](https://crates.io/crates/pdfium-render) on the roadmap for rendering, [`tesseract-rs`](https://crates.io/crates/tesseract-rs) for OCR.
- **License**: GPL-3.0

## Build from source

Prereqs: Rust ≥ 1.75, Node ≥ 20, pnpm ≥ 9.

```bash
git clone https://github.com/Sanjays2402/slab
cd slab
pnpm install
pnpm tauri dev          # run in dev mode
pnpm tauri build        # produce an installer / app bundle
```

## Tests

```bash
cd src-tauri && cargo test
```

## Why?

PDF tools online are uniformly garbage: ads, file-size caps, watermarks, sign-up walls, and quietly logging your documents. Desktop tools are either $200/year (Acrobat) or surprisingly capable but ugly (a half-dozen older Java apps). Slab is what I wish existed.

## License

[GPL-3.0](LICENSE) — free as in freedom. Fork it, ship it, just don't close-source it.
