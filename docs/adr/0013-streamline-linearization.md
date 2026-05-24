# ADR 0013 — Streamline: PDF Linearization (Fast Web View)

**Status:** Accepted — 2026-05-23
**Version:** v3.13.0 "Streamline"

## Context

When a PDF is served over HTTP, a naive reader has to download the entire file
before it can render page 1, because the cross-reference table sits at the end
of the file. PDF 1.4 §F defines a *linearized* (a.k.a. "Fast Web View") layout:
the linearization parameter dictionary is the first indirect object, followed
by the first-page object subtree, a primary hint stream, and then the rest of
the document. A streaming reader (Acrobat Reader 9+, pdf.js 4+, Foxit Reader
11+) can render page 1 after fetching only the prefix — typically a few hundred
KB regardless of total file size.

Acrobat Pro ($239/yr) sells this under "Optimize for Fast Web View." Acrobat
Standard ($179/yr) does NOT include it. PDF Expert ($79/yr) doesn't have it at
all. Foxit Phantom ($129/yr) has it. Every web publisher and DMS vendor
(SharePoint, Box, M-Files) cares.

Slab ships it free, offline, batched (via Atelier recipes), in one click.

## Decision

We implement a **pure-Rust** linearizer on top of `lopdf` and `flate2` (both
already in deps). No `qpdf` shellout — that would break our cross-platform
parity and zero-cloud privacy story, and adds a 50 MB native dep.

### Spec subset we DO implement (v1)

- Linearization parameter dictionary as object 1:
  - `/Linearized 1`
  - `/L` (file length)
  - `/H` (primary hint stream offset + length, [off len])
  - `/O` (first-page object number)
  - `/E` (end-of-first-page byte offset)
  - `/N` (page count)
  - `/T` (offset of first entry in main xref)
- Primary hint stream with two hint tables:
  - **Page-offset hint table** — per-page byte offsets + object counts
  - **Object-number hint table** — first-page reachable object IDs
- Classic cross-reference table (no xref streams) — broad reader compat.

### Spec subset we DO NOT implement (v1)

- Cross-reference streams (we force classic xref).
- Encrypted PDFs (rejected with `PdfError::Other("encrypted input not supported in v1")`).
  Follow-up v3.13.1 to add decrypt → linearize → re-encrypt round-trip.
- PDF 2.0 hint table extensions.
- Optional hint tables (shared-object, thumbnail, outline) — those are optional
  per spec §F.4.5 and not required for readers to accept the file as linearized.

### Reader compatibility target

- Adobe Acrobat Reader 9+ (2008)
- pdf.js 4+ (2024+)
- Foxit Reader 11+ (2021+)
- Apple Preview (macOS 12+) — graceful fallback (accepts non-linearized files).

### Validation strategy

- Round-trip every linearized output through `lopdf::Document::load` to ensure
  the file remains a valid PDF.
- External smoke test: `qpdf --check <out.pdf>` in CI (already installed for
  v3.12.0 Atelier batch tests) — non-zero exit means we broke spec compliance.
- Snapshot test the linearization param dict for a known fixture.

### Out of scope

- Byte-for-byte parity with `qpdf --linearize`. We only need readers to accept
  the output as linearized. Different object orderings are fine as long as the
  first-page reachable set really does come before the hint stream.

## Consequences

**Pros:**
- One of the last "Acrobat Pro only" gating features Slab is missing.
- Composable with Atelier recipes (`linearize` is a zero-arg step) — batch a
  folder of 500 PDFs to Fast Web View in one drop.
- Adds a measurable, demoable wow: "first-page-visible-at: 188 KB / 12.3 MB"
  before/after panel.

**Cons:**
- ~1.2 k LOC of new code; hint-table layout is fiddly.
- We choose not to be `qpdf`-byte-perfect; some pedantic validators may flag
  cosmetic differences. We accept this — readers don't care.
- Encrypted-PDF support deferred — surfaces a clear error, will land in v3.13.1.
