# Marquee — `try.slab.app` interactive playground

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Ship a fully-client-side, no-install interactive Slab playground at `try.slab.app` (and `/try` on the marketing site) — visitors edit a sample PDF in the real Slab UI within 5 seconds, hit a "Download Slab" wall the instant they want a desktop-only feature, and leave wanting to install. Convert the 10 "[desktop-only]" notices Sanjay just shipped into either real working demos or high-intent CTAs.

**Architecture:**
The existing SvelteKit app (`@sveltejs/adapter-static`, SPA fallback) already builds to a static bundle. We add `routes/try/` that bypasses the normal app shell, ships a curated set of WEB-SAFE panels (Reader, Outline, Markdown→PDF, Page tools that can be done in-browser via pdf-lib, Beacon-lite via WebLLM-style demo), and routes every desktop-only action through a single `<DownloadWall>` modal with deep-link CTAs. Sample PDFs are bundled at `static/try/samples/`. Zero backend, zero Tauri, zero analytics-by-default — same privacy promise as the desktop app.

**Tech stack:**
- SvelteKit (existing) + adapter-static
- `pdf-lib@1.17` (already-known tree-shakeable Apache-2 PDF mutation library — merge/split/rotate/insert/remove/metadata all client-side)
- `pdfjs-dist@4` (already used in Reader) — rendering
- File System Access API where supported (`window.showSaveFilePicker`) with Blob-download fallback
- `posthog-js` lite (opt-in only, behind a "Help improve Slab" toggle defaulted OFF — privacy is the wedge)
- No Beacon AI in v1 (Ollama is desktop-only); show a "Beacon AI is desktop-only" wall with a 6-second screen recording

**Buy-Button audit:** 4/4 PASS
1. **Pay-for-it:** Adobe / PDF Expert / Foxit have no free in-browser editor that respects privacy. (Smallpdf / iLovePDF send files to their servers.) `try.slab.app` becomes the only zero-upload PDF playground on the web.
2. **Notice-it:** Returning user sees a "Try Slab in your browser" CTA on the landing page that didn't exist yesterday.
3. **Pick-us:** Visitor lands → manipulates a PDF in 5 seconds → realizes the desktop app is the same UI with more power → downloads. This IS the funnel.
4. **Tell-a-friend:** "You can edit a PDF without uploading it anywhere — here, look" → URL share, Twitter screenshot. Beats every competitor's marketing site.

**WOW moment:** A persistent banner at the bottom: `📄 Your file never left this browser tab. Watch the network panel.` with a live counter `0 bytes uploaded since you opened this page`. Reset on every file load. Screenshot bait.

**Out of scope (deferred to Marquee-II / v3.1.x):**
- Beacon AI demo via WebLLM (separate plan — needs WebGPU detection + 1GB model download UX)
- Plugin / Foundry marketplace browsing
- Account / saved-session sync
- Server-rendered preview (we will stay 100% client-side; that's the whole point)

---

## Pipeline position

Sanjay shipped 10 web-preview-graceful commits Sat-Sun (`improve/*` branches) — every panel now degrades cleanly when `isInTauri()` is false. This plan is the immediate next step: **promote that graceful degradation into a marketing surface**. Lands in parallel to v3.0.0 Bedrock (which continues on `feature/v3.0.0-bedrock-pdfa`). Branch: `feature/v3.19.0-marquee-try`.

After Marquee:
- v3.1.1 — Beacon-Lite via WebLLM (gated by WebGPU detect)
- v3.2.0 — `try.slab.app` -> `app.slab.app` (same surface, but PWA + offline + SW caching)

---

## Slice map (10 slices, ~1800 net LOC, 12 commits)

| # | Slice | LOC | Commits | Buy-button |
|---|-------|-----|---------|------------|
| 0 | Pre-flight ADR + routing scaffold | 80 | 1 | infra |
| 1 | `/try` route + minimal shell + sample PDF loader | 220 | 2 | Notice-it |
| 2 | `<DownloadWall>` modal + deep-link CTA system | 180 | 1 | Pick-us |
| 3 | Reader (read-only) in /try with bundled sample | 160 | 1 | Tell-a-friend |
| 4 | Merge / Split / Rotate / Remove pages — pdf-lib backed | 320 | 2 | Pay-for-it |
| 5 | Metadata edit (read+write title/author/subject/keywords) | 140 | 1 | Pay-for-it |
| 6 | Markdown→PDF client-side via existing markdown→pdf code | 180 | 1 | Tell-a-friend |
| 7 | "0 bytes uploaded" privacy banner + telemetry-off-by-default | 100 | 1 | WOW |
| 8 | Landing page integration (`docs/landing/index.html` + `/try` CTA hero) | 120 | 1 | Pick-us |
| 9 | Release pipeline — Vercel/Cloudflare deploy + `try.slab.app` CNAME doc | 110 | 1 | Notice-it |

**Total:** ~1610 net LOC + ~400 test LOC. 12 commits. Every slice independently buy-button-positive.

---

## Slice 0 — Pre-flight ADR + scaffold

### Task 0.1: Write ADR for browser-only Slab surface

**Files:**
- Create: `docs/adr/0007-try-slab-browser-playground.md`

**Step 1: Write ADR**

```markdown
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
- defaults all telemetry to OFF (opt-in)

## Rejected alternatives
- **Server-rendered preview**: violates the wedge. Hard no.
- **Tauri-on-WebAssembly**: not production-ready (2026).
- **iframe of a demo VM**: latency too high for trust-building demo.
- **Video demos only**: doesn't beat "click and try it yourself."

## Consequences
- The "web preview" graceful-degradation work Sanjay shipped this week becomes a
  product surface, not just a no-op.
- We commit to keeping the SvelteKit SPA buildable as a pure web bundle (no
  Tauri-only imports at module top level).
- We accept a ~150 KB pdf-lib payload on /try only (lazy-loaded, not in main).
```

**Step 2: Commit**

```bash
git checkout -b feature/v3.19.0-marquee-try
git add docs/adr/0007-try-slab-browser-playground.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "docs(adr): try.slab.app browser playground (ADR-0007)"
```

---

## Slice 1 — `/try` route + sample loader

### Task 1.1: Add `pdf-lib` dependency

**Files:**
- Modify: `package.json`

**Step 1: Add dep**

```bash
pnpm add pdf-lib@^1.17.1
```

**Step 2: Verify lockfile**

```bash
pnpm install --frozen-lockfile
```
Expected: exits 0.

**Step 3: Commit**

```bash
git add package.json pnpm-lock.yaml
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "chore(deps): add pdf-lib 1.17 for browser-only PDF mutation"
```

### Task 1.2: Create `/try` route with sample-loader shell

**Files:**
- Create: `src/routes/try/+layout.svelte` (~40 LOC — strips main app chrome)
- Create: `src/routes/try/+layout.ts` (`export const prerender = true; export const ssr = false;`)
- Create: `src/routes/try/+page.svelte` (~100 LOC — landing for /try)
- Create: `src/lib/try/samples.ts` (sample registry; ~40 LOC)
- Create: `static/try/samples/employment-offer.pdf` (placeholder; AI: `python3 -c "..." | head` to mint a real 2-page sample)
- Create: `static/try/samples/scanned-invoice.pdf`
- Create: `static/try/samples/multi-chapter-report.pdf`

**Step 1: Failing test**

```ts
// src/lib/try/samples.test.ts
import { describe, it, expect } from 'vitest';
import { SAMPLES, loadSample } from './samples';

describe('samples registry', () => {
  it('exposes at least 3 samples', () => {
    expect(SAMPLES.length).toBeGreaterThanOrEqual(3);
  });
  it('each sample has slug + label + path + pages', () => {
    for (const s of SAMPLES) {
      expect(s.slug).toMatch(/^[a-z0-9-]+$/);
      expect(s.label).toBeTruthy();
      expect(s.path).toMatch(/^\/try\/samples\/.+\.pdf$/);
      expect(s.pages).toBeGreaterThan(0);
    }
  });
});
```

**Step 2: Run, expect fail**

```bash
pnpm vitest run src/lib/try/samples.test.ts
```
Expected: FAIL "cannot find module './samples'"

**Step 3: Implement `samples.ts`**

```ts
// src/lib/try/samples.ts
export interface Sample {
  slug: string;
  label: string;
  path: string;
  pages: number;
  description: string;
}

export const SAMPLES: Sample[] = [
  {
    slug: 'employment-offer',
    label: 'Employment offer letter (2 pp)',
    path: '/try/samples/employment-offer.pdf',
    pages: 2,
    description: 'Try filling, signing, or redacting.',
  },
  {
    slug: 'scanned-invoice',
    label: 'Scanned invoice (1 p, image-only)',
    path: '/try/samples/scanned-invoice.pdf',
    pages: 1,
    description: 'See what OCR could pull out (desktop only).',
  },
  {
    slug: 'multi-chapter-report',
    label: 'Multi-chapter report (24 pp)',
    path: '/try/samples/multi-chapter-report.pdf',
    pages: 24,
    description: 'Try splitting by chapter or extracting pages.',
  },
];

export async function loadSample(slug: string): Promise<Uint8Array> {
  const sample = SAMPLES.find((s) => s.slug === slug);
  if (!sample) throw new Error(`unknown sample: ${slug}`);
  const res = await fetch(sample.path);
  if (!res.ok) throw new Error(`failed to load ${sample.path}`);
  return new Uint8Array(await res.arrayBuffer());
}
```

**Step 4: Implement `+layout.svelte` + `+layout.ts` + `+page.svelte`**

(Full code in execution; ~100 LOC of grid of sample cards + "Or drop your own PDF" zone.)

**Step 5: Mint 3 sample PDFs**

```bash
# Use pdf-lib via a one-off node script at scripts/mint-samples.mjs
node scripts/mint-samples.mjs
# emits static/try/samples/*.pdf
```

**Step 6: Run tests + check**

```bash
pnpm vitest run src/lib/try/samples.test.ts
pnpm check
```
Expected: all green.

**Step 7: Commit**

```bash
git add src/routes/try src/lib/try static/try scripts/mint-samples.mjs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "feat(try): /try route + sample PDF loader + 3 bundled samples"
```

---

## Slice 2 — `<DownloadWall>` modal + deep-link CTA system

### Task 2.1: Component + per-feature copy

**Files:**
- Create: `src/lib/try/DownloadWall.svelte` (~120 LOC)
- Create: `src/lib/try/wallCopy.ts` (~60 LOC — per-feature wall copy: OCR, Sign, Beacon AI, Redact, Bates, Compress, etc.)
- Create: `src/lib/try/DownloadWall.test.ts`

**Step 1: Failing test**

```ts
import { render, screen } from '@testing-library/svelte';
import DownloadWall from './DownloadWall.svelte';
import { describe, it, expect } from 'vitest';

describe('DownloadWall', () => {
  it('renders the feature-specific headline', () => {
    render(DownloadWall, { props: { feature: 'ocr', open: true } });
    expect(screen.getByText(/OCR runs offline in Slab/i)).toBeInTheDocument();
  });
  it('exposes download links per platform', () => {
    render(DownloadWall, { props: { feature: 'sign', open: true } });
    expect(screen.getByRole('link', { name: /macOS/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Windows/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Linux/i })).toBeInTheDocument();
  });
});
```

**Step 2-4: Implement + verify + commit.**

Each wall copy block: 1-line headline, 2-line value prop, 3 platform CTAs, 1 "Why not in browser?" link to a help doc, all in Liquid Glass styling matching the main app.

```bash
git -c ... commit -m "feat(try): DownloadWall modal + per-feature CTA copy (12 features)"
```

---

## Slice 3 — Reader (read-only) in /try

### Task 3.1: Mount existing ReaderPanel in /try with web-safe surface

**Files:**
- Modify: `src/routes/try/+page.svelte`
- Create: `src/routes/try/reader/+page.svelte` (~80 LOC)
- Modify: `src/lib/panels/ReaderPanel.svelte:1-50` — ensure it accepts a `bytes: Uint8Array` prop instead of requiring a Tauri-loaded file (refactor; ~30 LOC delta)

**Steps 1-5:** TDD as above. Sample-card click → loads sample bytes → renders Reader full-page → keyboard shortcuts work → highlight/select copies text. Toolbar buttons that need desktop (Save, Print to file, Add bookmark to library) open DownloadWall.

```bash
git -c ... commit -m "feat(try): Reader in browser with bundled samples — read-only end-to-end"
```

---

## Slice 4 — Page operations (merge / split / rotate / remove) via pdf-lib

### Task 4.1: `src/lib/try/pdfOps.ts` — pure functions over pdf-lib

**Files:**
- Create: `src/lib/try/pdfOps.ts` (~150 LOC)
- Create: `src/lib/try/pdfOps.test.ts` (~120 LOC; load fixture, mutate, re-parse, assert)

```ts
import { PDFDocument, degrees } from 'pdf-lib';

export async function rotatePages(bytes: Uint8Array, indices: number[], deg: 90|180|270): Promise<Uint8Array> {
  const doc = await PDFDocument.load(bytes);
  for (const i of indices) {
    const page = doc.getPage(i);
    const cur = page.getRotation().angle;
    page.setRotation(degrees((cur + deg) % 360));
  }
  return await doc.save();
}

export async function removePages(bytes: Uint8Array, indices: number[]): Promise<Uint8Array> {
  const doc = await PDFDocument.load(bytes);
  const sorted = [...indices].sort((a,b) => b - a);
  for (const i of sorted) doc.removePage(i);
  return await doc.save();
}

export async function mergeFiles(files: Uint8Array[]): Promise<Uint8Array> {
  const out = await PDFDocument.create();
  for (const f of files) {
    const doc = await PDFDocument.load(f);
    const pages = await out.copyPages(doc, doc.getPageIndices());
    for (const p of pages) out.addPage(p);
  }
  return await out.save();
}

export async function splitAt(bytes: Uint8Array, splits: number[]): Promise<Uint8Array[]> {
  // splits = [3, 7] on a 10-page doc → 3 PDFs: pages [0..2], [3..6], [7..9]
  // (full impl in execution)
}
```

**Step 5: Commit**

```bash
git -c ... commit -m "feat(try): pdf-lib-backed page ops (merge/split/rotate/remove) + 14 tests"
```

### Task 4.2: Wire into a `<PagesPanel>` for /try

**Files:**
- Create: `src/routes/try/pages/+page.svelte` (~180 LOC)

Page-grid view (thumbnails via pdfjs), shift+click multi-select, R/Del keys, drag-reorder, "Save as PDF" → File System Access API → fallback to Blob download. End-to-end: open sample → reorder + rotate + remove pages → save to disk → re-open in same tab → changes persist (proves it actually works).

```bash
git -c ... commit -m "feat(try): /try/pages — drag-reorder + rotate + remove, save to disk"
```

---

## Slice 5 — Metadata edit

### Task 5.1: Read + write document metadata via pdf-lib

**Files:**
- Create: `src/lib/try/metadata.ts` + test (~80 LOC + 60 LOC tests)
- Create: `src/routes/try/metadata/+page.svelte` (~60 LOC)

Title, author, subject, keywords, producer, creator. Live preview of "Properties" panel. Save → download.

```bash
git -c ... commit -m "feat(try): /try/metadata — edit title/author/subject/keywords in browser"
```

---

## Slice 6 — Markdown → PDF in browser

### Task 6.1: Lift existing markdown→PDF logic into a web-safe module

**Files:**
- Inspect: `src/lib/panels/MarkdownPanel.svelte` (already in repo)
- Create: `src/lib/try/mdToPdf.ts` (~120 LOC — uses `marked` + pdf-lib `drawText` with a bundled font)
- Create: `src/routes/try/markdown/+page.svelte` (~60 LOC — split-pane editor on left, live PDF preview on right)

Use `pdf-lib` `StandardFonts.Helvetica` for v1 (no font embedding complexity). Headings → larger font + bold. Paragraphs → wrap. Bullets → indented. Page breaks on overflow.

```bash
git -c ... commit -m "feat(try): /try/markdown — live md→pdf split-pane preview"
```

---

## Slice 7 — Privacy banner + telemetry-off-by-default

### Task 7.1: `<PrivacyBanner>` with live "0 bytes uploaded" counter

**Files:**
- Create: `src/lib/try/PrivacyBanner.svelte` (~80 LOC)
- Modify: `src/routes/try/+layout.svelte` — include banner at bottom

```ts
// Inside the component:
let bytesUploaded = $state(0);
// Patch window.fetch on mount to count outbound bytes to non-static origins.
// Same-origin fetches of /try/samples/* don't count.
// XHR + WebSocket likewise patched.
```

Display: `📄 Your file never left this browser tab. 0 bytes uploaded since you opened this page.` The counter is also a tooltip with the methodology link.

**Step 2: WOW polish**

A 320ms cubic-bezier(0.34, 1.56, 0.64, 1) fade-in on first interaction. If `bytesUploaded > 0` ever ticks (would only happen if we add telemetry), the banner turns amber and explains what was sent.

**Step 3: Telemetry default-off**

- Create: `src/lib/try/telemetry.ts` — opt-in only; no events fire unless `localStorage.slab_try_telemetry === 'on'`.
- Add a Settings popover in /try with the toggle, default OFF, with explanatory copy.

```bash
git -c ... commit -m "feat(try): privacy banner (0 bytes uploaded) + telemetry default-off"
```

This is the **WOW** for this release. `LAST_WOW_TICK_AT` will be updated to this commit's timestamp.

---

## Slice 8 — Landing-page integration

### Task 8.1: Add a "Try in browser, no install" hero CTA + dedicated section

**Files:**
- Modify: `docs/landing/index.html:24-40` — add a third primary CTA `<a class="primary" href="https://try.slab.app">Try in your browser</a>`
- Modify: `docs/landing/index.html` — add a new `<section id="try">` after `<section class="hero">` with 3 mini-demos as cards (rotate pages, edit metadata, md→pdf) each linking to the relevant `/try/<route>`
- Modify: `docs/landing/styles.css` — match Liquid Glass styling

```bash
git -c ... commit -m "feat(landing): try-in-browser CTA + 3-card playground teaser section"
```

---

## Slice 9 — Release pipeline

### Task 9.1: Cloudflare Pages deploy config + DNS doc

**Files:**
- Create: `.cloudflare/pages.toml` (or `wrangler.toml`) with build command `pnpm build`, output dir `build/`
- Create: `docs/ops/try-slab-deploy.md` — DNS + CNAME instructions for `try.slab.app`
- Modify: `.github/workflows/deploy-try.yml` (NEW) — on push to main, deploy to Cloudflare Pages
- Modify: `docs/release-notes/v3.19.0.md` — marketing release notes

The workflow uses the standard `cloudflare/wrangler-action@v3` with secrets `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` (Sanjay must add these via repo settings — flag in the release notes).

**Sanjay TODO list (surfaced in deliver message):**
1. Add Cloudflare Pages project named `slab-try` pointing at this repo
2. Add `CLOUDFLARE_API_TOKEN` + `CLOUDFLARE_ACCOUNT_ID` to repo secrets
3. CNAME `try.slab.app` → `slab-try.pages.dev`

```bash
git -c ... commit -m "ops(try): Cloudflare Pages workflow + DNS guide for try.slab.app"
```

### Task 9.2: Release notes + tag

```bash
git -c ... commit -m "docs(release): v3.19.0 — Marquee, browser playground"
```

After merge to main:

```bash
git tag v3.19.0
git push origin main --follow-tags
```

CI uploads desktop artifacts; Cloudflare workflow deploys `try.slab.app`. `gh release create v3.19.0` finalizes.

---

## Quality gates (every slice)

```bash
cd src-tauri && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --lib
cd .. && pnpm check && pnpm vitest run
```

The Marquee branch touches almost no Rust (only Slice 3's potential ReaderPanel prop change might cascade to a Tauri command signature — unlikely). Cargo gates should be trivially green. Frontend gates are the meaningful ones.

---

## Final review checklist

- [x] Each task ≤ 2-5 min
- [x] Exact file paths
- [x] Complete code (this plan has skeleton; subagents fill in full per slice)
- [x] Commands with expected output
- [x] DRY (pdf-lib used everywhere, no duplication)
- [x] YAGNI (no WebGPU/AI in v1, no plugin marketplace, no auth)
- [x] TDD (each slice with failing test → code → pass)
- [x] Frequent commits (12 commits over 10 slices)
- [x] Buy-button passes (4/4)
- [x] WOW moment included (privacy banner in Slice 7)
- [x] Quality gates spelled out
- [x] Release pipeline defined

---

## Execution handoff

Plan saved. Ready to execute via `subagent-driven-development` — one subagent per slice with full context, two-stage review (spec compliance + code quality), proceed only on both passes. Estimated 8-10 ticks to complete from Slice 0 through Slice 9 merge + release.

**Recommended sequencing:** Slices 0-2 in tick 1 (scaffold + DownloadWall — together they're the foundation). Slices 3-4 in tick 2 (Reader + page ops — first real demo). Slices 5-7 across ticks 3-4. Slice 8-9 in tick 5 (landing + deploy).

This plan is intentionally smaller-version (v3.19.0 — not v4.0.0) because it sits orthogonal to the Bedrock backend pipeline and ships independently. v3.0.0 Bedrock continues unblocked on its own branch.
