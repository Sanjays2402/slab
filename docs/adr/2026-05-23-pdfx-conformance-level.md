# ADR: PDF/X conformance level for the Press module

**Date:** 2026-05-23
**Status:** Accepted
**Module:** `src-tauri/src/pdf/press`
**Version target:** v3.8.0 "Press"

## Context

Slab's enterprise-PDF wedge needs a print-production export. Commercial
print shops require **PDF/X** — a constrained subset of PDF defined by
ISO 15930 that guarantees fonts are embedded, colour is unambiguous, and
no interactive features survive prepress.

There are multiple ISO 15930 parts in active use:

| Part | Codename | Base PDF | Allows transparency | Allows ICC RGB | Adoption (2025) |
|------|----------|----------|---------------------|----------------|-----------------|
| 15930-1 | X-1a:2001 | PDF 1.3 | No | No (CMYK + spot only) | Legacy; still required by some offset houses |
| 15930-7 | X-4:2010 | PDF 1.6 | **Yes** | **Yes** | Dominant since ~2015; most print shops accept |
| 15930-8 | X-6:2020 | PDF 2.0 | Yes | Yes | <2% real-world adoption |

## Decision

**Slab v3.8.0 ships PDF/X-4 (ISO 15930-7) only.**

### Why X-4 (not X-1a)

- Modern files routinely contain transparency (drop shadows, soft masks,
  modern InDesign exports). X-1a forbids transparency, forcing flattening
  which is destructive and slow.
- X-4 allows ICC-tagged RGB images alongside CMYK, matching how design
  studios actually work.
- Every major print shop's RIP since 2012 accepts X-4. Pre-2012 X-1a
  workflows are rare.

### Why not X-6

- PDF 2.0 base; <2% real-world adoption. Adobe Acrobat doesn't even
  preview X-6 reliably. Premature.

### Why not "X-4p" (referenced profile)

- "p" variant references an external ICC profile by URL instead of
  embedding it. Saves bytes but adds a network dependency the print shop
  may not have. Always embed in v3.8.0; defer "-p" to v3.8.1 if anyone
  asks.

## Output-intent defaults

Slab bundles two output intent ICC profiles and picks based on locale:

```ts
const defaultIntent =
  navigator.language?.startsWith("en-US") ? "gracol2013" : "fogra51";
```

- **FOGRA51** (PSO Coated v3) — European coated offset.
- **GRACoL2013 CRPC6** — North American sheet-fed coated.

User can override in the Convert tab. Custom profile upload is **deferred
to v3.8.1**.

## Conformance scope

The Slab validator implements the **auto-decidable subset** of ISO 15930-7
— 32 rules grouped by category (structural, output intent, XMP, fonts,
colour, geometry). A PDF that passes Slab's validator will be accepted by
every commercial RIP we tested against (Heidelberg Prinect, Kodak
Prinergy, Esko ArtPro+). Hand-decidable rules (e.g. "ink coverage ≤ 320%")
are documented as warnings, not enforced.

## Consequences

- The `pdf::press::OutputIntent` enum carries exactly two variants.
- The Tauri command surface is three calls: `slab_press_inspect`,
  `slab_press_convert`, `slab_press_validate`.
- Profile data ships in `assets/icc/press/` (vendored at compile time).
- See implementation plan: `docs/plans/2026-05-23-v3.8.0-press-pdf-x.md`.

## References

- ISO 15930-7:2010 — Graphic technology — PDF/X-4
- ICC profile registry — https://www.color.org/registry/
- ECI offset profiles — https://www.eci.org/
- IDEAlliance GRACoL — https://www.idealliance.org/
