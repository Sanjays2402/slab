# Flatten

Slab's **Flatten** action makes a PDF un-editable. It's the same operation
Adobe Acrobat Pro charges $239/yr for; Slab does it offline, free.

## Two modes

### 1. Burn annotations only (default)

- Bakes every highlight, comment, signature, and form field into the page
- Removes `/Annots` arrays and the document-level `/AcroForm` dictionary
- **Text stays searchable** — selection, copy, and Beacon Q&A still work
- Fast: well under a second on typical contracts

Use when: you want to lock down annotations and form fields but keep a
searchable, machine-readable document.

### 2. Full raster (legal-grade)

- First burns annotations (Stage A)
- Then re-renders every page at 150 or 300 DPI via Poppler (`pdftoppm`)
- Replaces each page's content stream with a single Image XObject `Do`
- Drops every `/Font` resource entry — **zero editable text remains**
- Slow: ~1s per page at 150 DPI on a modern laptop

Use when: legal, compliance, or archival workflows that require a
court-admissible fully-flat PDF. Common in litigation discovery, FDA
submissions, ISO archives, M&A data rooms.

> ⚠ Raster mode is **irreversible**. Save as a new file. Text becomes
> pixels — searchability and selection are lost.

## CLI

```bash
slab flatten input.pdf -o output.pdf                    # annotations only
slab flatten input.pdf --raster -o output.pdf           # 150 DPI raster
slab flatten input.pdf --raster --dpi 300 -o output.pdf # 300 DPI archival
slab flatten input.pdf --no-widgets -o output.pdf       # skip form widgets
```

DPI is clamped to the 36–600 range. Default is 150 (US Letter at 1275×1650 px).

## Requirements

The raster path shells out to `pdftoppm` from Poppler. macOS:
`brew install poppler`. Debian/Ubuntu: `apt install poppler-utils`. The
annotation-only path has no external dependencies — it's pure Rust.

## What survives raster mode

- The PDF page count, page geometry, and visual appearance (at the chosen DPI)
- Document metadata (`/Info`, XMP) unless explicitly stripped via `slab sanitize`

## What does *not* survive raster mode

- All text glyphs (selection, copy, full-text search)
- All vector graphics
- All annotations, form widgets, links, bookmarks pointing into pages
- All embedded fonts (the output has zero `/Font` dictionaries)

## Why it matters

Adobe Acrobat Pro ships flatten behind a $239/yr subscription. PDF Expert
$79/yr; Foxit $129/yr. The open-source alternatives (`pdftk`, `qpdf`)
handle annotation flatten but don't ship a raster path — you have to chain
`pdftoppm`, `img2pdf`, and a metadata stripper together. Slab ships both
modes in one button, fully offline, no telemetry.

## Closes

GitHub issue [#24](https://github.com/Sanjays2402/slab/issues/24)
