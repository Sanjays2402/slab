# ADR: PDF/A target conformance is 2b (default), 3b (opt-in)

## Status

Accepted, 2026-05-22.

## Context

ISO 19005 defines four families and three conformance levels per family:

- **PDF/A-1** (2005, PDF 1.4): no transparency, no JBIG2, no embedded files,
  one colour family per document. Easiest to validate, hardest to *produce*
  from arbitrary input — most modern PDFs use transparency.
- **PDF/A-2** (2011, PDF 1.7): allows transparency, JPEG2000, embedded PDF/A
  attachments, OpenType fonts. Sweet spot for real-world conversion.
- **PDF/A-3** (2012, PDF 1.7): like 2 but allows arbitrary file attachments
  (XML invoices etc). Government / e-invoicing target.
- **PDF/A-4** (2020, PDF 2.0): minimal differences for our use case; defer.

Conformance levels:

- **a** (accessible) — requires logical structure (PDF/UA). Out of scope.
- **b** (basic) — visual reproducibility only. Achievable from raster PDFs.
- **u** (unicode) — text Unicode-mappable. Achievable when fonts are embedded
  with ToUnicode maps (already true of every PDF that has selectable text
  in Slab today).

## Decision

- Default target: **PDF/A-2b**. Covers ≥95% of legal/contract/archival
  use cases users actually want.
- Opt-in via radio button: **PDF/A-3b** (when user wants to attach a
  sidecar XML/CSV — ZUGFeRD/Factur-X e-invoicing flows).
- PDF/A-2u attempted opportunistically: if every font has a ToUnicode
  CMap we emit `<rdf:li>2U</rdf:li>` in the XMP. Validator will catch
  mismatches.

## Consequences

- One ICC profile to ship (sRGB v4) — covers RGB and grayscale via the
  PDF default greyscale colourspace.
- Transparency is allowed → no need to flatten before conversion (v2.0.3
  Flatten remains a separate user choice).
- Embedded files are allowed in 2 and 3. PDF/A-2 requires they be
  themselves PDF/A-compliant; we warn but do not reject in 2b mode. We
  validate attachment conformance only when emitting PDF/A-3b.
- PDF/A-1 conversion is explicitly deferred to v3.0.1 — it's a strictly
  smaller superset and we can opportunistically add it once the pipeline
  is in place.

## Alternatives considered

- **Target 1b as default**: rejected. >40% of inputs we have tested
  (annotated contracts, scanned-then-flattened invoices) contain
  transparency or JBIG2 that downconverts poorly. Forcing 1b would
  require flattening + recompression, costing fidelity for ~no gain.
- **Wrap Ghostscript `gs -dPDFA=2`**: rejected. Adds a 35 MB native
  dependency, output is frequently non-compliant (well-known issue),
  and it would break the offline-pure-Rust promise. We do, however,
  opportunistically pipe through `verapdf` *if installed* as a
  third-party sanity check.
