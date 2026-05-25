# Slab Cron State

Last updated: 2026-05-25 06:50 PT by Cake (cron) — MODE C + RELEASE (partial)

## Active version

**v3.33.0 "Atlas Smart" — MERGED + TAGGED + PUSHED. Draft release created.
Awaiting CI binaries (BLOCKED on GitHub Actions billing).**

- Merge SHA: `7d4a673` on `main`
- Tag: `v3.33.0`
- Draft release: https://github.com/Sanjays2402/slab/releases (titled
  "v3.33.0 — Atlas Smart", needs artifacts uploaded once CI runs)

## ⚠️ CI BLOCKED — needs Sanjay

All workflows on the v3.33.0 push + tag failed in 4-5s with:

> _"The job was not started because recent account payments have failed
> or your spending limit needs to be increased. Please check the
> 'Billing & plans' section in your settings"_

Same error on the prior `chore(state): finalize v3.32.0 Atlas release`
build run too — so v3.32.0's binaries that shipped earlier today were
the LAST ones produced before billing lapsed.

**Action for Sanjay**: log in to https://github.com/settings/billing
and either update payment method or raise the spending limit. Once
unblocked, re-run via `gh run rerun 26403858592` (build) and
`gh run rerun 26403858547` (Docker), then I can finalize the Draft
release with artifacts and the `latest` tag.

## Release pending

- v3.33.0 — Draft release exists, needs 6 artifacts (macos arm64+x64 dmg,
  linux x64 deb+AppImage, windows msi+nsis) once CI runs.

## This tick (2026-05-25 06:37–06:50 PT) — MODE C → RELEASE

**v3.33.0 "Atlas Smart" shipped end-to-end (6 commits, +1336 LOC):**

- `5cf8447` feat(library): `update_smart_collection` partial-update helper + test
- `6b6b040` feat(tauri): `slab_smart_collection_update` command + TS wrapper
- `36581a0` feat(ui): SmartCollectionBuilder modal (837 LOC) — visual filter
  builder w/ live preview pane, 12 icons, 8 colors, AND across title/tag/folder
- `c235a35` feat(ui): wire builder into sidebar — Cmd+Shift+N hotkey,
  right-click "Edit rules…" menu, drag-to-collection drop targets, toast
- `755a498` feat(ui): library doc cards are draggable (`x-slab-doc-ids`
  dataTransfer payload)
- `6984ecd` chore(release): bump to 3.33.0 + release notes

All quality gates passed BEFORE push:
- `cargo fmt --check` clean
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo test --lib` → 1772 passed
- `pnpm check` → 0 errors, 104 pre-existing warnings

Merged feature branch to main fast-forward style with --no-ff, tagged v3.33.0,
pushed both. Draft release created with marketing-voice notes.

## Buy-Button qualifying

- **Pick-us test** ✅ Smart Mailboxes for PDF; Adobe/PDF Expert ship nothing
  comparable.
- **Tell-a-friend** ✅ Live preview pane + drag-to-collection are demo-worthy.
- **Notice-it** ✅ Returning user opens the rail, sees the new "+" next to
  Smart and the right-click menu on existing smart rows.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T13:50:00Z — SmartCollectionBuilder live preview
pane with pulsing match-count badge that re-runs as you type. Next BIG wow
due by ~2026-05-26 13:50 UTC.

## Recently closed issues

- v3.33.0 shipped (no GH issue, internal roadmap item).
- Atlas pipeline progressing: v3.30.0 → v3.31.0 → v3.32.0 → v3.33.0 today.

## Next ticks

- **Tick 1 (NEXT)**: Poll CI billing status. If unblocked, run
  `gh run rerun 26403858592 26403858547`, wait for green, download
  artifacts, upload to the v3.33.0 Draft release, publish it.
- **Tick 2**: Plan v3.34.0 — NOT / OR rules in the smart collection builder
  (`LibraryFilter` schema extension + UI nesting). This is the natural
  follow-on while users are still excited about Atlas Smart.
- Alternative if CI stays blocked >24h: ship code-only ticks that
  accumulate value (v3.34.0 NOT/OR, v3.35.0 Library presets), then batch
  a single release once billing is restored.

## Pipeline state

| Branch                              | Status                          | Notes                            |
| ----------------------------------- | ------------------------------- | -------------------------------- |
| `main`                              | v3.33.0 merged + tagged + pushed | Draft release exists             |
| `feature/v3.33.0-atlas-smart`       | merged into main                | Local branch still present       |

## Notes / housekeeping

- Disk: src-tauri/target was 14 GiB last week, watch for fill.
- 104 pre-existing svelte a11y warnings; not blocking but worth a polish tick.
- 1 Dependabot moderate vulnerability surfaced on push (#7). Investigate
  next tick.
- GitHub Actions billing is the immediate blocker — flagged to Sanjay in
  this delivery and pinned to the top of this file.
