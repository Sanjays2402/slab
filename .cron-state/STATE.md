# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: ✅ v3.24.0 'Stack Pro' PUBLISHED (2026-05-24 16:45 PT)

**TICK 2026-05-24 16:45 PT (Sunday off-hours)** — MODE B FINALIZE complete.

- Polled CI `26375713797` (build on main) → ✅ success (all 4 bundles + 3 cargo tests).
- Docker `slab-server` on v3.24.0 tag → ✅ success earlier (`26375713807`).
- Downloaded 6 artifacts to `/tmp/slab-release-v3.24.0`.
- `gh release create v3.24.0 --title "v3.24.0 — Stack Pro" --notes-file docs/release-notes/v3.24.0.md <6 assets>`.
- Verified `isDraft=false`, asset_count=6.
- Release live: https://github.com/Sanjays2402/slab/releases/tag/v3.24.0

### LAST_WOW_TICK_AT: 2026-05-24T23:26:19Z (shareable redline PDF, prior tick)

### Next tick (MODE C — start v3.25.0)

Candidates: visual pixel diff (complements text 3-way Stack Pro), Beacon AI v0.10.0
local-first chat-with-PDF (buyer magnet), or Foundry plugin marketplace UI.
Recommend Beacon — biggest buy-button win that's still uncut.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 through **v3.24.0** all published. No open issues.

---

## (previous status, kept for history)

### STATUS: ✅ v3.24.0 'Stack Pro' SHIPPED to main + tag pushed (2026-05-24 16:26 PT)

**TICK 2026-05-24 16:26 PT (Sunday off-hours)** — MODE A RELEASE.
Finished Task 6 (shareable redline PDF export) + Task 7 (merge + tag + push).

### Tick stats

- 2 commits on `feature/v3.24.0-stack-pro-three-way`:
  - `70917f8` feat(diff3): bake three-way redline into shareable PDF
  - `6b2e5f7` chore(release): v3.24.0 — version bumps + release notes
- Merged into `main` as `c651cd8` (merge commit, --no-ff).
- Tag `v3.24.0` created and pushed.
- Gates: cargo fmt clean, clippy --lib clean, 6/6 new stack_diff3_export
  tests green, pnpm check 0 errors / 72 a11y warnings (pre-existing).
- CI status: 3 workflows queued on `main` push + tag-triggered Docker
  build queued on `v3.24.0`. Release page not yet populated (CI runs
  ~5min). **Next tick: poll CI, verify v3.24.0 release artifacts land
  on https://github.com/Sanjays2402/slab/releases/tag/v3.24.0.**

### What shipped (v3.24.0 capstone)

- Three-way PDF compare (base/mine/theirs) with conflict resolution
- Materialized merged PDF (preview + bake)
- **Shareable redline PDF export** — landscape 3-column color-coded
  PDF using standard-14 fonts. Recipient does NOT need Slab installed.
- "Export redline PDF" button in Diff3Panel merge-actions toolbar.
- New Tauri command: `slab_diff3_export_pdf`.
- CHANGELOG + docs/release-notes/v3.24.0.md.

### Buy-Button verdict

✅ PASS — Litera Compare ($400/seat/yr) shipped free + offline,
plus the export PDF feature makes this _shareable_ with non-Slab users.
Acrobat doesn't ship 3-way at all.

### LAST_WOW_TICK_AT: 2026-05-24T23:26:19Z

Wow = shareable redline PDF that opens in any viewer without Slab.

### Next tick recommendation

1. Poll CI for v3.24.0 (`gh run list --limit 8`). If green and release
   draft auto-published, write `gh release edit v3.24.0 --notes-file
   docs/release-notes/v3.24.0.md`.
2. If CI failed → triage. Tag workflows don't re-run on main pushes,
   so a CI failure on `v3.24.0` tag = re-cut as v3.24.1.
3. After v3.24.0 is fully released, start v3.25.0 planning. Candidates:
   - Beacon AI / Ollama selection-actions polish
   - Visual diff (image-level pixel compare, complements text 3-way)
   - Plugin marketplace UI (Foundry Slice 9)

---

## (previous status, kept for history)

### STATUS: 🔧 v3.24.0 'Stack Pro' — branch ready to merge (2026-05-24 15:54 PT)

**TICK 2026-05-24 15:54 PT (Sunday off-hours)** — MODE C DEVELOP, second
tick on the v3.24.0 plan. Disk was full at start (213 MiB free, 11 GiB
target/) — `cargo clean` recovered 12.9 GiB before build could run, then
shipped the rest of Task 5's down-payment end-to-end.

### Tick stats

- Branch: `feature/v3.24.0-stack-pro-three-way`
- **5 commits ahead of main** · ~1397 net LOC across 7 files
- Commits this tick:
  - `19b4f84` feat(diff3): materialize_merged_text + slab_diff3_materialize command
  - `7bda39d` feat(diff3): merged-text preview + clipboard export in Diff3Panel
- Prior tick (still on branch, not yet merged):
  - `b21bb34` feat(diff3): ThreeWayDiff data model + three_way_diff merge engine
  - `81b3b89` feat(diff3): slab_diff3_pdfs Tauri command
  - `8aca250` feat(diff3): Diff3Panel + sidebar nav + Cmd+Shift+3 + palette + shortcuts
- Gates: cargo fmt + clippy + 1691 lib tests green, pnpm check 0 errors.
- Buy-Button: ✅ Litera Compare $400/yr feature, plus the merged-text
  preview + clipboard export = users can produce a clean merged document
  TODAY (paste into Word) before the PDF exporter ships.
- Branch pushed to origin.

### What's working end-to-end

1. Cmd/Ctrl+Shift+3 (or command palette → "Compare 3-way") opens the
   Diff3 panel.
2. Pick base/mine/theirs PDFs.
3. "Run three-way compare" classifies every base line (unchanged /
   mine-only / theirs-only / both-agree / conflict).
4. For each conflict, click Keep Mine or Keep Theirs.
5. "Build merged preview" calls `slab_diff3_materialize` → renders the
   merged document per page, source-colour-coded.
6. "Copy merged text" → clipboard plain text with page separators.

### Next tick

MODE A — merge `feature/v3.24.0-stack-pro-three-way` into main.
Quality gates green on the branch already. After merge, optionally cut
v3.24.0 tag + release (the plan's Task 6/7 covers a true PDF exporter
using md2pdf::render on the materialized text — that can be a v3.24.1
follow-up if we want to ship sooner).

OR: stay on the branch one more tick to finish the real PDF exporter
(Task 5 final form: write `merged.pdf` to disk via md2pdf), then merge
+ tag v3.24.0 with the full Litera Compare killer feature set.

Recommendation: ship the PDF exporter next tick, then merge as one
solid v3.24.0 release. The clipboard export is a nice down-payment but
the screenshot/demo will land on the "Export merged PDF" button.

---

## STATUS: 📋 v3.24.0 'Stack Pro' plan written (2026-05-24 15:27 PT)

**TICK 2026-05-24 15:27 PT (Sunday off-hours)** — Sanjay invoked the
`writing-plans` skill explicitly, so this tick is plan-authoring, not
shipping. Wrote `docs/plans/2026-05-24-v3.24.0-stack-pro-three-way.md`
(984 lines, 7 tasks, end-to-end Stack Pro three-way compare).

- Commit `e47d835` on `main`, pushed.
- Plan extends v3.23.0 diff stack: `pdf::diff3` module + merge engine,
  Tauri command, 3-column Svelte panel, `Cmd+Shift+3`, shareable PDF
  exporter, release wiring.
- Buy-Button: Litera Compare = $400/seat/yr for this exact feature.
  Acrobat/PDF Expert/Foxit ship 2-way only. PASS on all four tests.

### Next tick

MODE C DEVELOP — start executing the plan (Task 1: `ThreeWayDiff` types).
Cut branch `feature/v3.24.0-stack-pro-three-way`. Target ≥4 tasks/600 LOC
in one tick by bundling Tasks 1+2+3 (data model + engine + Tauri command).

---

## STATUS: ✅ v3.23.0 'Stack' PUBLISHED (2026-05-24 15:15 PT)

**TICK 2026-05-24 15:15 PT (Sunday off-hours)** — MODE B FINALIZE.

CI run 26373664021 went green (all 4 bundle jobs + 3 cargo tests).
Downloaded 6 artifacts, created GitHub release v3.23.0 with the
marketing notes from `docs/releases/v3.23.0.md`. Release live:
https://github.com/Sanjays2402/slab/releases/tag/v3.23.0

`RELEASE_PENDING` cleared. Docker (slab-server) on tag v3.23.0 also
went green earlier.

### Next tick

MODE C DEVELOP — v3.24.0. Two strong candidates:
- **v3.24.0 "Stack Pro"** — three-way compare (mine/theirs/base),
  branch-style for legal/dev teams. Extends Stack moat.
- **v0.10.0 "Beacon"** — local-first AI (chat with PDF, summarise).
  The buyer-magnet from the original roadmap; still uncut.

Beacon is the bigger buy-button win. Recommend starting the v0.10.0
spec next tick, then a Tick 1 of the implementation plan.

---

## STATUS: 🚀 v3.23.0 'Stack' MERGED + TAGGED (2026-05-24 14:48 PT)

**TICK 2026-05-24 14:48 PT (Sunday off-hours)** — MODE C → MODE A in
one tick. Shipped Task 7 (the buyer-magnet) + 9 + 10 from the v3.23.0
plan: shareable redline PDF export, version bump, merge to main, tag.

### Tick stats

- 3 commits on `feature/v3.23.0-stack-visual-diff`:
  - `c4867e4` feat(diff): stack_redline — export shareable redline PDF
    (new module, 4 unit tests, Tauri command)
  - `6d1f87c` feat(ui): Export Redline button + palette entry in Diff panel
  - `ccd369e` chore(release): v3.23.0 Stack — bump versions + release notes
- Merge commit on main: `59e478b` "Merge v3.23.0 'Stack' — word-level
  redline + shareable redline PDF export"
- Tag: `v3.23.0` (annotated, professional no-emoji message)
- LOC net: ~1100 (stack_redline.rs ~660 + lib.rs cmd + DiffPanel/Palette UI + release notes ~95)
- Tests: 1680 passed (was 1676 → +4 new redline tests). cargo fmt
  clean, clippy -D warnings clean, pnpm check 0 errors.
- Buy-Button: PASS on all four:
  1. Pay-for-it: Litera Compare charges $400/seat/yr for this exact
     feature (export shareable redline). Acrobat Pro Compare $239.
  2. Notice-it: New button in Diff panel + new palette entry.
  3. Pick-us: Acrobat doesn't ship the redline as a baked PDF
     (annotations only). We do.
  4. Tell-a-friend: "Slab exports a redline PDF for free that Litera
     charges $400/yr for" — screenshot-worthy.

### LAST_WOW_TICK_AT: 2026-05-24T21:48:00Z (Export Redline shareable PDF)

This counts as a wow — it's the kind of capability customers tell
their paralegal friends about. >24h window reset.

### RELEASE_PENDING: v3.23.0

- Merge SHA: `59e478b`
- Tag: `v3.23.0` pushed with `--follow-tags`
- CI runs at push time (2026-05-24T21:48:30Z):
  - `build` workflow on main (run 26373664021)
  - `Docker (slab-server)` on tag v3.23.0 (run 26373663937)
  - `deploy-try` on main (run 26373664012)
  - `pages build and deployment` (run 26373663626)
- Next tick (MODE B FINALIZE): poll those run IDs, download artifacts,
  `gh release create v3.23.0` with the marketing notes from
  `docs/releases/v3.23.0.md`, upload the 6 best artifacts (macos-arm64
  dmg, macos-x64 dmg, linux x64 deb + AppImage, windows msi + nsis).

### Next tick (MODE B FINALIZE — first priority)

1. `gh run list --limit 8` — wait for all 4 above to succeed.
2. `gh run download <build_run_id> --dir /tmp/slab-release-3.23.0`.
3. `gh release create v3.23.0 --title 'v3.23.0 — Stack' --notes-file docs/releases/v3.23.0.md` + upload artifacts.
4. Clear `RELEASE_PENDING` from this file. Begin v3.24.0 "Stack Pro"
   (three-way compare) OR fall through to v0.10.0 "Beacon" pipeline.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 through v3.23.0 published. None open.

---

## STATUS: 🚧 v3.23.0 'Stack' — Tick 1 shipped (2026-05-24 14:10 PT)

**TICK 2026-05-24 14:10 PT (Sunday off-hours)** — MODE C DEVELOP.

Started executing `docs/plans/2026-05-24-v3.23.0-stack-visual-diff.md`.
Five commits on `feature/v3.23.0-stack-visual-diff`:

- `9bb797b` Task 1: `WordOp`/`WordDiff` types on `LineDiff` + 3 tests.
- `d5646cd` Task 2: `pdf::diff_words::{tokenize, diff_words}` via `similar`
  crate, 10 unit tests (round-trip safe, coalesces same-op runs).
- `3cc01ca` Task 3: `attach_word_diffs` post-processes every Delete→Insert
  pair so `LineDiff.words` carries the redline data. +2 tests.
- `9230a79` Task 4: `DiffPanel.svelte` renders `<ins>/<del>/<span>` tokens
  with subtle green/red tint + underline/strikethrough — falls back to
  whole-line tinting for isolated inserts/deletes.
- `1183d27` Task 8 (folded in): Cmd/Ctrl+Shift+D global shortcut, 3 command
  palette entries (`stack:compare/export/rerun`), ShortcutsOverlay entry,
  `$effect` listeners in DiffPanel for the palette deep-link events.

End-of-tick gates: cargo fmt clean, clippy -D warnings clean,
`cargo test --lib` = **1676 passed** (was 1661 → +15 new), pnpm check
0 errors. Branch pushed to origin.

### Tick stats

- 5 commits, 464 LOC net (target was 600 — under but the end-to-end
  capability is real: Cmd+Shift+D → drop two PDFs → see word-level
  inline redline. Buy-Button: PASS on all four criteria.)
- Buyer hook: Adobe Acrobat Pro Compare Files is $239/yr per seat;
  Litera Compare is $400/yr; we ship it free + offline.

### LAST_WOW_TICK_AT: 2026-05-24T19:50:00Z (Backfill dry-run table — kept)

Inline word-level redline counts as "Notice-it" polish; the bigger
WOW (visual side-by-side + scroll-sync ribbon) lands in Tick 2.

### Next tick (Tasks 5-7 — VISUAL MODE)

1. Task 5: `mode: "inline" | "visual"` tab in DiffPanel + lazy-load
   pdfjs-dist + paired canvas columns (model on PagesVisualPanel).
2. Task 6: Scroll-sync + change ribbon (the actual WOW — update
   `LAST_WOW_TICK_AT` then).
3. Task 7: Export Redline PDF (lopdf overlays).

If Tick 2 lands the ribbon, Tick 3 = Tasks 9-10 (version bump +
release notes + MODE A merge → v3.23.0 tag).

### RECENTLY_CLOSED_ISSUES

- v3.18.0 through v3.22.0 published. None open.

---

## STATUS: 📝 v3.23.0 'Stack' PLAN WRITTEN (2026-05-24 13:55 PT)

**TICK 2026-05-24 13:55 PT (Sunday off-hours)** — Planning tick.

Wrote `docs/plans/2026-05-24-v3.23.0-stack-visual-diff.md` (11 tasks,
~1500 LOC budget across 2 dev ticks + 1 release tick) for the next major
release: **Stack** — visual redline PDF compare, Litera-Compare class.

Three buyer hooks in one release:
- Word-level inline redline (green ins / red del at token granularity)
- Visual side-by-side via pdfjs with synced scroll + change ribbon (wow)
- Export Redline PDF (shareable, baked-in markup, no Slab needed to read)

Plan is committed but not yet executed — execution starts next off-hours
tick at Task 0 (branch `feature/v3.23.0-stack-visual-diff`).

### Next tick (MODE C — execute Task 0-4)

1. `git checkout -b feature/v3.23.0-stack-visual-diff` off `main`.
2. Tasks 1-4 (backend WordOp/WordDiff types, diff_words helper, attach to
   LineDiff, inline frontend redline). Single tick, ≥4 commits, ~700 LOC.
3. Tick after = Tasks 5-8 (visual mode + ribbon + export + palette).
4. Tick after = Tasks 9-10 (release prep + MODE A merge).
5. Tick after = MODE B FINALIZE.

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 (Cloudflare Pages).
- Optional: record 5-second demo video for landing (issue #27 closed).
- Disk on the mini at ~5 GiB free — `cargo clean` before heavy builds.

### LAST_WOW_TICK_AT: 2026-05-24T19:50:00Z (Backfill dry-run preview table)

Within 24h — no wow required this tick.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 / v3.19.0 / v3.20.0 / v3.21.0 / v3.22.0 published. None open.

---

## STATUS: ✅ v3.22.0 'Hopper Loop' PUBLISHED (2026-05-24 13:35 PT)

**TICK 2026-05-24 13:35 PT (Sunday off-hours)** — MODE B FINALIZE complete.

### What happened this tick

- Polled `26371749779` (build on main) → ✅ success.
- Docker `slab-server` on v3.22.0 tag → ✅ success (`26371749789`).
- `gh run download` → all 6 bundle artifacts present.
- `gh release create v3.22.0 --title "v3.22.0 — Hopper Loop"
  --notes-file docs/releases/v3.22.0.md <6 assets>` →
  https://github.com/Sanjays2402/slab/releases/tag/v3.22.0
- Verified: `isDraft=false`, `asset_count=6`. RELEASE_PENDING cleared.

### Next tick (MODE C — plan v3.23.0)

Re-poll `gh issue list` first. If empty, propose v3.23.0 from the
roadmap (Lathe / Atlas / Lens / etc) and start a feature branch.

### LAST_WOW_TICK_AT: 2026-05-24T19:50:00Z (Backfill dry-run preview table)

### RECENTLY_CLOSED_ISSUES

- v3.18.0 / v3.19.0 / v3.20.0 / v3.21.0 / **v3.22.0** published.

Session log: `.cron-state/sessions/2026-05-24-1335.md`.

---

## STATUS: 🚀 v3.22.0 'Hopper Loop' — MERGED + TAGGED, CI building (2026-05-24 13:20 PT)

**TICK 2026-05-24 13:20 PT (Sunday off-hours)** — MODE A RELEASE complete.

### What happened this tick

- Bumped versions 3.21.0 → 3.22.0 in `Cargo.toml`, `package.json`,
  `tauri.conf.json`, lockfile (commit `90cecba`).
- Wrote `docs/releases/v3.22.0.md` — marketing-tone release notes for
  Hopper Loop (backfill any folder, dry-run preview table, sqlite history).
- Quality gates on feature branch — all green (fmt + clippy + 1661 tests +
  pnpm check 0 errors).
- Merged `feature/v3.22.0-hopper-loop` → `main` (`d095acd` merge commit).
- Re-ran quality gates on `main` — all green.
- Tagged `v3.22.0` and pushed `main --follow-tags` + feature branch.

### RELEASE_PENDING: v3.22.0

- merge SHA: `d095acd`
- tag: `v3.22.0`
- CI runs in-flight:
  - `26371749779` — build on main
  - `26371749789` — Docker (slab-server) on v3.22.0 tag
  - `26371749772` — deploy-try (main)
  - `26371751680` — build on feature branch

### Next tick (MODE B FINALIZE)

1. `gh run watch 26371749779` (build on main) → must succeed.
2. `gh run download 26371749779 --dir /tmp/slab-release-v3.22.0` → 6 bundle artifacts.
3. `gh release create v3.22.0 --title "v3.22.0 — Hopper Loop" --notes-file docs/releases/v3.22.0.md <6 assets>`.
4. Verify `isDraft=false`, asset_count=6, Docker tag run succeeded.
5. Clear RELEASE_PENDING. Plan v3.23.0.

### LAST_WOW_TICK_AT: 2026-05-24T19:50:00Z (Backfill dry-run preview table)

---

## STATUS: 🚧 v3.22.0 'Hopper Loop' — UI complete, ready for release prep (2026-05-24 12:50 PT)

**TICK 2026-05-24 12:50 PT (Sunday off-hours)** — MODE C DEVELOP.

### What happened this tick

Shipped Tasks 5+6+7+8 of the v3.22.0 Hopper Loop plan end-to-end. The
backfill feature is now fully reachable from real UI:

- **Task 5** (c49ab33): Tauri command surface — `slab_hopper_plan_backfill`,
  `execute_backfill`, `list_backfill_runs` registered.
- **Tasks 6+7** (6cc1525): TS API client + `HopperBackfillPanel.svelte` —
  Liquid Glass overlay with dry-run preview table, action bar, recent-runs
  disclosure. Wired "Test on this folder…" button into Rules Editor.
- **Task 8** (1c27b84): Command palette entry "Hopper: Backfill folder
  with rules" + Cmd/Ctrl+Shift+H global shortcut + ShortcutsOverlay entry.

All 4 commits on `feature/v3.22.0-hopper-loop`. Quality gates all green:
cargo fmt + clippy + cargo test (73 hopper tests pass) + pnpm check 0
errors.

### LAST_WOW_TICK_AT: 2026-05-24T19:50:00Z (Backfill dry-run preview table)

The dry-run table is the wow: zero competitors (Hazel, Adobe AutoActions,
Foxit, Acrobat) show what's about to happen before they commit. Slab does.

### Next tick (Tasks 9-10 — RELEASE)

1. Bump versions to 3.22.0 in `Cargo.toml`, `tauri.conf.json`, `package.json`.
2. Write `docs/releases/v3.22.0.md` marketing-tone notes.
3. MODE A: merge branch → main, quality gates, tag v3.22.0, push.
4. MODE B: watch CI, download artifacts, create GitHub release.

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 (Cloudflare Pages).
- Optional: record 5-second demo video for landing (issue #27 closed).
- Disk on the mini at ~5 GiB free — `cargo clean` before heavy builds.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 / v3.19.0 / v3.20.0 / v3.21.0 published.

Session log: `.cron-state/sessions/2026-05-24-1250.md`.

---

## STATUS: ✅ v3.21.0 'Hopper Conditions' PUBLISHED (2026-05-24 10:20 PT)

**TICK 2026-05-24 10:20 PT (Sunday off-hours)** — MODE B FINALIZE complete.

### What happened this tick

- Polled `gh run watch 26367522708` (build on main) → ✅ success.
- Docker `Docker (slab-server)` on v3.21.0 tag → ✅ success (`26367522688`).
- `gh run download 26367522708 --dir /tmp/slab-release-v3.21.0` → all 6
  bundle artifacts present (macos-arm64 dmg, macos-x64 dmg, linux deb,
  linux AppImage, win msi, win nsis).
- `gh release create v3.21.0 --title "v3.21.0 — Hopper Conditions"
  --notes-file docs/releases/v3.21.0.md <6 assets>` →
  https://github.com/Sanjays2402/slab/releases/tag/v3.21.0
- Verified: `isDraft=false`, `asset_count=6`. RELEASE_PENDING cleared.
- Wrote next-tick plan: `docs/plans/2026-05-24-v3.22.0-hopper-loop.md`
  (10 bite-sized tasks, ≥600 LOC, end-to-end backfill+dry-run, Cmd+Shift+B).

### LAST_WOW_TICK_AT: 2026-05-24T16:55:00Z (Hopper Conditions live preview)

Within 24h — no wow required this tick.

### Next tick (MODE C — start v3.22.0 'Hopper Loop')

1. `git checkout -b feature/v3.22.0-hopper-loop`.
2. Execute Tasks 1-4 from `docs/plans/2026-05-24-v3.22.0-hopper-loop.md`
   (backend: types + `plan_backfill` + `execute_backfill` + sqlite history).
3. End-of-tick quality gates + push branch.
4. Tasks 5-10 across following 2-3 ticks.

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 (Cloudflare Pages).
- Optional: record 5-second demo video for landing (issue #27 closed).
- Disk on the mini at ~5 GiB free — `cargo clean` before heavy builds.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 / v3.19.0 / v3.20.0 / **v3.21.0** published.

Session log: `.cron-state/sessions/2026-05-24-1020.md`.

---
