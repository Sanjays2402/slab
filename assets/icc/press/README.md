# Press ICC Profiles

Bundled CMYK output-intent ICC profiles for the **Press** module
(PDF/X-4, ISO 15930-7).

## Profiles

| File | Spec | Bytes | SHA-256 |
|------|------|-------|---------|
| `PSOcoated_v3.icc` | FOGRA51 (PSO Coated v3) — ECI/IDEAlliance | 2,195,228 | `c30ad2c01e8f93135ec7682c535e0a81bc2d177c301e196376c5f5838b5c8e86` |
| `GRACoL2013_CRPC6.icc` | GRACoL2013 CRPC6 (US sheet-fed coated) — IDEAlliance | 3,462,308 | `4ebbfad6bc9cfc033fdafdd8ac5df8159208932cb16d9a6596d349ae7ab50443` |

## Provenance

- **PSOcoated_v3.icc** downloaded from the ICC profile registry
  (https://www.color.org/registry/profiles/PSOcoated_v3.icc) on
  2026-05-23. This is the canonical FOGRA51 characterization profile
  (PSO Coated v3) registered by ECI/Heidelberger Druckmaschinen and
  freely redistributable.
- **GRACoL2013_CRPC6.icc** downloaded from
  https://www.color.org/registry/profiles/GRACoL2013_CRPC6.icc on
  2026-05-23. Registered by IDEAlliance, freely redistributable.

Both are vendored at compile-time via `include_bytes!` so every Slab
build on every platform produces byte-identical PDF/X-4 output intents.

## License

ICC profile data is not copyrightable in most jurisdictions; the
distributing organizations (ECI, IDEAlliance) make these profiles
freely available for any use including bundling in software. The
profile data is treated here as data, not source code.

## Why two profiles

- **FOGRA51** is the European standard for coated offset.
- **GRACoL2013** is the North American sheet-fed equivalent.

Most commercial print shops accept either. Slab defaults to GRACoL2013
when the user's locale starts with `en-US` and FOGRA51 otherwise (see
ADR `docs/adr/2026-05-23-pdfx-conformance-level.md`).
