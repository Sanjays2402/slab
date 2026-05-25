# Slab Cron State

Last updated: 2026-05-25 04:35 PT by Cake (cron)

## Active version

**v3.32.0 "Atlas" — DONE on `feature/v3.32.0-atlas`. Merge + tag next tick.**

Branch is feature-complete: Collections backend + Smart Collections +
Tauri commands + sidebar UI with pulsing badges + release notes.
Quality gates green (1771 tests pass, clippy + svelte-check clean).

## This tick (2026-05-25 03:53–04:40 PT) — DOUBLE-SHIP

**MODE B + MODE C in one tick.**

### Mode B — v3.31.0 finalized
- CI run 26395604737 completed success across all 7 jobs.
- `gh release create v3.31.0 — Atlas Lite` with 6 artifacts:
  macOS arm64 + x64 dmgs, Linux deb + AppImage, Windows nsis + msi.
- Release notes lead with "Slab now opens where you left off."

### Mode C — v3.32.0 Atlas shipped end-to-end
4 commits on `feature/v3.32.0-atlas`:
- `b6f5017` feat(library): collections schema + Rust module (514 LOC, 6 tests)
- `f6ecbdf` feat(commands): 11 Tauri commands wired into invoke_handler
- `8e042e1` feat(ui): CollectionsSidebar with pulsing badges (521 LOC)
- `<release>` chore(release): bump to 3.32.0 + notes + spec

~1400 net LOC, end-to-end working capability (schema → DB → commands →
client bindings → sidebar → filter override). Buy-Button passes:
"Pick-us test" beats both Acrobat's cloud-only collections and
PDF Expert's Mac-only Folders.

## Release pending

(none yet — Atlas will become RELEASE_PENDING next tick when merged.)

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T11:40:00Z — Atlas pulsing count badges
on Collections (scale 1.18 + accent fill, 220ms spring) when a fresh
doc lands. Demo-able: drag, drop, badge bounces.
Next BIG wow due by ~2026-05-26 11:40 UTC.

## Recently closed issues

- v3.31.0 released (no GitHub issue tracked).
- v3.32.0 spec'd + shipped on feature branch this tick.

## Queued plans

- `.cron-state/proposals/v3.32.0-atlas.md` — DONE (folded 9 tasks
  into one BIG tick).
- Next: v3.33.0 — likely "drag-from-doc-card to collection" UX plus
  per-collection sort + smart-collection builder UI (currently the
  user can only see the two seeded defaults). Spec next tick.

## Next ticks

- **Tick 1 (NEXT, MODE A → B)**: merge `feature/v3.32.0-atlas` to
  main, tag `v3.32.0`, push, watch CI run, finalize release.
- **Tick 2**: start v3.33.0 — Smart Collection builder modal (pick
  filters, name it, save) + drag-from-card-to-rail UX.

## Pipeline state

| Branch                              | Status                | Notes                                  |
| ----------------------------------- | --------------------- | -------------------------------------- |
| `main`                              | v3.31.0 RELEASED      | Quill Smart Fill → Atlas Lite shipped  |
| `feature/v3.32.0-atlas`             | **DONE, ready to merge** | 4 commits, all gates green          |
| `feature/v3.31.0-atlas-lite`        | merged + released     | Safe to delete                         |
