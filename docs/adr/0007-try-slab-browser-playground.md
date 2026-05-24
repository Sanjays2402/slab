# ADR-0007: try.slab.app — browser playground

## Status
Accepted, 2026-05-24.

## Context
Slab's landing page has a "Download free" CTA but no way to evaluate the
product without installing. Competitors (Smallpdf, iLovePDF) trade evaluation
ease for cloud uploads, which is exactly what Slab markets against. We need a
zero-install evaluation surface that does NOT compromise the privacy wedge.

## Decision
Ship `try.slab.app` — the existing SvelteKit SPA, served as a static bundle on
Cloudflare Pages, with a curated `/try` route that:

- bundles sample PDFs in `static/try/samples/`
- executes all PDF mutation in-browser via `pdf-lib` and `pdfjs-dist`
- wraps every desktop-only panel in `<DownloadWall>` with a deep-link CTA
- shows a persistent "0 bytes uploaded" banner with live network counter
- defaults all telemetry to OFF (opt-in only)

## Rejected alternatives
- **Server-rendered preview**: violates the wedge. Hard no.
- **Tauri-on-WebAssembly**: not production-ready (2026).
- **iframe of a demo VM**: latency too high for trust-building demo.
- **Video demos only**: doesn't beat "click and try it yourself."

## Consequences
- The "web preview" graceful-degradation work shipped this week becomes a
  product surface, not just a no-op.
- We commit to keeping the SvelteKit SPA buildable as a pure web bundle (no
  Tauri-only imports at module top level inside `/try`).
- We accept a ~150 KB `pdf-lib` payload on `/try` only (lazy-loaded, not in
  main).
- `pdf-lib` (Apache-2) and `pdfjs-dist` are already direct dependencies, so
  no new dependency footprint.

## Implementation map
- Routes live under `src/routes/try/` (prerender + ssr false).
- Pure helpers live under `src/lib/try/` and are framework-agnostic.
- Sample PDFs are minted at build time by `scripts/mint-samples.mjs` (no
  binary fixtures in git outside of small generated ones).
- The persistent "0 bytes uploaded" banner uses `PerformanceObserver` on
  `resource` entries to count bytes since page load, filtering out asset
  requests on the same origin (those are app bundles, not user data).

## Privacy guarantees encoded in `/try`

1. No `fetch()` to a non-same-origin endpoint, ever, in any `/try/*` route.
2. No `<script src>` to a third-party origin.
3. No analytics by default. A future "Help improve Slab" toggle (defaulted
   OFF) is the only opt-in path.
4. CSP for `/try` should be tightened in a follow-up to `default-src 'self'`.
