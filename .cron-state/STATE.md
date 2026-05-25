# Slab Cron State

Last updated: 2026-05-25 05:30 PT by Cake (cron) — planning tick

## Active version

**v3.32.0 "Atlas" — MERGED to main + TAGGED + PUSHED. CI in progress.**

Merge SHA `a1225f9`. Build workflow run `26399157609` (queued/running).
Docker workflow `26399160794` running. Tag `v3.32.0` pushed.

## Release pending

RELEASE_PENDING: v3.32.0 — merge SHA a1225f9, tag v3.32.0,
build run 26399157609, docker run 26399160794.

Next tick: poll those two runs. If green → `gh run download` →
`gh release create v3.32.0 --title "v3.32.0 — Atlas" --notes-file
docs/release-notes/v3.32.0.md` + upload 6 artifacts.

## This tick (2026-05-25 04:48–04:55 PT) — MODE A

- Recovered 13 GiB by `cargo clean` (target was 14 GiB, disk at 100%).
- Quality gates on main: fmt OK, clippy clean, 1771 tests pass,
  pnpm check 0 errors (99 a11y warnings, pre-existing).
- Merged `feature/v3.32.0-atlas` into main (no-ff), 1441 insertions
  across 15 files. Tagged + pushed.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T11:40:00Z — Atlas pulsing count badges
(scale 1.18 + accent, 220ms spring) on Collections sidebar.
Next BIG wow due by ~2026-05-26 11:40 UTC.

## Recently closed issues

- v3.31.0 released (no GH issue tracked).
- v3.32.0 merged + tagged this tick (RELEASE_PENDING).

## Next ticks

- **Tick 1 (NEXT, MODE B)**: poll CI runs 26399157609 + 26399160794.
  If green → finalize v3.32.0 GitHub release with 6 artifacts.
  If failing → read log, fix, hotfix on a follow-up branch.
- **Tick 2 (MODE C)**: execute v3.33.0 "Atlas Smart" plan at
  `docs/plans/2026-05-25-v3.33.0-atlas-smart.md` (6 tasks, ≥6 commits,
  end-to-end Smart Collection builder + drag-to-collection).

## v3.33.0 plan written this tick

Plan at `docs/plans/2026-05-25-v3.33.0-atlas-smart.md` covers:
- Task 1: backend `update_smart_collection` + unit test
- Task 2: Tauri command + TS wrapper
- Task 3: `SmartCollectionBuilder.svelte` modal w/ live preview
- Task 4: wire into sidebar + Cmd+Shift+N + context menu
- Task 5: drag-from-doc-card → collection rail
- Task 6: release prep (3.33.0 version bump + release notes)

Branch will be `feature/v3.33.0-atlas-smart`. Buy-Button test: Pick-us
(Smart Mailboxes for PDF, Adobe/PDF Expert have no equivalent) + Tell-a-friend
(live preview pane + pulsing count badge on add).

## Pipeline state

| Branch                              | Status                | Notes                                  |
| ----------------------------------- | --------------------- | -------------------------------------- |
| `main`                              | v3.32.0 tagged, CI WIP | Atlas Collections shipped              |
| `feature/v3.32.0-atlas`             | merged                | Safe to delete                         |
| `feature/v3.31.0-atlas-lite`        | merged + released     | Safe to delete                         |

## Notes / housekeeping

- Disk: src-tauri/target was 14 GiB, cleaned. Watch for fill again.
- 99 pre-existing svelte a11y warnings in SignetPanel.svelte etc. —
  not blocking but worth a polish tick.
