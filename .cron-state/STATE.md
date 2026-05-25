# Slab Cron State

Last updated: 2026-05-24 22:42 PT by Cake (cron)

## Active version

**v3.29.0 "Forms Tour" — MERGED to main + TAGGED.** Build run 26385319422
in_progress, Docker tag run 26385319416 in_progress. Finalize next tick
(MODE B: download artifacts + `gh release create`).

RELEASE_PENDING: v3.29.0 — merge SHA ffe9de4, tag v3.29.0, CI run 26385319422

## This tick (2026-05-24 22:38–22:42 PT)

**MODE A — Release pipeline.** Merged `feature/v3.29.0-forms-tour` into
main via no-ff merge, tagged v3.29.0, pushed main + tag together.

- Pre-merge CI on feature branch (run 26384738089): SUCCESS
- Post-merge gates on main: cargo fmt ✓, clippy ✓, 1740 lib tests ✓,
  pnpm check 0 errors (74 pre-existing warnings, unchanged)
- 4 commits + 1 merge commit (ffe9de4), 905 / 4 LOC carried forward
- Tag-triggered Docker build + main build kicked off at 05:41:47Z

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T05:22:39Z (v3.29.0 Forms Tour — spotlight
+ cubic-bezier coachmark)

## Recently closed issues

(none — issue list still empty)

## Next ticks

- **MODE B**: poll CI 26385319422 → success → `gh run download` →
  `gh release create v3.29.0` with 6 artifacts + release notes from
  `docs/release-notes/v3.29.0.md`.
- **Then**: re-poll `gh issue list`, then begin **v0.10.0 Beacon** arc
  (Ollama-backed local PDF chat) per roadmap. Specs at
  `.cron-state/proposals/v0.10.0-beacon-ai.md`.

## Pipeline state

| Branch                        | Status            | Notes                                  |
| ----------------------------- | ----------------- | -------------------------------------- |
| `main`                        | v3.29.0 MERGED    | Tag pushed, CI in_progress             |
| `feature/v3.29.0-forms-tour`  | merged            | Safe to delete after release published |
