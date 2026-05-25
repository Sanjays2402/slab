# Slab Cron State

Last updated: 2026-05-25 01:21 PT by Cake (cron)

## Active version

**v3.30.0 "Quill Smart Fill" — MERGED to main + TAGGED. CI in progress.**
Awaiting CI green to finalize the GitHub release with artifacts.

## This tick (2026-05-25 01:17–01:22 PT)

**MODE A — Release.** Slice 4 of v3.30.0 shipped end-to-end.

- 107e1f7 — chore(release): version bump 3.29.0 → 3.30.0 in
  package.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json,
  Cargo.lock refresh
- 91de8af — Merge commit (--no-ff) feature/v3.30.0-quill-smart-fill → main
  (25 files changed, 3092 insertions, 43 deletions across 10 feature commits)
- Tag `v3.30.0` annotated and pushed
- Pushed `main` + tag to origin

Quality gates ALL CLEAN before merge:
- pnpm check ✓ (0 errors, 74 pre-existing a11y warnings)
- cargo fmt ✓
- cargo clippy --all-targets -D warnings ✓
- cargo test --lib ✓ (1765 passed)

## Release pending

RELEASE_PENDING: v3.30.0 — merge SHA 91de8af, tag v3.30.0
- build (main): run 26390862691
- Docker (slab-server) on tag v3.30.0: run 26390862476
- deploy-try (main): run 26390862687
- pages-build-deployment: run 26390861907

Next tick (MODE B): if CI green →
  `gh run download 26390862691 --dir /tmp/slab-release-3.30.0`
  curate 6 best artifacts (macos-arm64.dmg, macos-x64.dmg,
  linux x64 .deb + .AppImage, windows .msi + nsis)
  `gh release create v3.30.0 --title 'v3.30.0 — Quill Smart Fill'
   --notes-file docs/release-notes/v3.30.0.md` + upload artifacts.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T07:35:00Z (Smart Fill drag-drop +
AI proposal diff UI shipped earlier tonight). v3.30.0 itself IS
the wow — public release publishing next tick.

## Recently closed issues

(none — issue list empty)

## Next ticks

- **Tick 1 (NEXT, MODE B)**: finalize GH release for v3.30.0 once CI green.
  Poll `gh run view 26390862691`. If failed → investigate logs, hotfix to
  v3.30.1. If success → create release with artifacts + marketing notes.
- **Tick 2 onwards**: re-poll issue list at start; if empty, start the
  next roadmap item. Suggested next: **v3.31.0 "Atlas Lite"** — Recent
  Files panel with thumbnails + last-position resume + tag/collection
  sidebar (cross-cutting feature from the roadmap; high pick-us value
  vs Acrobat which doesn't surface recents well). Or **v3.31.0
  "Theater"** — full-screen presenter mode (Buy-Button: tell-a-friend).
  Sanjay can vote; default = Atlas Lite.

## Pipeline state

| Branch                              | Status              | Notes                                  |
| ----------------------------------- | ------------------- | -------------------------------------- |
| `main`                              | **v3.30.0 TAGGED**  | CI in progress, release pending        |
| `feature/v3.30.0-quill-smart-fill`  | merged              | Safe to delete after release published |
| `feature/v3.29.0-forms-tour`        | merged + released   | Safe to delete                         |
