# Slab v3.18.0 — Bind

**Drop a research PDF → read it on your Kindle on the train tonight.
Offline. Free. No cloud.**

Adobe Acrobat doesn't ship EPUB export at all. PDF Expert and Foxit don't
either. Calibre's PDF→EPUB is a notoriously janky 2008-era GUI. So we
built one — first-class, free, local, on every platform.

## ✨ What's new

- **Bind panel** — drop a PDF, type a title and author, get a reflowable
  EPUB 3 file that Apple Books, Kindle Previewer, Kobo, and Calibre open
  natively. 13 e-reader languages, sane metadata defaults from the PDF
  itself.
- **Chapter detection** — Bind splits on H1 headings by default. A
  research paper becomes one chapter per section, not one giant text
  wall. Toggle off for single-chapter mode.
- **Semantic XHTML5 + EPUB 3 spec compliance** — real `<h1>`, `<p>`,
  `<ul>`, `<ol>`, `<table>` markup with an embedded reflow-friendly
  stylesheet. mimetype-first / Stored compression / nav.xhtml / OPF — it
  passes spec checkers, not a "best effort" ZIP.
- **Atelier `convert-to-epub` step** — drop a folder of papers into a
  batch recipe and walk away with a bookshelf.
- **Cmd/Ctrl+K → "Convert PDF to EPUB"** — palette discoverable in two
  keystrokes, with rich keywords (kindle, kobo, calibre, reflowable,
  ebook).
- **Detachable Bind window** — pop the panel into its own native window
  for split-screen workflows.

## 📚 The expanding conversion shelf

Slab now exports to: **DOCX** (Reflow), **XLSX** (Tabulate), **PPTX**
(Slide), **Markdown** + **HTML** (Markdown), and now **EPUB 3** (Bind).
One offline pipeline, one Block IR, zero cloud, zero token limits.

## 🏗️ Quality

- 1,588 unit + integration tests still green (+21 new EPUB-specific
  tests covering chapter splitting, package XML, XHTML emission,
  ZIP-store mimetype, and end-to-end roundtrip)
- spec-compliant mimetype-first / Stored-compression EPUB containers
- end-to-end roundtrip verified: a real PDF in, a real EPUB out, opens
  in Calibre clean
- self-rolled UUID v4 generator — no new dependency added

## 🔐 The wedge

- Free, forever.
- No telemetry. No upload. No subscription.
- Same UX on macOS, Windows, Linux.

Read the paper, then read the paper — on your couch.
