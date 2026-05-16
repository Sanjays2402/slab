# Markitdown — Research Note (2026-05-16)

## TL;DR
Microsoft's [`markitdown`](https://github.com/microsoft/markitdown) is a tiny Python CLI/library that converts ~12 file formats into Markdown. It's MIT-licensed, requires Python 3.10+, and we'll use it via subprocess — exactly the way `pdf::ocr` shells out to `tesseract` + `pdftoppm`.

## Why it fits Slab v0.8.1 "Polyglot"
v0.8.0 already ships a pure-Rust `md2pdf::render` (pulldown-cmark → lopdf, base-14 fonts, no native deps). Polyglot is just a one-stage funnel in front of it:

```
input file (.docx/.xlsx/.pptx/.html/.epub/.csv/.json/.xml/img/audio)
        │ markitdown subprocess
        ▼
    markdown (stdout)
        │ md2pdf::render
        ▼
       PDF
```

That gives us ~12 new input formats for free, with zero new Rust dependencies and no bundled Python — users install `markitdown` once on their system, same model as `tesseract` in v0.8.

## Facts

| | |
|---|---|
| Latest PyPI version | `0.1.5` |
| License | MIT (Microsoft Corporation) |
| Min Python | 3.10 |
| Install (recommended) | `pipx install 'markitdown[all]'` |
| Install (minimal) | `pip install 'markitdown[docx,xlsx,pptx]'` |
| Source | https://github.com/microsoft/markitdown |

### Supported formats (per upstream README)
PDF, PowerPoint, Word, Excel (xlsx/xls), Images (EXIF + OCR), Audio (EXIF + transcription), HTML, Text-based (CSV, JSON, XML), ZIP (recurses), YouTube URLs, EPub, RSS/Outlook.

### CLI shape
```bash
markitdown path-to-file.docx               # → stdout
markitdown path-to-file.docx -o out.md     # → file
cat file.docx | markitdown                 # → stdout (stdin)
```
Exit code `0` on success; nonzero with stderr message on error. Output is plain UTF-8 Markdown.

### Optional install extras (we recommend `[all]` for users)
`[pptx]` `[docx]` `[xlsx]` `[xls]` `[pdf]` `[outlook]` `[az-doc-intel]` `[audio-transcription]` `[youtube-transcription]`

## Slab integration decisions

1. **Subprocess, not embed.** Markitdown is Python only; binding from Rust is infeasible and the dep tree (mammoth, openpyxl, pdfplumber, pydub, …) is heavy. Subprocess matches our OCR pattern and keeps the Rust binary tiny.
2. **No bundling.** Users `pipx install 'markitdown[all]'` themselves. We surface a friendly hint via `require_markitdown()`, mirroring `require_binary("tesseract")`.
3. **Stdout capture, not `-o`.** We capture the markdown string in memory and pipe straight into `md2pdf::render`. Avoids a temp file and keeps the conversion one round-trip.
4. **Extension allow-list, not magic sniffing.** We dispatch by file extension (`.docx`, `.xlsx`, `.pptx`, `.html`, `.htm`, `.epub`, `.csv`, `.json`, `.xml`, `.rtf`, `.odt`, image/audio variants). If markitdown can handle something we forgot, the CLI's `--force` flag (later) lets users bypass the allow-list. Skip PDF on purpose — round-tripping PDF→MD→PDF would silently degrade content.
5. **Error mapping.**
   - markitdown binary missing → `PdfError::Other("markitdown not found on PATH …")` with install hint.
   - Subprocess exits nonzero → `PdfError::Other("markitdown failed: <stderr trimmed>")`.
   - Unsupported extension → `PdfError::Other("unsupported polyglot input: .xyz")`.
   - Empty markdown output → `PdfError::Other("markitdown returned empty document")`.
6. **Audio/image fidelity caveat (document loudly).** Audio gets EXIF + transcription; images get EXIF + OCR. We're a layer above markitdown's quirks — users see exactly what markitdown produces, plus our base-14-font md2pdf render. No silent magic.

## Out of scope for v0.8.1
- Azure Document Intelligence pathway (paid, requires keys — irrelevant for offline app).
- YouTube URLs (network input — Slab is offline-first; we only accept local files).
- ZIP recursion (separate UX problem; could land in a later release if asked).
- Round-trip PDF→MD→PDF (refuse; users get a confused product).

## Quality bar
- All Rust changes pass `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib`.
- Live `markitdown` integration tests are gated behind `polyglot_available()` so dev machines without the binary don't break the build.
- No new Rust dependencies — we already have `tempfile` and `std::process::Command`.
