# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🚀 v3.19.0 Marquee Slice 6 (Markdown→PDF) SHIPPED (2026-05-24 06:25 PT)

**TICK 2026-05-24 06:17 PT (Sunday off-hours)** — MODE C, finished Slice 6 of v3.19.0.

### What shipped this tick
Branch `feature/v3.19.0-marquee-try`, 3 commits, ~1243 LOC:

- `05055b7` feat(try): mdToPdf — pure-function markdown to PDF (StandardFonts, no deps)
  - 440 LOC `src/lib/try/mdToPdf.ts` — block + inline lexer + pdf-lib layout
  - 130 LOC `scripts/test-mdtopdf.mjs` — 17 assertions, all green
  - Zero new dependencies (uses existing `pdf-lib`)
- `900c2a5` feat(try): /try/markdown — split-pane live md→pdf preview with Cmd-S download
  - 472 LOC `src/routes/try/markdown/+page.svelte`
  - Live pdfjs preview, 400ms debounce, Cmd-S download, Cmd-B/I shortcuts
  - Responsive (stacks below 880px)
- `fb82738` feat(try): surface /try/markdown on the playground landing + md-extras wall
  - New "Or jump straight into a tool" section on /try with 3 cards
  - New `md-extras` wallCopy key (images / fonts / footnotes / math → desktop)

### Buy-Button verdict — 4/4 PASS
- **Pay-for-it ✅** — Smallpdf and iLovePDF charge $7-$12/mo for browser
  Markdown→PDF *and they upload your file*. Slab does it free, zero upload.
- **Notice-it ✅** — Returning /try visitor sees 3 tool cards that didn't
  exist before, plus a Markdown editor with live preview.
- **Pick-us ✅** — Smallpdf/iLovePDF require account + upload. No competitor
  ships in-browser md→PDF that respects privacy.
- **Tell-a-friend ✅** — "Type markdown, watch a PDF render live, hit Cmd-S,
  it never leaves your tab." Screenshot bait.

### Plan also committed (in this tick)
- `docs/plans/2026-05-24-v3.19.0-marquee-finish.md` — finish-Marquee plan
  (Slice 6 = this tick; Slice 9 = next tick: deploy pipeline + tag release).

### Quality gates
- `pnpm check` ✅ 0 errors, 62 warnings (baseline preserved)
- `node --experimental-strip-types scripts/test-mdtopdf.mjs` ✅ all green (17 assertions)
- `node --experimental-strip-types scripts/test-pdfops.mjs` ✅ 17/17 green
- Rust gates skipped (no Rust changes)

### Marquee progress
Slices done: 0, 1, 2, 4, 5, 6, 7, 8.
Slices remaining: 3 (in-browser Reader — nice-to-have), 9 (deploy pipeline + tag release — MUST do next).

### LAST_WOW_TICK_AT: 2026-05-24T13:08Z (set previous tick — Marquee privacy banner)
This tick is a buyer-magnet ship (md→PDF live), but not strictly a new
wow surface vs the privacy banner. Counter stays as-is.

### Active branch: feature/v3.19.0-marquee-try
- Pushed in previous tick + this tick will push.
- Next tick: Slice 9 — write `.github/workflows/deploy-try.yml`,
  `docs/ops/try-slab-deploy.md`, release notes, then merge to main +
  tag v3.19.0 + MODE B finalize.

### RECENTLY_CLOSED_ISSUES
- v3.18.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.18.0
- v3.19.0 in progress on feature/v3.19.0-marquee-try

---

## PRIOR STATUS: v3.18.0 Bind PUBLISHED + v3.19.0 Marquee Slices 0-5,8 shipped (2026-05-24 06:08 PT)

(See prior tick log + git history for v3.18.0 Bind details — Tauri
slab_bind_to_epub command + BindPanel + Atelier ConvertToEpub step,
all green, 1588 tests passing. Marquee Slices 0-5 + 7-8 shipped in
the 05:51 PT tick, see `.cron-state/sessions/2026-05-24-0551.md`.)


> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🎉 v3.18.0 "Bind" SHIPPED — merged + tagged, CI running (2026-05-24 05:38 PT)

**TICK 2026-05-24 05:25 PT (Sunday off-hours)** — MODE C → MODE A → RELEASE_PENDING.

Tasks 7-10 of the v3.18.0 Bind plan all executed in one tick. Feature
branch finalised, merged to main, tag pushed, CI kicked off.

### What shipped this tick (5 commits on feature/v3.18.0-bind → merged)

- `092734f` feat(epub): Tauri slab_bind_to_epub + Atelier ConvertToEpub step
- `32f5af6` feat(epub): BindPanel + nav entry + command palette + detached window
- `edd5f4e` chore(release): bump v3.17.0 -> v3.18.0 'Bind' + release notes
- `d855a02` chore(epub): allow clippy::too_many_arguments on slab_bind_to_epub
- `aad86bf` Merge v3.18.0 'Bind' — offline PDF to EPUB 3  ← on main

End-to-end working capability shipped:

1. Backend (last tick): `pdf::epub::convert_to_epub()` produces valid
   EPUB 3 ZIPs (mimetype-first, Stored compression, OPF, nav.xhtml,
   XHTML5 chapters).
2. Tauri command: `slab_bind_to_epub(input, output, opts)` registered
   in generate_handler — frontend can invoke it now.
3. Atelier batch: `Step::ConvertToEpub { detect_tables, detect_lists,
   split_on_h1, language }` — drop a folder of PDFs in a recipe, get
   a folder of EPUBs out.
4. Frontend: `BindPanel.svelte` (13-language dropdown, title/author
   metadata, dropzone, full result stats), wired into:
   - side nav as `bind` with 📖 icon
   - DETACHABLE_PANELS (pop into native window)
   - +page.svelte active + detached routes
   - Command palette: "Convert PDF to EPUB" with keywords for kindle/
     kobo/calibre/reflowable/ebook
5. Marketing release notes + README competitive-matrix table
   (Slab vs Acrobat / PDF Expert / Foxit / Calibre — only Slab and
   Calibre ship PDF→EPUB at all, and Calibre's UI is a 2008 GUI).

### Quality gates (this tick) — ALL GREEN

- `cargo fmt --all --check` ✅
- `cargo clippy --lib --all-targets -- -D warnings` ✅
- `cargo test --lib` ✅ **1588 passed, 0 failed**
- `pnpm check` ✅ 0 errors, 62 (pre-existing) warnings

### Ship size

- 5 commits on feature branch (4 substantive + 1 clippy fix)
- Across both ticks: 10 commits, ~1,430 net LOC
- This tick alone: +662 LOC (panel 522, recipe/run/lib.rs wires 70+,
  release notes + README 70+)

### RELEASE_PENDING: v3.18.0

- Merge SHA: `aad86bf`
- Tag: `v3.18.0`
- CI runs in_progress at tick end:
  - build (main): `26361458975`
  - Docker slab-server (v3.18.0 tag): `26361459009`
  - pages: `26361458765`
- Next tick: MODE B FINALIZE — poll CI, download artifacts, publish
  GitHub release with the 6 best artifacts (mac arm64/x64 dmg, linux
  deb + AppImage, win msi + nsis), verify Docker on GHCR, attach the
  release notes from `.cron-state/release-notes-v3.18.0.md`.

### Buy-Button verdict — all 4 PASS

- **Pay-for-it ✅** — Adobe doesn't ship EPUB. Foxit doesn't. PDF
  Expert doesn't. Calibre does but the UI is from 2008. A $49 power
  user reading research on Kindle would pay.
- **Notice-it ✅** — New nav entry, new palette command, new wedge
  feature visible immediately on next launch.
- **Pick-us ✅** — Closes the only category where Slab competitors
  literally have nothing.
- **Tell-a-friend ✅** — "Drop a PDF, get a Kindle book, offline, free."
  Screenshot bait.

### LAST_WOW_TICK_AT: 2026-05-24T12:38Z (v3.18.0 Bind merge)

Fresh wow this tick (offline PDF→EPUB UI). Next 24h timer reset.

### Next tick (after MODE B finalize) — v3.19.0 "Marquee"

A sibling cron promoted `docs/plans/2026-05-24-v3.19.0-marquee-try.md`
during this tick (note: was originally numbered v3.1.0, sibling
renamed to v3.19.0 to avoid collision with Bind). MODE C will start
on `feature/v3.19.0-marquee-try` next time, after Bind is finalised.

---

## PRIOR STATUS: 🚀 v3.18.0 "Bind" — backend end-to-end on feature branch (2026-05-24 04:55 PT)

(See git history for prior tick log — Tasks 1-6 of the Bind plan, backend
landed on feature branch with 1588 tests green and the convert_to_epub()
pipeline functional end-to-end.)
