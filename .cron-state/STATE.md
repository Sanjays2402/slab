# Slab Cron State

Last updated: 2026-05-25 06:02 PT by Cake (cron) — MODE B finalize

## Active version

**v3.32.0 "Atlas" — RELEASED. https://github.com/Sanjays2402/slab/releases/tag/v3.32.0**

CI green (build run 26399157609, docker run 26399160794). All 6 artifacts uploaded
(macos-arm64 dmg, macos-x64 dmg, linux deb + AppImage, windows msi + nsis).
Merged feature branches deleted locally.

## Release pending

(none)

## This tick (2026-05-25 06:01–06:05 PT) — MODE B

- Polled both CI runs → both `success`.
- Downloaded 6 artifacts from build run 26399157609.
- `gh release create v3.32.0 --title "v3.32.0 — Atlas"` with release notes
  from `docs/release-notes/v3.32.0.md`. Release URL above.
- Cleaned up: deleted merged feature branches `v3.32.0-atlas` and
  `v3.31.0-atlas-lite` locally.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T11:40:00Z — Atlas pulsing count badges on
Collections sidebar. Next BIG wow due by ~2026-05-26 11:40 UTC.

## Recently closed issues

- v3.31.0 released (no GH issue tracked).
- v3.32.0 released this tick.

## Next ticks

- **Tick 1 (NEXT, MODE C)**: Execute v3.33.0 "Atlas Smart" plan at
  `docs/plans/2026-05-25-v3.33.0-atlas-smart.md`. Create branch
  `feature/v3.33.0-atlas-smart` from main. Aim for Tasks 1-3 first tick
  (backend + Tauri command + SmartCollectionBuilder modal) = ≥4 commits,
  ≥600 LOC. Buy-Button: Pick-us (Smart Mailboxes for PDF) + Tell-a-friend
  (live preview pane).
- **Tick 2**: Complete Tasks 4-6 (sidebar wiring + drag-to-collection +
  release prep). Merge + tag + release v3.33.0.

## v3.33.0 plan

Plan at `docs/plans/2026-05-25-v3.33.0-atlas-smart.md`. 6 tasks:
- Task 1: backend `update_smart_collection` + unit test
- Task 2: Tauri command + TS wrapper
- Task 3: `SmartCollectionBuilder.svelte` modal w/ live preview
- Task 4: wire into sidebar + Cmd+Shift+N + context menu
- Task 5: drag-from-doc-card → collection rail
- Task 6: release prep (3.33.0 version bump + release notes)

## Pipeline state

| Branch                              | Status                | Notes                                  |
| ----------------------------------- | --------------------- | -------------------------------------- |
| `main`                              | v3.32.0 released      | Atlas Collections live on GitHub       |
| `feature/v3.33.0-atlas-smart`       | not yet created       | Next tick: create + Task 1-3           |

## Notes / housekeeping

- Disk: src-tauri/target was 14 GiB, cleaned last tick. Watch for fill again.
- 99 pre-existing svelte a11y warnings in SignetPanel.svelte etc. —
  not blocking but worth a polish tick eventually.
