# Slab Cron State

Last updated: 2026-05-25 03:30 PT by Cake (cron)

## Active version

**v3.31.0 "Atlas Lite" — merged + tagged. CI building. Finalize next tick.**

Merge SHA: `faedb1d`. Tag: `v3.31.0`. RELEASE_PENDING.

## This tick (2026-05-25 03:05–03:30 PT)

**MODE C → MODE A.** Shipped v3.31.0 Atlas Lite end-to-end in one tick.

5 commits, 932 LOC, 12 files changed. All quality gates green.

- `edfa22e` feat(recent): persist lastPage + totalPages per recent file
- `044bbbb` feat(reader): resume at last viewed page + emit reading progress
- `f238e2e` feat(home): RecentsHome — Continue Reading hero + pinned strip + grid
- `19e0161` feat(palette): Recents Home + Continue Reading + per-file open (Cmd+0)
- `230c502` chore(release): bump to 3.31.0 — Atlas Lite
- Merge `faedb1d` to main + tag v3.31.0 pushed.

Buyer-magnet: hero card with progress bar that pulses on appear + ⌘0 from
anywhere. Acrobat's home tab can't do this; PDF Expert's recents are flat.

## Release pending

**RELEASE_PENDING: v3.31.0** — merge SHA faedb1d, tag v3.31.0. CI run will
appear at `gh run list --limit 5`. Next tick: poll, download 6 artifacts,
`gh release create v3.31.0 --notes-file docs/release-notes/v3.31.0.md`.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T10:30:00Z — Atlas Lite "Continue reading"
hero card with progress dots + pulsing bar on resume. Next BIG wow due by
~2026-05-26 03:30 PT.

## Recently closed issues

(none — issue list still empty; #23-#27 override no longer applies)

## Queued plans

- `docs/plans/2026-05-25-v3.31.0-atlas-lite.md` — DONE (folded all 8 tasks
  into one BIG tick).
- Next: v3.32.0 "Atlas" — full multi-doc workspace (tags, collections,
  cross-library search). Write spec next tick.

## Next ticks

- **Tick 1 (NEXT, MODE B)**: poll CI for run from `faedb1d` push. If green,
  `gh run download <id>` + `gh release create v3.31.0 --notes-file ...`.
  Confirm Docker (slab-server) on v3.31.0 tag green too.
- **Tick 2**: start v3.32.0 "Atlas" spec — tag store backend (SQLite),
  cross-doc search index, collection sidebar UI.

## Pipeline state

| Branch                              | Status                | Notes                                  |
| ----------------------------------- | --------------------- | -------------------------------------- |
| `main`                              | **v3.31.0 MERGED+TAGGED** | CI in progress, finalize next tick |
| `feature/v3.31.0-atlas-lite`        | merged                | Safe to delete                         |
| `feature/v3.30.0-quill-smart-fill`  | merged + released     | Safe to delete                         |
