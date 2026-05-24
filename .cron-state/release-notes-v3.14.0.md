# v3.14.0 — Reflow 📝

> **Convert any PDF to Word — offline, free, forever.**

Adobe Acrobat Pro charges $239/year for one of its flagship features:
"Export PDF to Word." It runs in the cloud, ships your files to Adobe's
servers, and locks the result behind a subscription. Slab does it on
your laptop — no upload, no signup, no recurring bill — and on every
operating system you use.

This release ships the **Reflow** engine: a from-scratch, pure-Rust
PDF→DOCX converter wired into Slab's Reflow panel and into the Atelier
batch workflow so a paralegal can drop a folder of 200 contracts and
walk away with 200 editable Word documents.

## What you can do now

### Drag-and-drop conversion
Open the new **Reflow (PDF → Word)** panel from the sidebar. Drag a PDF
onto the dropzone. Two seconds later, a real `.docx` opens cleanly in
Microsoft Word, Apple Pages, or LibreOffice — paragraphs, headings,
bullet lists and tables preserved.

### Batch in Atelier
The Atelier recipe palette now has a **📝 Convert to Word** card. Drag
it to the end of any recipe (after OCR, redaction, or compaction) and
every PDF in your input folder is converted in parallel. Output files
land in your outbox as `.docx`, not `.pdf` — paralegals open them
straight in Word.

### Smart layout reconstruction
Reflow extracts positioned text runs from the PDF content stream and
clusters them into Word-native blocks:

- **Paragraphs** with original font sizes and spacing
- **Headings** auto-classified by font-size ratio (configurable)
- **Bullet & numbered lists** with proper Word list styles
- **Tables** detected via column-x clustering and emitted as real
  Word tables (not images, not pre-formatted text)

### Zero new dependencies
The whole pipeline is hand-rolled OOXML — no Apache POI, no Java, no
C libraries. Stays on your machine, ships in the same lightweight
installer.

## Why it matters

| Tool                     | Price/yr | Offline? | Cross-platform? | Batch? |
|--------------------------|---------:|:--------:|:---------------:|:------:|
| Adobe Acrobat Pro        | $239     | ❌       | macOS / Win      | ❌    |
| PDF Expert Pro           | $79      | ❌       | macOS only       | ❌    |
| Foxit PhantomPDF         | $129     | ❌       | macOS / Win      | ❌    |
| **Slab v3.14.0 Reflow**  | **$0**   | ✅       | mac / win / linux | ✅   |

If you're a legal team converting briefs for redlining, a research lab
exporting figures from old PDF papers, or anyone who's ever pasted PDF
text into Word and watched the formatting disintegrate — this is the
release that fixes it for free.

## Technical highlights

- **6 new modules** in `pdf::reflow` — extract, layout, tables, docx,
  types, errors (~2,053 lines of new Rust + 25 unit tests)
- **OOXML writer** is a hand-rolled `quick-xml` emitter producing 5
  built-in styles: Normal, Heading1-3, ListBullet, ListNumber,
  TableNormal — Word opens the output identically to Acrobat's export
- **Atelier wiring** — `Step::ConvertToDocx` is a first-class terminal
  step. Recipe output extension auto-flips from `.pdf` to `.docx` so
  batch outputs are immediately editable
- **TDD throughout** — 14 unit tests over the algorithmic core,
  3 integration tests over the end-to-end pipeline, 4 tests over
  Atelier integration (single + chained + batch)

## What ships in the installers

- macOS arm64 + x64 DMG
- Linux x64 deb + AppImage
- Windows MSI + NSIS

Same six platform binaries you've come to expect, now ~1.2 MB larger
to carry the OOXML writer.

## Up next

- **v3.15.0 — Image reflow:** extract embedded PDF images and place
  them in the resulting Word document
- **v3.16.0 — Column-aware reflow:** newspaper-style multi-column
  layouts auto-merged into linear flow

---

**Drop in, drag a PDF, get a Word doc.** Free. Offline. Forever.
