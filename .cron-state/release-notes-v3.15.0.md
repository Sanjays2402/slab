# Slab v3.15.0 — Tabulate 📊

**Turn any PDF into a typed Excel workbook — offline, free, and batchable.**

Adobe Acrobat Pro charges $239/yr for cloud-only PDF → Excel export. PDF Expert doesn't ship the feature. Foxit doesn't support Linux. Slab does it all locally, with one click — or 10,000 clicks at a time via Atelier batch recipes.

## What's new

### 📊 Tabulate panel — PDF → .xlsx in one click

Drop a PDF, get a workbook. Each page becomes a sheet. Tables are detected with the same column-clustering pipeline that powers Reflow's PDF → Word export. Cell values are **typed**:

- **Numbers** — US (`1,234.56`) and EU (`1.234,56`) formats both recognized; Excel sees real `t="n"` cells you can sum.
- **Dates** — ISO (`2026-05-24`), US (`05/24/2026`), EU (`24/05/2026`), and long-month (`May 24, 2026`) formats detected.
- **Text** — everything else, escaped safely.

Three toggles control the heuristics; defaults are tuned for paralegal-style invoice / statement work.

### 🧰 Slot Tabulate into any Atelier recipe

Atelier (the batch automation panel) now has a **Convert to Excel** step. Build a recipe like _"OCR (English) → Auto-redact SSN → Convert to Excel"_ and run it across a folder. The output extension flips from `.pdf` to `.xlsx` automatically.

### Under the hood

- New `pdf::tabulate` module (374-line OOXML SpreadsheetML writer, ZIP-packaged via the `zip` crate already in tree).
- Reuses Reflow's `extract_text_runs` + `layout::layout_page` + `Block::TableRow` grouping — minimal new code, maximum correctness.
- 21 new unit tests for cell typing, table extract, ZIP validity, sheet-name sanitization.
- All cell rendering is XML-safe (escape pass before write).

## Quality

- 1528 Rust unit tests passing
- 0 svelte-check errors
- `cargo clippy -D warnings` clean
- Cross-platform: macOS (arm64 + x64), Linux (deb + AppImage), Windows (MSI + NSIS)

## Get it

Pick your platform from the assets below. Offline-first, your files never leave your machine.

— Cake 🍰
