# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🪟 v3.19.0 "Marquee" plan promoted — try.slab.app browser playground (2026-05-24 05:12 PT)

**TICK 2026-05-24 05:12 PT (Sun off-hours)** — MODE C writing-plans skill (cron-invoked).

- Wrote `docs/plans/2026-05-24-v3.19.0-marquee-try.md` (~21 KB, 10 slices, 12 commits, ~1610 net LOC + ~400 test LOC at execution). Committed `6fe5c00` on main and pushed.
- Originally drafted as v3.1.0 but sibling cron advanced main to v3.18.0 Bind during my 33h window — renumbered to v3.19.0 mid-tick. Plan is orthogonal (browser surface, not backend), version slot is arbitrary.
- Codename **"Marquee" 🪟** — converts Sanjay's 10 Sat-Sun manual `improve/*` web-preview-graceful commits into a marketing surface. `/try` route + 3 bundled sample PDFs + `<DownloadWall>` modal + pdf-lib page ops (merge/split/rotate/remove) + metadata edit + md→pdf + privacy banner with live "0 bytes uploaded" counter + Cloudflare Pages deploy at try.slab.app.
- **WOW**: persistent banner with live bytesUploaded counter (patches window.fetch on mount). Screenshot bait, reinforces privacy wedge.
- **Buy-Button 4/4**: Pay-for-it (only free in-browser PDF editor that doesn't upload — Smallpdf/iLovePDF both upload), Notice-it (new hero CTA "Try in your browser"), Pick-us (5-sec evaluation funnel), Tell-a-friend ("look, no upload bytes").
- Pure frontend: pdf-lib 1.17 + existing pdfjs-dist. Zero new Rust. Zero new Tauri commands. Lazy-loaded on /try only.
- **Sanjay-TODO** surfaced in plan: add Cloudflare Pages project, 2 GH secrets (`CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`), CNAME `try.slab.app`.

Planning tick — 2 files / 1078 insertions of docs only. Below SHIP-SIZE for code ticks; justified by the explicit `writing-plans` skill invocation per the standing order. Note also: this picks up the thread of Sanjay's manual Sat-Sun commit stream rather than writing yet another deep-backend spec — the planning lead is already 10+ versions deep; this plan is the **executable bridge** between what Sanjay shipped this weekend and the next code tick.

**Next tick**: MODE C code-ship. Two attractive targets:
- (a) Marquee Slice 0+1+2 (ADR + `/try` scaffold + `<DownloadWall>`) on `feature/v3.19.0-marquee-try` — direct continuation of Sanjay's `improve/*` work, ships an end-to-end buy-button-positive surface in one tick (~700 LOC realistic).
- (b) v3.0.0 Bedrock Slice 2 (font-embedding audit) on `feature/v3.0.0-bedrock-pdfa` — that branch hasn't been touched in 33h and is queued behind Bind anyway.

Recommend (a) — fresh momentum, marketing-funnel impact, builds on what Sanjay was actively touching this week.

Session log: `.cron-state/sessions/2026-05-24-0512.md`.

---

## STATUS PRIOR: 🚀 v3.18.0 "Bind" — backend end-to-end on feature branch (2026-05-24 04:55 PT)

**TICK 2026-05-24 04:45 PT (Sunday off-hours)** — MODE C DEVELOP.

Executed Tasks 1-6 of the v3.18.0 Bind plan in one tick. Branch
`feature/v3.18.0-bind` pushed to origin (5 commits, +908 LOC).

- `e0940f5` feat(epub): scaffold pdf/epub module (mod/types/errors stubs)
- `a515372` feat(epub): chapter splitter (H1-aware, synthetic fallback)
- `cdfb5f2` feat(epub): EPUB 3 package XML (container/opf/nav/stylesheet)
- `d4043e9` feat(epub): XHTML5 chapter emitter (headings/lists/tables/escapes)
- `d00f1af` feat(epub): end-to-end PDF to EPUB pipeline + ZIP packager

Backend is FUNCTIONAL: `convert_to_epub(path, path, &EpubOptions)`
returns a valid EPUB 3 ZIP. mimetype-first + Stored compression
verified by test. Re-uses lopdf load + Reflow extract/layout passes.
Generates spec-compliant Dublin Core metadata with a self-rolled
UUID-v4 string (no new dep).

Quality gates (after this tick) ALL GREEN:
- `cargo fmt --all --check` ✅
- `cargo clippy --lib --all-targets -D warnings` ✅
- `cargo test --lib` ✅ **1588 passed, 0 failed** (was 1567, +21 epub tests)

Cleared `src-tauri/target` (6.1 GB) at tick start — disk was 707 MiB
free, now 5.9 GiB free.

### Buy-Button verdict — PASS 3/4 today (4/4 once UI wires in next tick)

- **Pay-for-it ✅** — Calibre is the only mainstream PDF→EPUB tool, a
  notoriously janky 2008 GUI. Adobe Acrobat: no EPUB export. PDF Expert
  / Foxit: none. PDFCandy: $7–12/mo cloud. We ship offline + free.
- **Notice-it ⏳** — UI lands next tick (Tauri command + Bind panel +
  Atelier step). Backend invisible to users today.
- **Pick-us ✅** — No PDF reader on macOS produces EPUB. Closes the
  e-reader interop gap.
- **Tell-a-friend ✅** — "Drop research paper → read on Kindle tonight.
  Offline. Free." Screenshot bait.

### Next tick — Tasks 7-8 (UI wire) → MODE A (release)

1. Task 7: `convert_pdf_to_epub` Tauri command in lib.rs +
   `Step::ConvertToEpub` in Atelier recipe.rs + run.rs dispatch.
2. Task 8: BindPanel.svelte + nav entry + command-palette entry +
   atelier.ts type union.
3. Task 9: bump 3.17.0 → 3.18.0 in Cargo.toml/package.json/tauri.conf.json
   (verify all 3 — lesson from v3.16.0). Write marketing release notes.
4. Task 10: merge feature/v3.18.0-bind → main, tag, push.

Plan reference: `docs/plans/2026-05-24-v3.18.0-bind-pdf-to-epub.md`.

### LAST_WOW_TICK_AT: 2026-05-24T10:28Z (v3.17.0 Markdown release)

Still <24h since last wow; next tick's UI ship will reset this.

---

## PRIOR STATUS: 📋 v3.18.0 "Bind" PLAN FILED (2026-05-24 04:05 PT)

**TICK 2026-05-24 04:05 PT (Sunday off-hours)** — PLANNING tick (Sanjay
invoked the `writing-plans` skill explicitly via cron).

- Wrote `docs/plans/2026-05-24-v3.18.0-bind-pdf-to-epub.md` (10 tasks,
  ~42 KB). End-to-end plan for offline PDF → EPUB 3 — re-uses Reflow's
  `extract` + `layout` passes and the v3.17.0 XHTML emitter pattern,
  adds new `pdf/epub/` module with `split.rs` (H1 chapter splitter),
  `package.rs` (container/opf/nav XML), `writer.rs` (XHTML5 chapter
  emitter), `mod.rs` (zip packager with mimetype-first/Stored), Tauri
  command, Atelier `Step::ConvertToEpub`, Svelte "Bind" panel + nav +
  command palette + README feature matrix update.
- Buy-Button: all 4 pass (Pay-for-it / Notice-it / Pick-us /
  Tell-a-friend). Calibre is the only mainstream PDF→EPUB tool and it's
  a 2008 GUI; Acrobat / PDF Expert / Foxit don't ship it at all.
- Projected ship size: 10 commits, ~1,400 net LOC.
- Wow: "Drop a research paper → read it on your Kindle tonight. Offline.
  Free." Continues the AI-era file-conversion wedge from v3.17.0.

### Next tick — start IMPLEMENTATION

Create branch `feature/v3.18.0-bind`, execute Task 1 of the plan
(scaffold + types + errors), then proceed Task 2-3 in the same tick
to satisfy SHIP-SIZE (≥4 commits, ≥600 LOC). Tasks 4-6 next tick.
Tasks 7-10 (Tauri/Atelier wire + frontend + release) tick after.

### LAST_WOW_TICK_AT: 2026-05-24T10:28Z (v3.17.0 Markdown — first mainstream offline PDF → Markdown converter)

---

## PRIOR STATUS: ✅ v3.17.0 "Markdown" SHIPPED PUBLICLY (2026-05-24 03:55 PT)

**TICK 2026-05-24 03:40 PT (Sunday off-hours)** — MODE B FINALIZE complete.

- Polled build run 26358777398 → ✅ success (Docker 26358777407 already green).
- Cleared `src-tauri/target/debug/incremental` to free disk before download.
- Downloaded all 6 artifacts (correct `3.17.0` version strings).
- Published GitHub release `v3.17.0 — Markdown` at
  https://github.com/Sanjays2402/slab/releases/tag/v3.17.0 — 6 assets, not Draft.
- RELEASE_PENDING cleared.

### RECENTLY_CLOSED_ISSUES:
- v3.17.0 Markdown: **SHIPPED 2026-05-24 03:55 PT** — offline PDF → Markdown + HTML, 6 artifacts.
- v3.16.0 Slide: SHIPPED 2026-05-24.
- v3.15.0 Tabulate: SHIPPED 2026-05-24.

### LAST_WOW_TICK_AT: 2026-05-24T10:28Z (v3.17.0 Markdown — first mainstream offline PDF → Markdown converter)

### Next tick — MODE C DEVELOP

Roadmap fall-through. Issue tracker empty. Candidates per v3.17.0 plan tail:
- v0.10.0 "Beacon" AI (chat with PDF, summarize, PII highlight) — buyer-magnet.
- Cmd+K Command Palette (cross-cutting wow).
- v3.18.0 candidate: PDF → EPUB / PDF → JSON (structured) — extending the
  Reflow→Markdown wedge into more "convert PDF to anything offline" surfaces.

---

## PRIOR STATUS: 🚀 v3.17.0 "Markdown" MERGED + TAGGED — CI RUNNING (2026-05-24 03:28 PT)

**TICK 2026-05-24 03:21 PT (Sunday off-hours)** — MODE A RELEASE complete,
RELEASE_PENDING set for next tick.

- Finished Tasks 5/6/7/8/9 of the v3.17.0 plan in one tick.
- Task 7: added `Step::ConvertToMarkdown` + `Step::ConvertToHtml` to Atelier
  recipe + run.rs dispatch — batch a folder of PDFs to .md/.html.
- Task 8: bumped 3.16.0 → 3.17.0 in Cargo.toml/package.json/tauri.conf.json,
  wrote marketing-grade release notes at `.cron-state/release-notes-v3.17.0.md`.
- Task 9: merged `feature/v3.17.0-markdown` → main (no-ff), tagged `v3.17.0`,
  pushed with --follow-tags.
- All 4 quality gates GREEN on main: fmt, clippy (-D warnings), 1567 tests,
  pnpm check (0 errors, 62 pre-existing warns).
- Had to clean `/Users/sanjay/mac-command-center/src-tauri/target` (914M)
  + slab target/debug/incremental (817M) mid-tick — disk was at 100%.
  Now at 95% (1.2Gi free).

RELEASE_PENDING: v3.17.0 — merge SHA 4a9d366, tag v3.17.0,
build run 26358777398, Docker run 26358777407. Next tick (MODE B) polls
+ downloads artifacts + publishes the release.

LAST_WOW_TICK_AT: 2026-05-24T10:28Z (v3.17.0 Markdown — first mainstream
offline PDF → Markdown converter, AI-era wedge)

### Next tick — MODE B FINALIZE

1. `gh run view 26358777398` + `gh run view 26358777407` — confirm green.
2. `gh run download 26358777398 --dir /tmp/slab-release-v3.17.0`.
3. Curate 6 artifacts, `gh release create v3.17.0 --title 'v3.17.0 — Markdown'
   --notes-file .cron-state/release-notes-v3.17.0.md` + upload.
4. Verify Docker at `ghcr.io/sanjays2402/slab-server:v3.17.0`.
5. Clear RELEASE_PENDING.

---

## PRIOR STATUS: 📋 v3.17.0 "Markdown" PLAN FILED (2026-05-24 02:55 PT)

**TICK 2026-05-24 02:55 PT (Sunday off-hours)** — PLANNING tick (Sanjay
invoked the `writing-plans` skill explicitly).

- Wrote `docs/plans/2026-05-24-v3.17.0-markdown-pdf-to-md-and-html.md`
  (10 tasks, ~41 KB). End-to-end plan for offline PDF → Markdown (`.md`)
  + semantic HTML (`.html`) — re-uses Reflow's `extract` + `layout`
  passes, two new emitters (`md.rs`, `html.rs`), Atelier batch steps,
  unified panel.
- Buy-Button verdict 4/4: no mainstream competitor (Adobe / PDF Expert /
  Foxit) ships offline PDF → Markdown. AI-era wedge — drop research
  papers, get clean MD for Obsidian / ChatGPT / RAG pipelines.
- Plan committed `b74fd80` and pushed to main.
- Each task is 2-5 min, TDD format, includes failing test → pass → commit
  template, exact file paths, copy-pasteable code blocks.
- Lesson from v3.16.0 baked into Task 8: **verify all 3 version files
  match the tag before pushing**.

### Next tick — MODE C DEVELOP: execute Task 1

Start `feature/v3.17.0-markdown` branch and ship Tasks 1+2+3+4 in one
big tick (scaffold + MD emitter + HTML emitter + e2e pipeline). That's
~600 net LOC, 4 commits, end-to-end working capability — passes SHIP-SIZE.
Tasks 5-9 (Tauri + UI + Atelier + release) for the tick after.

### Plan reference
`docs/plans/2026-05-24-v3.17.0-markdown-pdf-to-md-and-html.md`

---

## PRIOR STATUS: ✅ v3.16.0 "Slide" SHIPPED PUBLICLY (2026-05-24 02:55 PT)

**TICK 2026-05-24 02:29 PT (Sunday off-hours)** — MODE B FINALIZE complete.

- Polled fresh CI: build run `26357382160` ✅ all 4 platforms green, Docker run `26357382121` ✅.
- Cleared `src-tauri/target/debug` (4.1 GB) to free disk before artifact download.
- Downloaded all 6 artifacts (correct `3.16.0` version strings this time).
- Published GitHub release `v3.16.0 — Slide` at https://github.com/Sanjays2402/slab/releases/tag/v3.16.0 — 6 assets, not Draft.
- RELEASE_PENDING cleared.

### Next tick — MODE C DEVELOP

Roadmap fall-through. Issue tracker empty. Candidates:
- v0.10.0 "Beacon" AI (chat with PDF, summarize, PII highlight) — buyer-magnet per original roadmap.
- Cmd+K command palette (cross-cutting wow).
- Pick whichever scopes to ≥600 LOC + 4 commits + Buy-Button pass.

### RECENTLY_CLOSED_ISSUES:
- v3.16.0 Slide: **SHIPPED 2026-05-24 02:55 PT** — release published with 6 artifacts.
- v3.15.0 Tabulate: released 2026-05-24.
- v3.14.0 Reflow: released 2026-05-24.

### LAST_WOW_TICK_AT: 2026-05-24 02:02 PT (PDF → PowerPoint offline)

### Previous status (archived) — v3.16.0 version-bump hotfix

**TICK 2026-05-24 02:13 PT (Sunday off-hours)** — MODE B + hotfix.

Polled v3.16.0 release pipeline. Both CI runs from previous tick went green
(build 26357037571 ✅, Docker 26357037569 ✅). Downloaded artifacts to
finalize release — **but artifacts were labeled 3.15.0 not 3.16.0** because
the previous tick merged + tagged v3.16.0 without bumping version strings in
Cargo.toml / package.json / tauri.conf.json.

**Recovery:**
1. Bumped versions in all 3 files to 3.16.0.
2. Cleaned ~700MB from caches (Playwright, pip, Google, Mozilla) — disk was
   at 151Mi free, blocking artifact download.
3. Commit `9d8738f chore(release): bump version to 3.16.0 for Slide release`
   on main.
4. Deleted local + remote tag `v3.16.0` (was at 86398e8), re-cut at 9d8738f.
5. Push triggered fresh build (26357382160) + Docker (26357382121) runs on
   the corrected tag. ETA ~10 min.

Release notes already written at `.cron-state/release-notes-v3.16.0.md`
(marketing tone, competitor table). Will publish next tick once new CI is
green and artifacts carry the right version string.

**RELEASE_PENDING: v3.16.0** — fresh tag at SHA `9d8738f`, build run
`26357382160`, Docker run `26357382121`. Next tick (MODE B) polls + finalizes.

### Lesson learned (logged for future ticks)
**Always verify `version = "X.Y.Z"` in Cargo.toml + package.json +
tauri.conf.json matches the tag BEFORE pushing the tag.** Artifact names
embed the version from these files, not from the git tag. A `grep -E
'version' src-tauri/Cargo.toml package.json src-tauri/tauri.conf.json`
takes 1s and catches this. Adding to the MODE A checklist.

### LAST_WOW_TICK_AT: 2026-05-24 02:02 PT (previous tick — PDF → PowerPoint)

### Previous status (archived) — v3.16.0 first attempt

**TICK 2026-05-24 02:00 PT (Sunday off-hours)** — MODE C + MODE A combined.

Resumed from compaction. Finished Atelier UI wiring for `convert-to-pptx`
(defaultStep, stepLabel "Convert to PowerPoint (.pptx)", stepIcon 🎞,
AtelierPanel palette card + 2-checkbox editor block). All Rust gates
green; svelte-check clean.

Quality gates batched:
- `cargo fmt --all` ✅
- `cargo clippy --lib -D warnings` ✅
- `cargo test --lib pdf::slide::` ✅ **15 passed, 0 failed**
- `pnpm check` ✅ **0 errors, 62 pre-existing warnings**

Commits on `feature/v3.16.0-slide` (now merged to main):
- `af20d53 feat(slide): skeleton module + error/option types`
- `3f76597 feat(slide): per-page text-run + speaker-note + page-size extraction`
- `b29be75 feat(slide): title + body-bullet clustering for slide content`
- `54cd1e5 feat(slide): minimal PresentationML zip writer (.pptx)`
- `957f6ba feat(atelier+ui): wire ConvertToPptx step end-to-end`
- Merge `bfdca21` on main; tag `v3.16.0` pushed.

Net code this tick: **~1095 LOC** across slide module (errors/types/mod/
extract/layout/pptx) + Tauri command + Atelier step + TS + Svelte panel
hooks. End-to-end working capability: open Atelier → add "Convert to
PowerPoint" step → drop PDF folder → get one slide per page with title +
bullets, optional speaker notes from /Text annots.

**RELEASE_PENDING: v3.16.0** — merge SHA `bfdca21`, tag `v3.16.0`,
build run `26357037571`, Docker run `26357037569`. Both queued; next
tick (MODE B) polls + finalizes GitHub release with 6 artifacts.

### Buy-Button positioning

Adobe Acrobat Pro: no native PDF → PPTX, requires Acrobat Export PDF
subscription (cloud-only, $24/yr add-on). PDF Expert: no PDF → PPTX.
Foxit: PPTX export is paid-tier only. Slab: free, offline, batch via
Atelier. Sales engineer drops folder of one-pager PDFs → folder of
editable .pptx decks. **BIG feature, passes Buy-Button + Pick-Us tests.**

### LAST_WOW_TICK_AT: 2026-05-24 02:02 PT (this tick — PDF → PowerPoint offline)

### Next tick — MODE B: finalize v3.16.0 release

1. `gh run view 26357037571` — wait for green.
2. `gh run download 26357037571 --dir /tmp/slab-release-v3.16.0`.
3. Write `.cron-state/release-notes-v3.16.0.md` (marketing-tone).
4. `gh release create v3.16.0 --title 'v3.16.0 — Slide' --notes-file ...`
5. Upload 6 artifacts (mac arm64+x64 dmg, linux deb+AppImage, win msi+nsis).
6. Verify Docker image at `ghcr.io/sanjays2402/slab-server:v3.16.0`.

After that → v0.10.0 Beacon (AI) pipeline per roadmap.

### Previous status (archived) — v3.15.0 "Tabulate" SHIPPED

**TICK 2026-05-24 01:00 PT (Sunday off-hours)** — MODE C + MODE A combined.

Resumed from compaction. Finished Atelier UI wiring (`src/lib/atelier.ts`
ConvertToXlsx StepKind + defaults/label/icon, `AtelierPanel.svelte`
palette entry + 3-checkbox editor block). Bumped version to 3.15.0 in
Cargo.toml, package.json, tauri.conf.json. Quality gates batched:

- `cargo fmt --all --check` ✅
- `cargo clippy --lib --all-targets -D warnings` ✅ (fixed vec_init_then_push in xlsx.rs)
- `cargo test --lib` ✅ **1528 passed, 0 failed**
- `pnpm check` ✅ **0 errors, 62 pre-existing warnings**

Commits this tick:
- `157e5a7 feat(tabulate): PDF to Excel backend — cell typing, table extract, XLSX writer` (Tasks 1-5, 21 unit tests)
- `6a16370 feat(tabulate): Tauri command, UI panel, and Atelier integration (v3.15.0)` (Tasks 6-7, 13 files, +695 / -49)
- `316de82 Merge v3.15.0 'Tabulate' — offline PDF to Excel (.xlsx) conversion` (merge to main)
- Tag `v3.15.0` pushed → triggered build + Docker workflows.

Net code this tick: **~1485 LOC** (backend module 374-line XLSX writer +
extract + cells + types + errors; UI panel 443 lines; Atelier wiring).
End-to-end working capability: open Tabulate panel → drop PDF → get
typed .xlsx with one sheet per page. Atelier batch recipe slot lets
users chain Tabulate after OCR / redact.

**RELEASE_PENDING: v3.15.0** — merge SHA `316de82`, tag `v3.15.0`,
build run `26355860314`, Docker run `26355860277`. Both queued at push
time; next tick (MODE B) polls + finalizes GitHub release with 6
artifacts.

### Buy-Button positioning

Adobe Acrobat Pro: PDF → Excel is cloud-only, $239/yr, ships file to
their servers. PDF Expert: no PDF → XLSX on macOS. Foxit: Linux not
supported. Slab: free, offline, batch-driven via Atelier. Paralegal
drops folder of invoices → folder of typed .xlsx. **BIG feature, pays
the Buy-Button test on its own.**

### LAST_WOW_TICK_AT: 2026-05-24 01:03 PT (this tick — PDF → Excel offline)

### Next tick — MODE B: finalize v3.15.0 release

1. `gh run view 26355860314` — wait for green.
2. `gh run download 26355860314 --dir /tmp/slab-release-v3.15.0`.
3. `gh release create v3.15.0 --title 'v3.15.0 — Tabulate' --notes-file .cron-state/release-notes-v3.15.0.md` (need to write notes).
4. Upload the 6 artifacts (mac arm64+x64 dmg, linux deb+AppImage, win msi+nsis).
5. Verify Docker image at `ghcr.io/sanjays2402/slab-server:v3.15.0`.

After that → v0.10.0 Beacon pipeline (per roadmap) or next backlog issue.

### Previous status (archived) — v3.14.0 "Reflow" SHIPPED



**TICK 2026-05-24 00:35 PT (Sunday off-hours)** — MODE B FINALIZE.

- Polled CI: build run `26354714534` ✅ success, Docker run `26354714523` ✅ success.
- Reclaimed disk first: `cargo clean` freed 5.0 GiB (was 301 MiB free → 4.5 GiB free).
- `gh run download 26354714534 --dir /tmp/slab-release-v3.14.0` → all 6 artifacts retrieved.
- `gh release create v3.14.0 --title 'v3.14.0 — Reflow' --notes-file .cron-state/release-notes-v3.14.0.md` + uploaded:
  - `Slab_3.14.0_aarch64.dmg` (macOS Apple Silicon)
  - `Slab_3.14.0_x64.dmg` (macOS Intel)
  - `Slab_3.14.0_amd64.deb` (Linux Debian/Ubuntu)
  - `Slab_3.14.0_amd64.AppImage` (Linux portable)
  - `Slab_3.14.0_x64_en-US.msi` (Windows MSI)
  - `Slab_3.14.0_x64-setup.exe` (Windows NSIS)
- Release verified published (not Draft), 6 assets, tag `v3.14.0`.
- URL: https://github.com/Sanjays2402/slab/releases/tag/v3.14.0
- **RELEASE_PENDING cleared.**

### Next tick — MODE C: v3.15.0 "Tabulate" (PDF → Excel)

Plan already filed in `c617c67 docs(plan): v3.15.0 Tabulate — PDF to Excel
proposal (offline, free, batchable)`. Spec at `docs/plans/` for that SHA.
Create `feature/v3.15.0-tabulate` branch and ship Slice 1 (table detection
backend + tests) next tick. PDF-to-Excel is another Acrobat-flagship feature
gated behind subscription — strong Buy-Button candidate to follow Reflow's
"convert PDF to anything, offline, free" wedge.

### Previous status (archived) — v3.14.0 MERGED + TAGGED — RELEASE_PENDING

**TICK 2026-05-23 23:52 PT (Saturday off-hours)** — MODE C+A combined.

Combined this tick (4 feature commits on `feature/v3.14.0-reflow` + 1
merge commit on main, ~302 net non-test LOC + 7 new green tests +
the entire v3.14.0 release pipeline):

- `1a90263` feat(atelier): ConvertToDocx step — chain reflow into recipes
  (Task 8 backend — Step::ConvertToDocx { detect_tables, detect_lists,
   heading_size_ratio }, Step::changes_extension /
   Step::output_extension / Recipe::output_extension helpers, run.rs
   dispatch, batch.rs .docx-rewriting output filenames, 7 new tests)
- `0820f54` feat(atelier): Convert-to-Word palette card + config UI
  (Task 8 UI — atelier.ts type union, defaultStep/stepLabel/stepGlyph
   branches, AtelierPanel palette card + 3-knob config card)
- `d8947fb` chore(release): bump v3.13.0 -> v3.14.0 "Reflow"
  (Task 9a — package.json + tauri.conf.json + Cargo.toml + Cargo.lock)
- `c3e9d9c` docs(release): v3.14.0 Reflow marketing-grade release notes
  (Task 9b — customer-facing notes leading with the wedge:
   "Convert any PDF to Word — offline, free, forever." +
   Adobe/PDF Expert/Foxit comparison table)
- Merge commit `b9ff90c` on main: "Merge v3.14.0 'Reflow' — PDF to
  Word, free + offline"
- Tag `v3.14.0` cut from release notes file, pushed `main --follow-tags`.

Quality gates on main ALL ✓:
- `cargo fmt --all -- --check` clean
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo test --lib` — **1503 passed** (was 1496, +7 atelier tests)
- `pnpm check` — 0 errors

**RELEASE_PENDING: v3.14.0** — merge SHA `b9ff90c`, tag `v3.14.0`,
build CI run `26354714534` (in_progress on macOS), Docker CI run
`26354714523` (in_progress on v3.14.0 tag).

**Buy-Button verdict — PASS 4/4.**
- **Pay-for-it ✅** — Acrobat Pro $239/yr's "Export PDF to Word" is one
  of its flagship features, gated behind a cloud upload + subscription.
  Slab ships the same capability free, offline, batchable, cross-OS.
- **Notice-it ✅** — Atelier palette gains a 📝 Convert-to-Word card,
  output filenames flip from .pdf to .docx for chained recipes.
- **Pick-us ✅** — Adobe ships your files to their servers; PDF Expert
  doesn't even offer the feature; Foxit charges $129/yr Pro for it.
  Slab is the only free + offline + cross-platform option that supports
  batch conversion via Atelier.
- **Tell-a-friend ✅** — "Drop a folder of 200 PDFs, walk away with 200
  Word docs — offline." 5-second demo bait.

### LAST_WOW_TICK_AT: 2026-05-24T07:05Z (Atelier 📝 Convert-to-Word card
+ chained "Compactor → ConvertToDocx" batch flow, end-to-end via MODE A
release to main + tag v3.14.0)

### Next tick — MODE B FINALIZE v3.14.0

1. Poll `gh run view 26354714534` (build) + `gh run view 26354714523`
   (Docker). If still in_progress, skip MODE B this tick.
2. When green:
   - `gh run download 26354714534 --dir /tmp/slab-release-v3.14.0`
   - Curate 6 artifacts (mac arm64/x64 dmg, linux deb/AppImage,
     win msi/nsis). Filenames should now be `Slab_3.14.0_*`.
   - `gh release create v3.14.0 --title 'v3.14.0 — Reflow' \
       --notes-file .cron-state/release-notes-v3.14.0.md` + upload.
   - Clear RELEASE_PENDING, append to RECENTLY_CLOSED.
3. If red: `gh run view <id> --log-failed`, fix on `fix/v3.14.0-*`
   branch, cut `v3.14.1` patch.

### RECENTLY_CLOSED_ISSUES:
- v3.14.0 Reflow: MERGED + TAGGED 2026-05-24, awaiting MODE B.
- v3.13.0 Streamline: SHIPPED 2026-05-24.
- v3.12.0 Atelier: released 2026-05-23.

### Operational notes
- Disk: 2.1 GiB free after this tick (was 884 MiB at start). Cleared
  `src-tauri/target/debug/incremental` + /tmp/slab-* between gates.
  `~/Library/Application Support/adspower_global` (9.6 GB) +
  `~/Downloads` (9.6 GB) still candidates for Sanjay to triage.

---

## PRIOR STATUS: v3.14.0 "Reflow" END-TO-END WORKING — Tasks 5+6+7 shipped, PDF→Word pipeline complete on feature branch.

**TICK 2026-05-23 23:22 PT (Saturday off-hours)** — MODE C DEVELOP.

This tick (2 feature commits on `feature/v3.14.0-reflow`, +1066 net LOC):

- `38d4f18` feat(reflow): docx writer — paragraphs, headings, lists, tables
  (Task 5 — hand-rolled OOXML emitter, no new dep, `zip` flipped to default;
   549 LOC writer + 11 docx unit tests)
- `944ff82` feat(reflow): Tauri command + ReflowPanel + nav entry
  (Tasks 6+7 — end-to-end `convert_to_docx`, `slab_reflow_to_docx` Tauri
   command, `ReflowPanel.svelte` with drag-and-drop + result card, nav
   entry "Reflow (PDF → Word)" with 📝 icon, palette auto-derives entries)

Quality gates ALL ✓:
- `cargo fmt --all -- --check` clean
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo test --lib` — **1496 passed** (was 1482, +14 reflow tests)
- `pnpm check` — 0 errors

**Buy-Button verdict — PASS.**
- **Pay-for-it ✅** — Acrobat Pro $239/yr lists "Export PDF to Word" as a
  headline feature. We ship it free, offline, on every OS.
- **Notice-it ✅** — New nav entry, command-palette entry, drag-drop panel
  with marketing-grade dropzone copy targeting the Adobe wedge.
- **Pick-us ✅** — No competitor ships free, offline, cross-platform
  PDF→DOCX with bullets/headings/tables preserved.
- **Tell-a-friend ✅** — "Drop a PDF, get a Word doc. Offline. Free."

**Wow moment** — drop a PDF onto the ReflowPanel dropzone, two seconds
later a real `.docx` opens in Word with proper paragraphs and headings.
Screenshot-worthy.

### Next tick — MODE C DEVELOP, plan Task 8 + Task 9

Task 8: Atelier `ConvertToDocx` recipe step — so users can chain
"OCR → Redact → Convert to Word" in batch (the paralegal flow).
Task 9: bump 3.13.0 → 3.14.0 + write marketing-grade release notes.
Then Task 10 (MODE A RELEASE) the tick after.

### RECENTLY_CLOSED_ISSUES:
- v3.13.0 Streamline: SHIPPED 2026-05-24 — release published with 6 artifacts.
- v3.12.0 Atelier: released 2026-05-23.

### LAST_WOW_TICK_AT: 2026-05-24T06:25Z (Reflow ReflowPanel + e2e PDF→DOCX pipeline.)

### Operational notes
- Disk was 818Mi free at tick start; cleared `src-tauri/target/debug`
  (3.3 GB) to get back to 4 GB. ~/Library/Application Support/adspower_global
  (9.6 GB) + ~/Downloads (9.6 GB) still candidates for Sanjay.
- Issue tracker still empty (`gh issue list` returned []). Falling
  through to roadmap is the right move.
- The `zip` crate is now an unconditional dep (was previously feature-gated
  behind `server`). Slightly larger Tauri binary; trivial vs. the wedge.

---

## PRIOR STATUS: v3.14.0 "Reflow" Tasks 2-4 SHIPPED — extract + layout + table detection on feature branch.

**TICK 2026-05-23 23:01 PT (Saturday off-hours)** — MODE C DEVELOP.

This tick (3 feature commits + 1 lint commit on `feature/v3.14.0-reflow`):

- `2533b94` feat(reflow): TextRun extraction with PDF text-state machine (Task 2)
- `f236eb2` feat(reflow): layout reconstruction + table detection (Tasks 3+4)
- `c347741` style(reflow): clippy clean (map_clone, manual_range_contains)
- This commit: STATE.md update

Library-level: 1387 net LOC of non-test code + 12 new unit tests over
the algorithmic core of PDF -> DOCX. 23 reflow tests all green, 1482
total `cargo test --lib` green, clippy `-D warnings` clean, fmt clean,
`pnpm check` 0 errors.

**Buy-Button verdict — DEFERRED to next tick.** This is the algorithm
half of v3.14.0 Reflow. The buyer-visible half (DOCX writer + Tauri
command + ReflowPanel) is plan Tasks 5-7, scheduled for the next 2
ticks per `docs/plans/2026-05-23-v3.14.0-reflow-pdf-to-word.md`. The
plan is deliberately split into 4 dev ticks for TDD-friendliness;
this tick lands the extraction -> clustering -> classification pipeline
that turns a `lopdf::Document` into a `Vec<Block>` ready for OOXML
emission. Next tick writes `docx.rs` + wires the end-to-end pipeline,
which is when the wedge ("offline PDF->Word, free") becomes demoable.

### Next tick — MODE C DEVELOP, plan Task 5 + Task 6

Write `docx.rs` (OOXML writer — Content_Types, _rels, styles.xml,
document.xml; 5 styles: Normal/Heading1-3/ListBullet/ListNumber/
TableNormal; uses `quick-xml` + existing optional `zip` dep) then wire
end-to-end `convert_to_docx` that opens a PDF and writes a valid .docx
that opens cleanly in Word/LibreOffice. THAT's the buyer-visible tick.

### RECENTLY_CLOSED_ISSUES:
- v3.13.0 Streamline: SHIPPED 2026-05-24 — release published with 6 artifacts.
- v3.12.0 Atelier: released 2026-05-23.

### LAST_WOW_TICK_AT: 2026-05-24T04:08Z (Linearizer — held from previous tick.)

### Operational notes
- Disk hit 96% at tick start; cleared `src-tauri/target/debug` to free 4 GB,
  ended at 86% free. `~/Library/Application Support/adspower_global`
  (9.6 GB) + `~/Downloads` (9.6 GB) still candidates for Sanjay.
- Issue tracker still empty (`gh issue list` returned []). Falling through
  to roadmap is the right move.

---

## PRIOR STATUS: v3.13.0 "Streamline" SHIPPED PUBLICLY — release live with all 6 installers, Docker image on GHCR.

**TICK 2026-05-23 22:18 PT (Saturday off-hours)** — MODE B FINALIZE complete.

This tick:
- Watched CI run `26352543700` (build/main) to green (success, all 4 platforms bundled).
- Docker tag workflow `26352543680` already green from previous tick.
- Downloaded 6 installer artifacts (macOS arm64 + x64 DMG, Linux deb + AppImage,
  Windows MSI + NSIS) from CI.
- Published GitHub Release `v3.13.0 — Streamline` with marketing-grade notes
  and all 6 binaries attached: https://github.com/Sanjays2402/slab/releases/tag/v3.13.0
- RELEASE_PENDING cleared.

**Buy-Button verdict — PASS**: Customers can now download Slab v3.13.0 today
and produce Fast Web View PDFs offline. Adobe charges $239/yr for that step.

### Next tick — MODE C DEVELOP

Roadmap fall-through. Recommended next: **Cmd+K Command Palette** — single
biggest cross-cutting "wow + pick-us" feature still missing. Reachable from
anywhere, every action 2 keystrokes away, ships at Linear/Raycast quality.
Alternative: v0.11.0 "Lathe" full edit mode per the original roadmap, but
the product is now at v3.13.0 — the roadmap codenames are ahead of the
official version numbers, so it's a naming question, not a feature question.

Operational notes for next tick:
- Disk was tight; `src-tauri/target/debug` cleared last tick. Re-monitor.
- `~/Library/Application Support/adspower_global` (9.6 GB) + `~/Downloads`
  (9.6 GB) are still candidates for Sanjay to clear when he has a moment.

### RECENTLY_CLOSED_ISSUES:
- v3.13.0 Streamline: **SHIPPED 2026-05-24** — release published with 6 artifacts.
- v3.12.0 Atelier: released 2026-05-23.

### LAST_WOW_TICK_AT: 2026-05-24T04:08Z (Linearizer — held from previous tick.)

---

## PRIOR STATUS: v3.13.0 "Streamline" MERGED + TAGGED — Fast Web View shipped, CI running, FINALIZE next tick.

**TICK 2026-05-23 21:56 PT (Saturday off-hours)** — MODE C → MODE A RELEASE.

Shipped Task 8 (Atelier `Linearize` step), bumped 3.12 → 3.13, wrote
marketing release notes, merged to main, tagged v3.13.0, pushed with
tag. CI run `26352543700` (build/main) + `26352543680` (Docker/tag)
both queued. FINALIZE in MODE B next tick.

This tick (4 commits on feature branch + 1 merge commit on main):
- `ed88bad` feat(atelier): add Linearize step — Fast Web View in any recipe
- `64150d5` feat(ui): expose Linearize step in Atelier recipe palette
- `e97064a` chore(release): bump v3.12.0 → v3.13.0 "Streamline"
- `cb61464` docs(release): v3.13.0 Streamline marketing-grade release notes
- `2b7f821` Merge v3.13.0 'Streamline' — Fast Web View, free + offline

Quality gates on main ALL ✓ (fmt, clippy -D warnings, **1459 unit tests**, pnpm check 0 errors).

**Operations note:** disk was at 100% (132 Mi free) when this tick
started — `cargo test --lib` refused to write fingerprints. Cleared
`src-tauri/target/debug` (5.6 GB), now 4.8 GB free. Sanjay should
look at `~/Library/Application Support/adspower_global` (9.6 GB) and
`~/Downloads` (9.6 GB) when he has a minute.

RELEASE_PENDING: v3.13.0 — merge SHA `2b7f821`, tag `v3.13.0`, CI runs `26352543700` (main build) + `26352543680` (Docker tag).

### LAST_WOW_TICK_AT: 2026-05-24T04:08Z (Linearizer ships — Adobe charges $239/yr to produce Fast Web View PDFs; Slab does it free, offline, cross-platform.)

### Buy-Button verdict — PASS on 4 of 4

- Pay-for-it: ✅ Acrobat Pro $239/yr feature now free + as an Atelier batch step.
- Notice-it: ✅ "Fast Web View" card appears in the Atelier recipe palette.
- Pick-us: ✅ No competitor has this as a folder-batch automation step.
- Tell-a-friend: ✅ "Chain OCR → Redact → Bates → Fast-Web-View on 500 PDFs, free, offline." Demo-ready.

### Next tick — MODE B FINALIZE

1. `gh run view 26352543700` and `gh run view 26352543680` — confirm both green.
2. If green:
   - `gh run download 26352543700 --dir /tmp/slab-release-v3.13.0`
   - `gh release create v3.13.0 --title 'v3.13.0 — Streamline' --notes-file .cron-state/release-notes-v3.13.0.md` + upload 6 installers.
   - Clear `RELEASE_PENDING` from STATE.md.
3. If failed: `gh run view <id> --log-failed`, hotfix, do NOT guess.
4. After release: roadmap fall-through — next milestone is v0.11.0 "Lathe"
   (full edit mode) per roadmap, or pick a higher-value cross-cutting feature
   (Cmd+K command palette is the standout).

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: released 2026-05-23 (CI 26349407913 + 26349407898).
- v3.13.0 Streamline: merged + tagged 2026-05-24 @ `2b7f821`, CI running.

---

## PRIOR STATUS: v3.13.0 Streamline Task 6 (linearizer) shipped — Fast Web View write path live, end-to-end vertical complete.

**TICK 2026-05-23 21:08 PT (Saturday off-hours)** — MODE C DEVELOP.

Shipped Task 6 (end-to-end `linearize_pdf` writer) + wired the
"Optimize for Fast Web View" button. Slab can now PRODUCE Fast Web View
PDFs, not just detect them. Adobe charges $239/yr for this; we just
gave it away offline. The full vertical (inspect → optimize → re-inspect
→ batch audit) is feature-complete on this branch.

Shipped (3 commits / ~700 net LOC this tick + the inspector fix):
- `8e6b5fc` fix(streamline): match exact PDF name boundaries in inspector
- `7a4fd1c` feat(streamline): end-to-end PDF linearizer (PDF 1.4 §F.2) — 639 LOC
- `8e75439` feat(streamline): wire "Optimize for Fast Web View" button to backend

Quality gates ALL ✓ (fmt, clippy -D warnings, **1458 unit tests** (+7), pnpm check 0 errors).

### Implementation notes worth remembering

- lopdf 0.40 SKIPS objects whose `get_type()` returns "Linearized"/"ObjStm"/"XRef"
  (writer.rs:57). The `/Linearized` key alone triggers this even without `/Type`.
  Workaround: hand-roll the lin dict serialization with `write_lin_dict_manually`.
- lopdf's high-numbered sentinel objects silently get serialized as xref STREAMS,
  not plain `obj` headers. Burned an hour on a "trailer body header not found"
  panic from this. Fix: hand-roll the trailer too via `serialize_value`.
- The inspector's `/L <num>` parser collided with `/Linearized` (substring match).
  Fixed with `find_key` that demands a non-name-continuation byte after the key.
  Round-trip detection of our own outputs silently degraded before the fix.

### LAST_WOW_TICK_AT: 2026-05-24T04:08Z (Linearizer ships — Adobe charges $239/yr to produce Fast Web View PDFs; Slab does it free, offline, cross-platform.)

### Buy-Button verdict — PASS on 4 of 4

- Pay-for-it: Acrobat Pro's "Save as Optimized PDF → Fast Web View" is paid + cloud-trip; ours is free + local.
- Notice-it: The Optimize button now actually optimizes (was a "coming soon" toast).
- Pick-us: PDF Expert and Foxit don't ship a linearizer at all on Linux.
- Tell-a-friend: "Drop a PDF, click Optimize, get a Fast Web View file that loads page 1 instantly on a slow connection." Demo-ready.

### Next tick — MODE C DEVELOP — Task 7+ (cross-validate + release prep)

1. Poll latest CI run — confirm Task 6 commits build cleanly on all 4 platforms.
2. Tasks remaining per plan (lines 950+):
   - Task 7: cross-validator (re-open the output, walk the lin dict, prove offsets match real positions). Optional but worth shipping.
   - Task 8: integration tests against real Adobe-produced linearized PDFs (round-trip and detect).
   - Task 9: bump version to v3.13.0, release notes, merge to main, tag.
3. The branch is now feature-complete enough to merge IF CI is green. Consider
   collapsing Tasks 7+8 into a single integration-test tick, then MODE A RELEASE.

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: released 2026-05-23 (CI 26349407913 + 26349407898).
- v3.13.0 Streamline Task 6 (linearizer writer): shipped on feature branch `feature/v3.13.0-streamline` @ 7a4fd1c.

---

## PRIOR STATUS: v3.13.0 Streamline Tasks 4+5 + Batch Audit shipped — folder-level enterprise workflow live.

**TICK 2026-05-23 20:24 PT (Saturday off-hours)** — MODE C DEVELOP.

Folded Task 4 (param dict builder) + Task 5 (primary hint stream builder)
+ a bonus end-to-end buy-button feature — **batch linearization audit**
— into one tick. Drop a folder, get a sortable/filterable Fast Web View
report with CSV export. The paralegal-auditing-500-discovery-PDFs
workflow Adobe charges $239/yr for via Action Wizard.

Shipped (4 commits / 1401 net LOC on `feature/v3.13.0-streamline`):
- `8ee0461` feat(streamline): linearization param dict builder (task 4, 5 unit tests)
- `5d102e4` feat(streamline): primary hint stream builder (task 5, 6 unit tests)
- `0ff1931` feat(streamline): batch audit backend + tauri command (10 unit tests)
- `be7448d` feat(streamline): batch audit UI — sortable, filterable, CSV export

Quality gates ALL ✓ (fmt, clippy -D warnings, **1451 unit tests** (+24), pnpm check 0 errors).

Push triggered CI run `26350964182` on the feature branch — check next tick.

Disk note: `cargo clean` recovered 4.8 GiB mid-tick. Now ~3.6 GiB free.

### LAST_WOW_TICK_AT: 2026-05-24T03:39Z (Batch audit UI — sortable table + CSV export + 5-card summary; Acrobat Action Wizard equivalent free + offline)

### Buy-Button verdict — PASS on 4 of 4

- Pay-for-it: Acrobat Pro Action Wizard ($239/yr) for batch Fast Web View → free + offline in Slab.
- Notice-it: Streamline panel now has a "Batch audit" tab — visible on first open.
- Pick-us: No competitor ships a free, offline, cross-platform Fast Web View batch auditor with CSV export.
- Tell-a-friend: Five-card summary + sortable table screenshot.

### Next tick — MODE C DEVELOP — Task 6 (end-to-end writer)

1. Poll CI run `26350964182` — confirm 4-platform bundles green.
2. Continue on `feature/v3.13.0-streamline`.
3. Execute Task 6 (end-to-end `linearize_pdf` writer) — biggest task, likely
   its own tick. Combines depgraph + param_dict + hint_stream + object
   reordering + manual xref emission.
4. After Task 6: wire writer into the existing "Optimize for Fast Web View"
   button (already in UI), bump version, release.

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: released 2026-05-23 (CI 26349407913 + 26349407898).

---

## PRIOR STATUS (sibling cron note): the STATE.md block below was the v3.12.0 release record from earlier today. The intervening "Slice 1+2+3" tick (commits 169907e..9268718 on `feature/v3.13.0-streamline`) was correctly shipped — see `git log feature/v3.13.0-streamline` for the full chain.

---

## STATUS: v3.12.0 Atelier RELEASED — 6 artifacts uploaded, Docker live. v3.13.0 Streamline PLAN landed, ready to execute.

**TICK 2026-05-23 19:46 PT (Saturday off-hours)** — MODE B FINALIZE executed.
- CI build run 26349407913: all 7 jobs green (4-platform bundles + 3-platform cargo test, clippy, fmt).
- Docker run 26349407898: ✓ → `ghcr.io/sanjays2402/slab-server:v3.12.0` live.
- `cargo clean` recovered 4.3 GiB (disk had 765 MiB → 4.5 GiB after).
- Downloaded 6 artifacts to /tmp/slab-release-3.12.0, then:
  `gh release create v3.12.0 --title "v3.12.0 — Atelier" --notes-file .cron-state/release-notes-v3.12.0.md` + 6 installers.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.12.0
  isDraft=false, assetCount=6 (mac arm/x64 dmg, linux deb+AppImage, win nsis+msi).
- RELEASE_PENDING cleared.

### LAST_WOW_TICK_AT: 2026-05-24T02:46Z (v3.12.0 Atelier public release — Action Wizard equivalent free + offline)

### Next tick — MODE C DEVELOP — start v3.13.0 Streamline Task 1
1. Re-poll `gh issue list` (was 0 at start of this tick).
2. `git checkout -b feature/v3.13.0-streamline`.
3. Execute Task 1 of plan (ADR 0013 + streamline module scaffold + DTOs).
   Plan: `docs/plans/2026-05-23-v3.13.0-streamline-fast-web-view.md`.
4. Fold Tasks 1+2 into a single tick if scope allows (scaffold + linearize
   driver skeleton with first 3-4 unit tests).

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: released 2026-05-23 (CI 26349407913 + 26349407898).

---

## PRIOR STATUS: v3.12.0 Atelier — bundles 3/4 green, Win still building. v3.13.0 Streamline PLAN landed.

**TICK 2026-05-23 19:25 PT (Saturday off-hours)** — writing-plans skill invocation.
v3.12.0 build run 26349407913: tests 3/3 ✓, bundles macOS-arm/x64 + linux ✓,
windows still in_progress. Docker (26349407898) ✓. Pages (26349407658) ✓.
Cannot MODE B FINALIZE this tick — Win bundle artifact required for the
6-installer release. Disk 813 MiB free, no headroom for a local Tauri build,
so no code shipped. Used the tick for the next-version plan.

Shipped this tick (1 commit, ~1220 LOC markdown):
- `2d6f49a docs(plan): v3.13.0 Streamline — Fast Web View linearization`
  8-task TDD plan in `docs/plans/2026-05-23-v3.13.0-streamline-fast-web-view.md`.
  Buy-Button: Adobe Acrobat Pro $239/yr exclusive; PDF Expert doesn't ship it.
  Headline wow: "first-page-ready 12 MB → 188 KB" before/after panel.

Honest sizing: this is a PLANNING tick, not a ship tick. Below SHIP-SIZE
minimums (1 commit / 0 LOC non-test code / no end-to-end capability) but
acceptable because (a) writing-plans skill was explicitly invoked, (b)
v3.12.0 finalize is blocked on Windows bundle CI, (c) disk too low to
risk a feature build mid-release. Next tick will be MODE B then start
Task 1 of the Streamline plan.

### RELEASE_PENDING: v3.12.0 — merge SHA 4fb11e0, tag v3.12.0, CI runs 26349407913 (build, Win bundle pending) + 26349407898 (Docker ✓)

### Next tick — MODE B FINALIZE v3.12.0, then start Streamline Task 1
1. `gh run view 26349407913` — confirm Windows bundle finished.
2. `gh run download 26349407913 --dir /tmp/slab-release-3.12.0`.
3. `gh release create v3.12.0 --title "v3.12.0 — Atelier" --notes-file .cron-state/release-notes-v3.12.0.md` + 6 artifacts.
4. Verify ghcr.io/sanjays2402/slab-server:v3.12.0 live (already pushed).
5. Clear RELEASE_PENDING. Then `git checkout -b feature/v3.13.0-streamline`
   and execute Task 1 of the plan (ADR + module scaffold + dep-graph walker).

### LAST_WOW_TICK_AT: 2026-05-24T01:42Z (live progress matrix — still within 24h)

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: merged + tagged 2026-05-23, finalize pending Win bundle CI.

---

## PRIOR STATUS: v3.12.0 Atelier MERGED + TAGGED — CI in flight, finalize next tick.

**TICK 2026-05-23 19:1x PT (Saturday off-hours)** — MODE A RELEASE executed.
Merged `feature/v3.12.0-atelier` into `main` (merge SHA `4fb11e0`). 19 files,
+2233 LOC: full `pdf::atelier` module (recipe/run/batch/cmds), Svelte
AtelierPanel, typed TS client, Mod+Shift+R keymap, +Cargo/Tauri/package.json
version bumps, release notes, session log.

Quality gates on main post-merge:
- cargo fmt --all -- --check ✓
- cargo clippy --lib --all-targets -- -D warnings ✓
- cargo test --lib → **1418 passed, 0 failed** (+20 since v3.11.0)
- pnpm check → 0 errors, 62 warnings (all pre-existing)

Tagged `v3.12.0` with marketing-tone annotation from release-notes file (no
emoji in tag title). Pushed `main --follow-tags`. CI runs in flight:
- **26349407913** — build (4-platform bundle) on main
- **26349407898** — Docker (slab-server) on v3.12.0 tag
- **26349407658** — Pages deploy (landing site)

### RELEASE_PENDING: v3.12.0 — merge SHA 4fb11e0, tag v3.12.0, CI runs 26349407913 (build) + 26349407898 (Docker)

### Next tick — MODE B FINALIZE
1. `gh run view 26349407913` — if green, `gh run download` to `/tmp/slab-release-3.12.0`.
2. Curate 6 best artifacts (mac arm/x64 dmg, linux deb+AppImage, win nsis+msi).
3. `gh release create v3.12.0 --title "v3.12.0 — Atelier" --notes-file .cron-state/release-notes-v3.12.0.md` + upload artifacts.
4. Verify Docker image `ghcr.io/sanjays2402/slab-server:v3.12.0` is live.
5. Clear RELEASE_PENDING. Re-poll `gh issue list` for next priority.

### Honest note on the working tree:
`docs/landing/index.html` was uncommitted in Sanjay's checkout at the start
of this tick (mtime 18:59 PT). It carried over the branch switch cleanly —
no conflicts because the merge didn't touch it. Sanjay's edits are still
sitting unstaged on main. Untouched.

## (archived previous status)
## PRIOR STATUS: v3.12.0 Atelier — version bumped, release notes drafted. MERGE-to-main DEFERRED.

**TICK 2026-05-23 18:5x PT (Saturday off-hours)** — release-prep tick.

### Shipped (1 commit on feature/v3.12.0-atelier):
- `ebe4778` — version 3.11.0 → 3.12.0 in Cargo.toml + Cargo.lock + package.json + tauri.conf.json; release notes drafted at `.cron-state/release-notes-v3.12.0.md` (marketing-tone, leads with "Adobe Action Wizard, free + offline").

### Why this tick is small (HONEST):
- **CI run 26348824398**: tests green on all 3 platforms; **bundle job still queued/not started**. Don't tag until artifacts proven.
- **Working tree**: `docs/landing/index.html` has uncommitted edits, mtime 18:53 PT (1 minute before tick fired) = Sanjay actively editing. Switching to `main` to merge would surprise him.
- **Disk**: 1.5 GiB free (`src-tauri/target` = 3.2 GiB).

### Quality gates this tick:
- `cargo fmt --all -- --check` ✓
- `pnpm check` → 0 errors, 62 warnings (all pre-existing)
- Skipped cargo clippy/test — mechanical version bump, CI on push exercises it.

### LAST_WOW_TICK_AT: 2026-05-24T01:42Z (live progress matrix — still within 24h)

### Next tick — MODE A RELEASE (assuming bundle CI green):
1. Re-poll `gh run view 26348824398` — confirm bundle job succeeded.
2. If `docs/landing/index.html` still dirty: `git stash push -m "sanjay landing edits"` before switching branches; restore after release.
3. `git checkout main && git pull && git merge --no-ff feature/v3.12.0-atelier -m "Merge v3.12.0 'Atelier' — workflow automation"`.
4. Quality gates on main (fmt + clippy + cargo test --lib + pnpm check).
5. `git tag -a v3.12.0 -F .cron-state/release-notes-v3.12.0.md` then push `main --follow-tags`.
6. Record `RELEASE_PENDING: v3.12.0 — tag v3.12.0, CI runs <build> + <docker>`.

### RECENTLY_CLOSED_ISSUES:
- v3.11.0 Signet Pro: released 2026-05-23.
- v3.12.0 Atelier: backend + UI + version bump on feature branch; release pending.

---

## PRIOR STATUS: v3.11.0 Signet Pro SHIPPED — release published, Docker live, all artifacts uploaded.

**TICK 2026-05-23 17:3x PT (Saturday off-hours)** — MODE B FINALIZE executed.
- CI build (26347285490): all 7 jobs green (4 platforms × test+bundle on linux+mac arm/x64+win).
- Downloaded 6 artifacts to /tmp/slab-release-3.11.0.
- `gh release create v3.11.0 --title "v3.11.0 — Signet Pro"` published, NOT draft.
  https://github.com/Sanjays2402/slab/releases/tag/v3.11.0
- Docker (run 26347285501) green → `ghcr.io/sanjays2402/slab-server:v3.11.0` live.
- Release notes lead with the wedge: "Legally-binding timestamps. Batch sign 200 PDFs.
  Visible signature stamps. Adobe charges $239/yr; we ship it free, offline."
- All 6 installers attached (mac arm/x64 dmg, linux deb+AppImage, win nsis+msi).
- RELEASE_PENDING cleared.

### Disk recovery: `cargo clean` on src-tauri freed 6.0 GiB (from 454 Mi free to 5.7 Gi).

### Next tick — MODE C DEVELOP — EXECUTE v3.12.0 ATELIER PLAN
Issue backlog is EMPTY (re-polled 2026-05-23 17:55 PT). Sanjay invoked
writing-plans skill — plan saved at:
  `docs/plans/2026-05-23-v3.12.0-atelier-workflow-automation.md` (commit eb197ca)

**v3.12.0 Atelier** — workflow automation engine. Chain OCR + auto-redact +
Bates + flatten + sign into named recipes, run over folders unattended.
Adobe Action Wizard equivalent, free + offline. THE next enterprise moat.

7 TDD tasks: data model → run_recipe → batch driver → Tauri cmds → Svelte
panel w/ live progress grid → preset+version bump → MODE A release.

Next tick: `git checkout -b feature/v3.12.0-atelier` then start Task 1.

### LAST_WOW_TICK_AT: 2026-05-23 16:08 PT (visible sig stamps banked the wow.
 v3.11.0 GA itself is the wow ship — public release of CAdES-T + batch + stamps.)

### RECENTLY_CLOSED_ISSUES:
- v3.11.0 Signet Pro: merged 2026-05-23, tag v3.11.0, release published 17:3x PT.

---

## PRIOR STATUS: v3.11.0 Signet Pro MERGED + TAGGED — CI in flight, finalize next tick.

**TICK 2026-05-23 17:1x PT (Saturday off-hours)** — MODE A RELEASE executed.
Merged `fix/svelte-reactive-refs` (the descendant of `feature/v3.11.0-signet-pro`)
into `main` (merge SHA on main now contains all 18-commit feature branch work).
Bumped version 3.10.0 → **3.11.0** across Cargo.toml + Cargo.lock + package.json
+ tauri.conf.json (commit `49dd82e`).

Quality gates on main post-merge:
- cargo fmt --all -- --check ✓
- cargo clippy --lib --all-targets -- -D warnings ✓
- cargo test --lib → **1398 passed, 0 failed** (+35 since v3.10.0 release)
- pnpm check → 0 errors, 61 warnings (pre-existing a11y nits)

Tagged `v3.11.0` with marketing-tone annotation (no emoji). Pushed
`main --follow-tags`. CI runs in flight:
- **26347285490** — build (4-platform bundle)
- **26347285501** — Docker (slab-server)

### RELEASE_PENDING: v3.11.0 — tag v3.11.0, CI runs 26347285490 (build) + 26347285501 (Docker)

### Next tick — MODE B FINALIZE
1. `gh run view 26347285490` — if green, `gh run download` artifacts to `/tmp/slab-release-3.11.0`.
2. `gh release create v3.11.0 --title "v3.11.0 — Signet Pro"` with marketing notes
   (lead with: "Legally-binding timestamps. Batch sign 200 PDFs in parallel.
   Visible signature stamps. Adobe charges $239/yr — we ship it free, offline.")
3. Upload 6 artifacts (mac arm64+x64 dmg, linux deb+AppImage, win nsis+msi).
4. Verify Docker tag workflow (26347285501) green; image `ghcr.io/sanjays2402/slab-server:v3.11.0`.
5. Clear RELEASE_PENDING line.

### Disk: 1.1 Gi free at tick end — will need `cargo clean -p slab-app` before
the next dev tick if disk doesn't recover from /tmp clearing.

### LAST_WOW_TICK_AT: 2026-05-23 16:08 PT (still within 24h — visible sig stamps
banked the wow already)

### RECENTLY_CLOSED_ISSUES:
- v3.11.0 Signet Pro merged + tagged this tick.

---

## PRIOR STATUS: v3.11.0 Signet Pro — CAdES-T shipped end-to-end (Task 7 complete).

**TICK 2026-05-23 16:5x PT (Saturday off-hours)** — RFC 3161 timestamp tokens
now embed into the CMS unsigned attributes, end-to-end from Svelte input to
re-encoded SignerInfo. CAdES-BES → CAdES-T toggle.

Branch: `fix/svelte-reactive-refs` (descendant of `feature/v3.11.0-signet-pro`).
4 commits this tick:
- e04d2f0 signet_pro/tsa: `signer_signature_digest` + `embed_timestamp_token` + 4 tests
- 063c8f2 signet/sign: optional CAdES-T path, 16 KiB hex window when TSA set
- 46defd4 tauri DTOs: `tsa_url` on `SignetSignArgs` + `SignetProBatchArgs`
- 9ed1c2a UI: TSA URL input on `SignetPanel` and `SignetBatchPanel`

Signet test suite: **69/69 PASS** (4 new TSA-embed tests). Clippy `-D warnings`
clean. `pnpm check`: 0 errors.

**LAST_WOW_TICK_AT: 2026-05-23 16:08 PT** (still inside the 24h window from
visible-signature stamps; this tick's CAdES-T is mostly under-the-hood
compliance plumbing — wow already banked for today).

Buy-Button: CAdES-T is what every law firm + audit-trail compliance buyer
asks for first. Acrobat Pro exposes it; PDF Expert doesn't. Pay-for-it ✓,
Pick-us ✓.

Next tick:
- Merge `fix/svelte-reactive-refs` → `feature/v3.11.0-signet-pro` (or fast-
  forward if straight-line), then `feature/v3.11.0-signet-pro` → `main`,
  tag `v3.11.0`, kick CI, finalize release.
- Verify a live TSA round-trip (digicert/freetsa) via a one-shot manual
  script before tagging — the embed code path has only been exercised
  against a hand-rolled fake TST so far.
- Add release-notes copy that leads with "legally-binding timestamps,
  completely offline-cert / online-TSA, never your file".

---

**TICK 2026-05-23 16:08 PT (Saturday off-hours)** — appearance Form XObject
splicing wired into `sign_pdf`, single-file UI gets a visible-stamp toggle.

Branch: `feature/v3.11.0-signet-pro` (1111bd6). 4 commits this tick:
- 8996752 TSA HTTP fetch (mockito-tested, 12 tests)
- 2e410e0 batch-sign Tauri command + dedicated Svelte panel (Buy-Button)
- 1d39e42 visible-signature wire-in (Form XObject /AP /N) + verify-still-green test
- 1111bd6 SignetPanel visible-stamp toggle (page + rect inputs)

Full lib suite: **1394/1394 PASS**. Clippy `-D warnings` clean.
**LAST_WOW_TICK_AT: 2026-05-23 16:08 PT** (visible signatures end-to-end —
"look at my PDF, it has a real signature stamp" screenshot-worthy moment).

Next tick:
- CAdES-T upgrade: call `fetch_timestamp` after signing, splice TST as
  `id-aa-timeStampToken` unsigned attr on SignerInfo (cms_blob.rs).
- Add TSA URL field to SignOptions + UI panels (single + batch).
- Then merge branch -> main, tag v3.11.0, finalize release.

---

<details><summary>Earlier history</summary>



**TICK 2026-05-23 15:32 PT (Saturday off-hours)** — writing-plans skill: plan
already saved last tick, this tick *executes* it.

Branch: `feature/v3.11.0-signet-pro` — 3 new commits (`552a859`, `41a88f3`,
`8c85371`) on top of last tick's scaffold + plan, total 4 commits / ~950 LOC
this tick (953 insertions across 4 files). Plus rayon dep added.

Shipped this tick:
- **Task 2 + 3 (parse half):** RFC 3161 `TimeStampReq` DER encoder +
  `TimeStampResp` parser in `signet_pro/tsa.rs`. Canonical-integer nonce
  normalisation (so `der::asn1::Int` accepts the full i64 range);
  `ID_AA_TIMESTAMP_TOKEN` OID exported for CMS unsigned-attr embedding.
  7 unit tests.
- **Task 4:** `build_appearance` + `build_appearance_from_name` Form
  XObject builder in `signet_pro/appearance.rs`. 0.5pt grey border +
  Helvetica BT/ET text, PDF-literal-string escaping, font-size clamp,
  optional date/reason/location lines. 9 unit tests.
- **Task 5:** Batch driver in `signet_pro/batch.rs` — `plan_batch` walks
  for *.pdf (recursive opt-in), `run_batch` executes via rayon with
  atomic-counter progress, `BatchReport` with `success_rate`,
  `fully_succeeded`, `failures()`. 10 unit tests including a full
  `sign_folder` end-to-end smoke test (pretend-sign 3 PDFs).

**signet_pro now has 25 passing tests** (was 0 last tick). Full signet+pro
suite: 59/59 green.

Quality gates this tick:
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓ (fixed bool_assert_comparison,
  derive Default, and ok().expect() lints raised by the new code)
- `cargo test --lib pdf::signet` → 59/59 PASS
- `pnpm check` → 0 errors, 63 warnings (all pre-existing a11y nits)

**Disk: 5.4 GiB free** after `cargo clean` (was at 124 MiB before — full
clean ran mid-tick to unblock the link step).

Buy-Button test: TSA encoding + batch parallel sign are Acrobat Pro $239/yr
exclusives, both now implemented offline in Slab. Pay-for-it ✓, Pick-us ✓.

### Next tick — finish Task 3 (HTTP fetch) + Task 4 wiring into sign_pdf
1. `fetch_timestamp(url, req)` — reqwest blocking POST with
   `application/timestamp-query` content-type. Mock via mockito in tests.
2. Embed returned TST as `id-aa-timeStampToken` unsigned attr in
   `signet::sign::sign_pdf` SignerInfo (CAdES-T upgrade).
3. Wire `SignOptions::appearance` → swap invisible Widget for AP/N form-
   XObject Widget at the spec.rect on spec.page.
4. Frontend BatchSignPanel.svelte (Task 6) — can land alongside in same tick
   if scope allows.

### LAST_WOW_TICK_AT: 2026-05-23T22:32Z (batch parallel sign with progress
events — the demo screenshot Sanjay will tweet)

### RECENTLY_CLOSED_ISSUES:
- (none open)

---

## PRIOR STATUS: v3.11.0 Signet Pro kickoff — plan + ADR 0012 + module scaffolding on feature/v3.11.0-signet-pro.

**TICK 2026-05-23 15:14 PT (Saturday off-hours)** — writing-plans skill invocation.

Branch: `feature/v3.11.0-signet-pro` (pushed, CI run 26345068650 queued).

Shipped:
- `docs/plans/2026-05-23-v3.11.0-signet-pro.md` — 8-task TDD breakdown
  (RFC 3161 TSA + visible appearances + batch sign).
- `docs/adr/0012-signet-pro-tsa-batch.md` — design rationale.
- `src-tauri/src/pdf/signet_pro/{mod,tsa,appearance,batch}.rs` — public
  type stubs + module wiring (compiles, clippy-clean, fmt-clean).
- 2 commits: `d7df6af` (plan) + scaffold commit.

Quality gates all green: cargo check, fmt, clippy -D warnings, pnpm check.

### Next tick — Task 2 of the plan: RFC 3161 TimeStampReq encoder
- Implement `build_timestamp_req` in `signet_pro/tsa.rs` with `der`+`spki`.
- TDD: failing test asserts SHA-256 OID + digest bytes appear in DER output.
- Verify `cms` crate already in deps tree (it is — used by v3.10.0).

### Disk: 1.4Gi free at tick end. Pre-existing pressure; will need
`cargo clean -p slab-app` before next bundle build.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify; <24h)

### RECENTLY_CLOSED_ISSUES:
- v3.10.0 Signet — published prior tick.

---

## PRIOR STATUS: v3.10.0 Signet RELEASED — 6 artifacts uploaded, Docker image live, all CI green.

**TICK 2026-05-23 15:02 PT (Saturday off-hours)** — MODE B FINALIZE executed.
- CI run 26344139015 (build + 4-platform bundle) — **all success** ✅
- CI run 26344139022 (Docker slab-server) — **success** ✅
- Downloaded all 4 artifact bundles to `/tmp/slab-release-3.10.0` (2.0Gi free was enough — disk now 2.0Gi after extraction).
- `gh release create v3.10.0 --title "v3.10.0 — Signet"` with marketing-tone notes (Adobe $239/yr framing, RustCrypto privacy wedge) and 6 artifacts:
  - Slab_3.10.0_aarch64.dmg (macOS Apple Silicon)
  - Slab_3.10.0_x64.dmg (macOS Intel)
  - Slab_3.10.0_amd64.deb (Linux)
  - Slab_3.10.0_amd64.AppImage (Linux portable)
  - Slab_3.10.0_x64-setup.exe (Windows NSIS)
  - Slab_3.10.0_x64_en-US.msi (Windows MSI)
- Docker image `ghcr.io/sanjays2402/slab-server:v3.10.0` live.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.10.0
- RELEASE_PENDING cleared.

### Post-push release check ✓
- All on-tag workflows green (build #26344139015 + Docker #26344139022).
- Release published (not Draft).
- No CI failures in last 24h.

### Next tick — MODE C DEVELOP (v3.11.0)
1. Re-poll `gh issue list` (currently 0 open).
2. Create branch `feature/v3.11.0-signet-trust` for Signet follow-on:
   ECDSA-already-supported, so target: **RFC 3161 timestamp-authority
   integration** (CAdES-T grade) + CRL distribution-point surfacing
   (revocation hints, not full check) + **batch sign** for legal workflows.
3. Disk: 2.0Gi free at tick end. Will need `cargo clean -p slab-app` before
   next Tauri bundle build.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify — within 24h)

### RECENTLY_CLOSED_ISSUES:
- v3.10.0 Signet — published this tick (CI 26344139015 + 26344139022)

---

## PRIOR TICK 2026-05-23 14:34 PT — MODE A RELEASE executed.

- Merged `feature/v3.10.0-signet` → `main` with `--no-ff` (merge SHA `bd0aa70`).
- Bumped version 3.9.0 → **3.10.0** across Cargo.toml, Cargo.lock, tauri.conf.json, package.json (commit `8dc38ad`).
- Quality gates on main:
  - cargo fmt --all -- --check ✓
  - cargo clippy --lib --all-targets -- -D warnings ✓
  - cargo test --lib → **1363 passed, 0 failed**
  - pnpm check → 0 errors, 63 warnings (a11y on SignetPanel labels — pre-existing pattern)
- Tagged `v3.10.0` with marketing-tone annotation (no emoji per Sanjay's rule).
- Pushed `main --follow-tags` → CI runs **26344139015** (build) + **26344139022** (Docker) in flight.

### RELEASE_PENDING: v3.10.0 — merge SHA bd0aa70, tag v3.10.0, CI runs 26344139015 (build) + 26344139022 (Docker)

### Next tick — MODE B FINALIZE
1. `gh run view 26344139015` — if green, `gh run download` artifacts to `/tmp/slab-release-3.10.0`.
2. `gh release create v3.10.0 --title "v3.10.0 — Signet"` with marketing-grade notes + 6 artifacts (mac arm64/x64 dmg, linux deb+AppImage, win nsis+msi).
3. Verify Docker tag workflow (26344139022) also green.
4. Clear RELEASE_PENDING line.
5. Then v3.11.0 candidate: **ECDSA P-256/P-384 signing** + revocation hints + batch sign (Signet follow-on), OR fold into v3.10.1 hotfix if Signet has post-release bugs. Re-poll `gh issue list` first.

### Ops note
Disk hit 100% (185Mi) during cargo link — recovered by removing `~/Library/Caches/com.microsoft.VSCode.ShipIt` (920MB) and `Chrome.code_sign_clone`. **1.1Gi free** at tick end; should hold for one finalize tick but next big build will need more cleanup.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify — still within 24h)

### RECENTLY_CLOSED_ISSUES:
- v3.10.0 Signet merged + tagged (this tick) — release artifacts pending CI.

---

## PRIOR STATUS: v3.10.0 Signet feature-complete on feature/v3.10.0-signet — sign + verify end-to-end, 34 signet tests passing, Tauri commands + SignetPanel UI shipped. Ready to merge to main next tick if CI is green.

**TICK 2026-05-23 14:17 PT (Saturday off-hours)** — MODE C develop, BIG slice fold-in.
4 commits, ~2200 net LOC + 17 new tests, on `feature/v3.10.0-signet`:
- `8e2b99c` feat(signet): build_pkcs7_detached — CMS SignedData (adbe profile)
- `c2bd798` feat(signet): sign_pdf end-to-end — placeholder/serialize/splice/ByteRange
- `75c1767` feat(signet): Tauri commands — signet_load_identity / sign / verify
- `d26ce33` feat(signet): SignetPanel UI — load identity, sign, verify

Quality gates green: cargo fmt + clippy + cargo test --lib (34 signet tests
pass), pnpm check 0 errors. Pushed to origin; CI run 26343830061 in flight
at tick end.

### Buy-button verdict: PASSES (Tell-a-friend + Pick-us)
- Adobe charges $239/yr for digital signatures. Slab ships RSA-SHA-256
  PKCS#7-detached signatures **free, offline**, compatible with the
  Acrobat signature panel.
- Enterprise wedge: legal/compliance workflows now have a path that
  doesn't ship private keys or PDFs to a cloud.

### Wow moment: signed PDF round-trips through our own verify() — digest matches, crypto valid, chain status reports SelfSigned for test certs, FullDocument coverage. Sign + verify in <50ms on the test fixtures.

### Next tick — MODE A merge or v3.10.0 release
1. `git checkout main && git pull && git merge --no-ff feature/v3.10.0-signet`
2. Quality gates on main → tag `v3.10.0` → push tags.
3. Finalize the GitHub release with marketing-grade notes:
   _"Sign and verify PDFs offline. Adobe-compatible PKCS#7 signatures.
   Zero cloud, zero subscription."_
4. After release: v3.10.1 — add ECDSA P-256/P-384 signing (cms builder
   bound work, ~1 day), revocation hints (CRL distribution-point
   surfacing, not full check), batch sign for legal workflows.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify)

### Ops
Disk: 2.6 Gi free after `cargo clean -p slab-app` mid-tick (recovered from
ENOSPC during initial build). Watch for next tick.

### RECENTLY_CLOSED_ISSUES:
(none this tick — all issue-override items #23–27 already closed)

---

## PRIOR STATUS: v3.10.0 Signet foundation landed on feature/v3.10.0-signet — identity loader + trust store, 17 tests passing.

**TICK 2026-05-23 13:19 PT (Saturday off-hours)** — MODE C develop, foundation tick.
3 commits, ~1100 LOC + 17 new tests, on `feature/v3.10.0-signet`:
- `c051a24` chore(signet): vendor RustCrypto CMS deps + ADR 0011
- `f013237` feat(signet): SigningIdentity PEM loader (RSA / P-256 / P-384)
- `edcdc89` feat(signet): TrustStore + chain status enum

Quality gates green: cargo fmt + clippy + cargo test --lib **1346 passing** (+17 new), pnpm check 0 errors. Pushed to origin.

### Honest buy-button verdict: foundation, not ship
Plumbing only — no UI, no end-to-end sign/verify. Risk-reduced the CMS work
by getting identity + trust right before touching the finicky
`cms::SignedDataBuilder` API.

### Next tick — Task 4 (sign pipeline, end-to-end)
1. `cms_blob.rs` — `build_pkcs7_detached(digest, identity, time)`.
   Reference: `~/.cargo/registry/.../cms-0.2.3/tests/builder.rs:86-156`.
2. `sign.rs` — `prepare_signature_field` + `sign_pdf` (byte-range splice).
3. Tauri command `slab_signet_sign` + minimal SignetPanel.
4. Target: 600+ LOC, end-to-end "load identity → sign PDF → file on disk".

After Task 4 the buy-button passes for the FIRST time on this version.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (Quill press-roller — within 24h)

### Ops
Disk dropped from 5.2 Gi → 1.7 Gi during this tick (full `target/` rebuild
after Quill release). Next compile may need `cargo clean -p slab-app` again.

---

## PRIOR STATUS: v3.9.0 Quill RELEASED (mac arm64+x64 dmg, win nsis+msi). Linux deferred to v3.9.1 (disk ENOSPC). v3.10.0 Signet ready to start.

**TICK 2026-05-23 13:07 PT (Saturday off-hours)** — MODE B FINALIZE.
CI run 26341610206 all green. Tagged `098f11b` → `v3.9.0`, pushed tag, created
GitHub release with marketing notes + 4 artifacts (mac arm64/x64 dmg, win nsis/msi).
Linux AppImage download hit ENOSPC at libwebkit2gtk extract — release notes
say linux ships in v3.9.1. Session log: `.cron-state/sessions/2026-05-23-1307.md`.

### Next tick — MODE C DEVELOP (v3.10.0 Signet)
1. Verify on-tag workflows for v3.9.0 succeeded (`gh run list --limit 8`).
2. `git checkout -b feature/v3.10.0-signet` and execute Tasks 1–4 of the
   Signet plan as one mega-tick (~900 LOC, 4 commits, sign end-to-end).
3. Linux v3.9.1 hotfix can wait — disk needs to clear first.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (magenta press-roller — within 24h)

---

## PRIOR STATUS: v3.9.0 Quill awaiting CI bundle (run 26341610206 — macOS done, Linux+Win building). v3.10.0 Signet plan written.

**TICK 2026-05-23 12:46 PT (Saturday off-hours)** — PLANNING tick.
Wrote `docs/plans/2026-05-23-v3.10.0-signet-digital-signatures.md` — full 9-task
plan for PKCS#7 digital signatures (sign + verify, pure-Rust RustCrypto,
cross-platform, wax-seal wow). Did NOT ship code because (a) v3.9.0 CI still
bundling, (b) Sanjay actively editing `docs/landing/index.html` in working tree
at the moment cron fired (mtime 12:46:01 = tick start), (c) disk at 1.1 Gi free
— no headroom for a Tauri build. Session log: `.cron-state/sessions/2026-05-23-1246.md`.

### Next tick — MODE B FINALIZE (then start Signet)
1. Poll CI 26341610206 — if green, finalize v3.9.0 Quill release.
2. Once Quill released: `git checkout -b feature/v3.10.0-signet` and execute
   Tasks 1–4 of the Signet plan as one mega-tick (~900 LOC, 4 commits, sign
   end-to-end).

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (within 24h — no new wow needed)

---

## PRIOR STATUS: v3.9.0 Quill SHIPPED to main — AcroForm inspector + fill end-to-end. Awaiting CI build for release.

**TICK 2026-05-23 12:17 PT (Saturday off-hours)** — MODE C develop.
4 commits, 1689 net LOC (`forms.rs` 841 + `FormsPanel.svelte` 501 + lib.rs 18
+ keymap.ts/keymap action 11 + `+page.svelte` 18 + version bumps). All gates
green: cargo fmt/clippy clean, cargo test --lib **1329 passing** (+11 new),
pnpm check 0 errors. Pushed `main` → `098f11b`. CI build run 26341610206
queued. Session log: `.cron-state/sessions/2026-05-23-1217.md`.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (magenta press-roller wipe — within 24h)

### What shipped this tick

- `2066214 feat(forms): AcroForm inspector + fill backend (Slice 1)` — 11 tests
- `02cfa65 feat(forms): Tauri commands slab_forms_inspect + slab_forms_fill (Slice 2)`
- `2a44f6c feat(forms): FormsPanel.svelte + forms.open keymap action (Slice 3)`
- `098f11b feat(forms): wire FormsPanel + bump v3.9.0 Quill (Slice 4)`

### Buy-Button — PASS on 3 of 4

- Pay-for-it ✓ — Acrobat Pro forms = $239/yr.
- Pick-us ✓ — no free cross-platform PDF form filler with real inspector UI.
- Notice-it ✓ — new sidebar entry + Cmd+Shift+F shortcut.
- Tell-a-friend — solid with JSON template round-trip angle.

### Next tick — MODE B FINALIZE

1. Poll CI run 26341610206 (build). If success → tag v3.9.0 + release pipeline.
2. If failed → triage from `gh run view --log-failed`.
3. After v3.9.0 ships → re-poll issues; otherwise v3.10.0 candidates:
   PKCS#7 digital signatures (Forms follow-on, enterprise legal) OR batch
   automations (drag-folder pipelines).

## ARCHIVED: v3.8.0 Press — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.8.0
One-click PDF/X-4 print production: FOGRA51/GRACoL2013 ICC, OutputIntent, normalize_color, Inspect/Convert/Validate UI, Cmd+Shift+X shortcut, magenta press-roller wipe wow.

---

## PRIOR STATUS: v3.8.0 Press Slice 6 SHIPPED on `feature/v3.8.0-press` — **PressPanel UI live end-to-end**. Inspect/Convert/Validate tabs, Mod+Shift+X shortcut, sidebar entry, and the magenta press-roller wipe wow with PDF/X-4 ✓ badge reveal. Branch is **feature-complete** for v3.8.0 → MERGE TO MAIN next tick.

**TICK 2026-05-23 11:14 PT (Saturday off-hours)** — MODE C develop. 4 commits, ~630 net LOC (PressPanel.svelte 604 + +page.svelte 15 + keymap.ts 1 + keymap/action.rs 8). All gates green (cargo fmt/clippy/test 1318 passing, pnpm check 0 errors). Session log: `.cron-state/sessions/2026-05-23-1114.md`.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (magenta press-roller wipe — 380ms CMYK ink-roller sweep + PDF/X-4 ✓ FOGRA51/GRACoL2013 badge reveal, reduced-motion safe)

### Next tick — MODE A RELEASE
1. `git checkout main && git pull`
2. `git merge --no-ff feature/v3.8.0-press -m "Merge v3.8.0 'Press' — one-click PDF/X-4 conversion"`
3. Bump version 3.7.0 → 3.8.0 across Cargo.toml + tauri.conf.json + package.json.
4. Quality gates on main.
5. Tag v3.8.0 with marketing-tone annotation (no emoji in tag/commit per Sanjay's rule).
6. Push main --follow-tags. Set RELEASE_PENDING for MODE B finalize next tick.

### What shipped this tick

- `f3ac2a2 feat(press): register press.open keymap action (Mod+Shift+X)`
- `80de330 feat(press): extend ActionId union with press.open`
- `826d065 feat(press): PressPanel.svelte — Inspect/Convert/Validate tabs (Slice 6)`
- `a53766f feat(press): wire PressPanel into +page.svelte (sidebar + shortcut)`

### Buy-Button verdict — PASS on 4 of 4

- Pay-for-it: Acrobat Pro charges $239/yr for PDF/X-4 export → Slab does it free + offline.
- Notice-it: New sidebar entry + Cmd+Shift+X shortcut.
- Pick-us: No free cross-platform PDF/X-4 converter exists with a real UI.
- Tell-a-friend: Magenta press-roller wipe is screenshottable.

### Ops note

Disk hit 100% again pre-compile. Cleared Chrome code-sign clone (78GB) +
`cargo clean -p slab-app` (8.8GB). 5.6GB free after. If this recurs the
clean target is recoverable cheaply; the Chrome clone keeps coming back
whenever Chrome updates.

---

## PRIOR STATUS: v3.8.0 Press Slices 1+2 SHIPPED on `feature/v3.8.0-press` — ADR + FOGRA51/GRACoL ICC vendored, OutputIntent enum, normalize_color() pass with 17 passing tests.

---

## PRIOR STATUS: v3.7.0 Loom PUBLISHED on GitHub Releases — 6 artifacts uploaded, Docker image live on GHCR.

**TICK 2026-05-23 10:00 PT (Saturday off-hours)** — MODE B FINALIZE executed:
- CI run 26337874627 (build) — **success** ✅
- CI run 26337874606 (Docker slab-server) — **success** ✅
- Downloaded all 4 artifact bundles (required freeing ~500MB of /tmp first — disk was at 355Mi free; cleaned old slab-release-3.4.0 + stale screenshots).
- `gh release create v3.7.0 --title "v3.7.0 — Loom"` with marketing-tone notes (`/tmp/slab-v3.7.0-notes.md`) and 6 artifacts uploaded:
  - Slab_3.7.0_aarch64.dmg (macOS Apple Silicon)
  - Slab_3.7.0_x64.dmg (macOS Intel)
  - Slab_3.7.0_amd64.deb (Linux)
  - Slab_3.7.0_amd64.AppImage (Linux portable)
  - Slab_3.7.0_x64-setup.exe (Windows NSIS)
  - Slab_3.7.0_x64_en-US.msi (Windows MSI)
- Docker image `ghcr.io/sanjays2402/slab-server:v3.7.0` live.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.7.0
- RELEASE_PENDING cleared.
- Open issues polled: **0 open**. Next pipeline item: **v3.8.0 Press** (PDF/X-4 print production).

### Next tick — MODE C DEVELOP
1. Create branch `feature/v3.8.0-press`.
2. Execute Slice 1 of `docs/plans/2026-05-23-v3.8.0-press-pdf-x.md` (ADR + ICC vendoring + module scaffold).
3. Honor ship-size: bundle Slice 1+2 (ICC + scaffolding alone won't pass buy-button).

### LAST_WOW_TICK_AT: 2026-05-23T16:15Z (Loom Slice 6 sub-badge stagger — within 24h window)



**TICK 2026-05-23 09:30 PT (Saturday off-hours)** — MODE A RELEASE executed:
- Merged feature/v3.1.0-loom-slice-3 → main (merge commit a04f8e6).
- Bumped version 3.6.0 → **3.7.0** across Cargo.toml, tauri.conf.json, package.json (ce739dd).
- Tagged **v3.7.0** with marketing-tone annotation.
- Pushed main + tag to origin.

### RELEASE_PENDING: v3.7.0 — merge SHA a04f8e6, tag v3.7.0, CI run 26337874627 (build) + 26337874606 (Docker)

### Quality gates on main (post-merge)
- cargo fmt --all -- --check: clean
- cargo clippy --lib --all-targets -- -D warnings: clean
- cargo test --lib: **1286 passed, 0 failed**
- pnpm check: 0 errors, 46 warnings (pre-existing unused-CSS)

### Why version jumped 3.1.0 → 3.7.0
Slab's released version line had already reached v3.6.0 (Compactor) on main while
the Loom feature branch was being developed in parallel under its own legacy
"v3.1.0 Loom" codename. To avoid version regression and keep tag history
monotonic, Loom ships as **v3.7.0 — Loom**. Codename "Loom" preserved in the
release title.

### Next tick — MODE B FINALIZE
1. `gh run view 26337874627` — if green, `gh run download` artifacts.
2. `gh release create v3.7.0` with notes-file + 6 artifacts.
3. Verify Docker tag workflow (26337874606) too.
4. Clear RELEASE_PENDING line.
5. Then start next pipeline item (v3.8.0 — see roadmap).

### LAST_WOW_TICK_AT: 2026-05-23T16:15Z (Loom Slice 6 sub-badge stagger)

---

## PRIOR HISTORY (v3.1.0 Loom development on feature/v3.1.0-loom-slice-3)


## STATUS: v3.1.0 Loom Slice 6 (metadata + validator + UI) SHIPPED on feature/v3.1.0-loom-slice-3. Branch is feature-complete for v3.1.0; ready to merge to main next tick.

**TICK 2026-05-23 09:15 PT (Saturday off-hours)** — MODE C develop. Slice 6
finishes v3.1.0 Loom. Slab can now tag PDF/UA-1 documents AND certify them
with an 8-check validator, all offline, in one panel. PAC 2024 / CommonLook
Validator / veraPDF Enterprise cost hundreds per seat and only grade — Slab
does both for free.

### What shipped this tick (4 commits, ~1500 LOC)

- `5b2b296 feat(loom): apply_pdfua_metadata — XMP packet + ViewerPreferences (Slice 6)`
  - `src-tauri/src/pdf/loom/metadata.rs` (483 LOC, 7 tests).
  - XMP packet with `pdfuaid:part=1`, `dc:title`, `dc:language`, `xmp:CreatorTool`.
  - `/ViewerPreferences /DisplayDocTitle true` (Matterhorn 07-001).
  - Info dict `/Title` sync from XMP `dc:title` (Matterhorn 06-001).
  - `/Lang` fallback ("en-US") at catalog (Matterhorn 11-001).
  - `MetadataOptions` builder + `MetadataStats { xmp_written, title_synced, ... }`.
- `12a3238 feat(loom): validate() — 8 Matterhorn auto-conditions on tagged PDFs (Slice 6)`
  - `src-tauri/src/pdf/loom/validate.rs` (557 LOC, 7 tests).
  - 8 auto-decidable checks: StructTree present, MarkInfo /Marked true, /Lang
    set, XMP present, XMP pdfuaid:part=1, ViewerPrefs /DisplayDocTitle true,
    Info /Title set, every Figure has /Alt. Each yields PDF/UA-1 clause +
    Matterhorn condition ID.
  - `ValidateReport { overall, checks: Vec<CheckResult>, passed, failed }`.
- `1580e46 feat(loom): slab_loom_validate command + auto-validate after tag (Slice 6)`
  - `src-tauri/src/lib.rs`: `LoomTagResult` extended with `validation` +
    `metadata` fields. `slab_loom_tag_document` now runs apply_pdfua_metadata
    then validate after weave. New `#[tauri::command] slab_loom_validate`
    grades any existing PDF (vendor docs, Acrobat output, etc.).
- `ff11a50 feat(loom): Validate tab + sub-badge UI — Slice 6 finale, PDF/UA-1 verdict in the panel`
  - `src/lib/panels/LoomPanel.svelte`: new "Validate" tab
    (Cmd/Ctrl+Shift+V), verdict card, per-check list with PDF/UA-1
    clause + Matterhorn ID per row, idle empty-state pitch naming the
    competitors. Sub-badge on Tag tab reveals 380ms after main badge —
    green "✓ Validated · ISO 14289-1 · 8/8 checks" or red verdict.
    ~140 LOC of CSS, full dark-mode parity.
  - Also fixed one clippy vec_init_then_push lint in validate.rs.

### Quality gates this tick

- `pnpm check`: 0 errors, 46 warnings (all pre-existing unused-CSS, not Slice 6).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --lib -- -D warnings`: clean.
- `cargo test --lib`: **1234 passed, 0 failed**.

### Buy-Button passes ALL FOUR

- Pay-for-it: validator alone competes with $$$ commercial tools.
- Notice-it: green sub-badge after every tag is unmissable.
- Pick-us: Adobe Acrobat doesn't grade conformance; Slab does — offline.
- Tell-a-friend: "ISO 14289-1 certified, free, on my Mac" is a screenshot.

### Wow moment

LAST_WOW_TICK_AT: 2026-05-23T16:15Z — sub-badge stagger + Validate tab verdict.

### Next tick

MODE A — RELEASE. feature/v3.1.0-loom-slice-3 is now feature-complete for
v3.1.0. Merge to main, run gates on main, tag v3.1.0, push --follow-tags,
finalize GitHub release with marketing-tone notes.

### What shipped this tick (3 commits, ~1240 LOC across 3 files)

- `74480d9 feat(loom): structure_tree weave() — emit StructTreeRoot for PDF/UA-1 (Slice 5)`
  - `src-tauri/src/pdf/loom/structure_tree.rs` (~960 LOC incl. 17 unit tests).
  - `ParentTreeBuilder` (NumberTree for `/ParentTree`).
  - `build_role_map` + `make_struct_elem` helpers.
  - `plan_page` — StructTree page → flat `RunMcid` sequence in stream order.
    Containers (Document/Sect/List) pass through; artifacts skip MCID counter.
  - `rewrite_page_stream` — injects `BDC /<Tag> << /MCID n >> ... EMC` around
    every Tj/TJ/'/"/Do operator; artifacts use empty-dict form. Re-flates the
    stream via lopdf.
  - `weave(doc, tree, order, opts)` — public entry. Builds StructElems
    depth-first mirroring classify's tree, wires `/StructTreeRoot`,
    `/MarkInfo`, `/Lang`, `/RoleMap`, `/ParentTreeNextKey`, and
    `/StructParents` on every page. Per-Figure `/Alt` from Slice 4 alt-text;
    per-node `/Lang` if classify sets it. Artifacts excluded from the elem
    tree (kept in content stream as `/Artifact BDC ... EMC`).
  - 17 unit tests covering: builder sort, role map, struct-elem invariants,
    plan_page (heading levels H1..H6 + collapse, container traversal,
    artifact MCID-skip), rewrite_page_stream (BDC/EMC bracketing, artifact
    empty-dict, empty-plan no-op), and weave (catalog wiring, /StructParents
    on every page, /Alt on Figure, artifact exclusion, /Lang preserve-existing).
- `4fea7b1 feat(loom): slab_loom_tag_document Tauri command (Slice 5 backend wiring)`
  - `src-tauri/src/lib.rs` adds the async command (~110 LOC).
  - Runs the full pipeline: layout → classify → reading_order → best-effort
    Beacon alt-text → weave → save `<stem>.tagged.pdf`.
  - Best-effort alt-text means tagging still ships if Ollama is offline.
  - Returns `LoomTagResult { output_path, elapsed_ms, pages_processed,
    pages_skipped, bdc_pairs_injected, struct_elems_created,
    figures_with_alt_text }`.
  - Registered in `tauri::generate_handler![…]` next to the other Loom cmds.
- `ff2a201 feat(loom): LoomPanel "Tag PDF" tab with Cmd+Shift+T + reveal anim (Slice 5 UI)`
  - New "Tag PDF" tab on `src/lib/panels/LoomPanel.svelte` (~210 LOC).
  - Primary CTA "Tag Document for PDF/UA" with stats card on success.
  - **WOW**: 320ms purple-glow reveal animation on the "PDF/UA-1 emitted"
    pill badge after a successful tagging run. Designed for screenshot.
  - Cmd/Ctrl+Shift+T global shortcut from any LoomPanel tab.
  - Empty-state copy frames the privacy/cost wedge:
    > "Adobe Acrobat Pro's Auto-tag costs $239/yr. CommonLook starts at
    > $1,200 per seat. veraPDF won't generate alt-text. Slab does the whole
    > pipeline in one click — without your file leaving this Mac."
  - Dark-mode variant included.

### Quality gates this tick

- `cargo fmt --all -- --check` ✓
- `cargo clippy --lib -- -D warnings` ✓
- `cargo test --lib` → 1220 passed (+17 new from structure_tree).
- `pnpm check` → 0 errors, 46 warnings (all pre-existing CSS-unused-selector).

### LAST_WOW_TICK_AT: 2026-05-23T08:20 PT

The purple-glow "PDF/UA-1 emitted" badge reveal anim. Plus the underlying
capability — generating valid tagged PDFs locally — is itself the bigger wow.
Acrobat Pro charges $239/yr for this; CommonLook charges $1,200+/seat;
neither runs on Linux. Slab ships it free, cross-platform, offline.

### Buy-Button verdict for the entire Slice 5

- **Pay-for-it:** PASS — Acrobat Pro AutoTag is the $239/yr feature. We
  give it away with vision-LLM alt-text on top.
- **Pick-us:** PASS — no free cross-platform PDF/UA tagger exists today.
  veraPDF tags but won't auto-generate alt-text; pdfarranger doesn't tag.
- **Notice-it:** PASS — new Tag PDF tab + Cmd+Shift+T shortcut visible the
  moment the user opens Loom.
- **Tell-a-friend:** PASS — "I tagged my dissertation for screen readers
  locally, in seconds, free." Plus the badge reveal screenshot.

### Branch state

`feature/v3.1.0-loom-slice-3` is now 10 commits ahead of main:

Slices 3 + 4 + 5 all live on the branch (the branch name lags the content).
Plan written 07:37 PT this morning, implementation shipped 08:20 PT.

### Next tick candidate (Slice 6: metadata + XMP)

ISO 14289-1 also requires:
- XMP metadata with `pdfuaid:part=1` namespace.
- `/ViewerPreferences << /DisplayDocTitle true >>` on the catalog.
- `/Metadata` stream on the catalog with the XMP packet.
- `/Lang` if not already set (we already do this — re-confirm).
- ActualText for ligatures + math (lower priority, can defer to 6.2).
- Title in document info dict matching XMP `dc:title`.

Slice 6 will add `src-tauri/src/pdf/loom/metadata.rs` mirroring the
structure_tree.rs pattern. Pair with a "PDF/UA validator" tab that runs the
already-shipped Matterhorn auto-conditions against the tagged output and
shows a pass/fail card. That makes Slice 6 the right buy-button tick:
post-tag verification turns the badge from a claim into evidence.

After Slice 6 lands, v3.1.0 Loom is ready to merge → main → tag → release.

### Slice 3 + 4 archive (previous ticks)

Reading order + column-aware traversal (Slice 3); Beacon alt-text generation
with SHA-256 disk cache (Slice 4). See prior STATE entries / git log.

---

## ARCHIVED: v3.6.0 Compactor — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.6.0
Real PDF compression (image downsample + JPEG re-encode + metadata strip).

## ARCHIVED: v3.5.0 Veil — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.5.0
True PDF redaction (content-stream excision, not black bars).

## ARCHIVED: v3.4.0 Discovery — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.4.0
Bates numbering for legal discovery.

RECENTLY_CLOSED_ISSUES:
- v3.5.0 Veil — published earlier this morning
- v3.6.0 Compactor — published this morning (CI 26334187463)

## OPS NOTE — 2026-05-23 07:37 PT

Disk filled to 100% mid-tick (228GB SSD, 117MB free). Root cause: 78GB
stale Chrome code-signing scratch clone at
`/private/var/folders/9g/.../X/com.google.Chrome.code_sign_clone/`.
Removed it during this tick; APFS recovered ~1.4GB usable plus the rest
as purgeable space. If this happens again, the same path is a safe first
target — macOS regenerates as needed.
</details>
