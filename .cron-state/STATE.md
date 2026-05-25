# Slab Cron State

Last updated: 2026-05-24 19:42 PT by Cake (cron)

## Active version

**v3.25.0 "Quill Pro Batch" — SHIPPED. Release published with 6 artifacts.**
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.25.0
- CI: all 3 runs green (build 26379549253, deploy-try 26379549254, Docker 26379549183)
- Artifacts: macOS arm64 dmg, macOS x64 dmg, Linux deb, Linux AppImage, Windows msi, Windows nsis exe

## RELEASE_PENDING

(none — v3.25.0 finalized this tick)

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-24T02:08:16Z (v3.25.0 Quill Batch — Acrobat Data Merge replacement, free + offline)

## Recently closed issues

(none touched this tick)

## What shipped this tick (Quill Pro Batch — 4 commits, ~1300 LOC)

1. `094c5f5` — added `csv` crate dependency
2. `b36ad0d` — `pdf/forms_batch.rs` driver (495 LOC, 13 unit tests): BatchSpec/RowResult/BatchReport DTOs, render_name templater, read_csv, run_batch with per-row error capture, optional flatten + zip
3. `03192ab` — `slab_forms_batch_fill` Tauri command + `QuillBatchPanel.svelte` (Liquid Glass 3-card UI) + sidebar entry + Cmd/Ctrl+Shift+B shortcut + `ActionId::QuillBatchOpen`
4. `9c5c82d` — version bump 3.24.0→3.25.0 (Cargo.toml, tauri.conf.json, package.json) + CHANGELOG entry + `docs/release-notes/v3.25.0.md`

Buy-Button test PASS: Adobe Acrobat charges $20/mo for Data Merge. PDF Expert doesn't have it. Foxit gates it. We do it free, offline, cross-platform. HR/legal/gov workflows.

Quality gates green on main: cargo fmt, clippy (--lib -D warnings), cargo test (13/13 new + 1697 unchanged), pnpm check (0 errors).

## Next ticks (after MODE B finalizes v3.25.0)

Re-poll `gh issue list` (issues #23-#27 were the priority override; check if still open).
If clear, continue version pipeline — likely v3.26.0 Quill Designer or v3.27.0 Quill Auto-Detect to complete the Quill quartet replacing Acrobat Prepare Form + Data Merge.

## Plans queued (in execution order)

- `docs/plans/2026-05-24-v3.26.0-quill-designer.md` — drag-to-draw AcroForm fields.
- `docs/plans/2026-05-24-v3.27.0-quill-autodetect.md` — heuristic field detector.
- `docs/plans/2026-05-24-v3.28.0-quill-hub.md` — unified QuillHubPanel (capstone).
- `docs/plans/2026-05-24-show-hn-launch.md` — **(NEW, 2026-05-24 19:25 PT)** 8-task launch-prep
  plan that bridges the Quill quartet → actual customers. Includes HN post variants,
  8s demo recording script, /launch landing page, T-24h runbook, LAUNCH_MODE cron flag.
  Execute AFTER v3.28.0 ships. Two Sanjay action items inside: record demo, take screenshots.
