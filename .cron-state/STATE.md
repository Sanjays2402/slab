# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

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
