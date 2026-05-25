# Slab Cron State

Last updated: 2026-05-25 07:36 PT by Cake (cron) — MODE C (CI still blocked)

## Active version

**v3.34.0 "Atlas Smart+" — feature branch pushed, code-complete except
release notes + integration polish. NOT merged to main yet (CI still
billing-blocked from v3.33).**

**v3.33.0 "Atlas Smart" — MERGED + TAGGED on main. Draft GitHub release
exists, awaiting binaries. Blocked on GitHub Actions billing.**

- v3.33 merge SHA: `7d4a673` on `main`
- v3.33 tag: `v3.33.0`
- v3.33 draft release: https://github.com/Sanjays2402/slab/releases
- v3.34 branch: `feature/v3.34.0-atlas-smart-plus` (5 commits, +962 LOC)

## ⚠️ CI BLOCKED — still needs Sanjay

All workflows on every push (v3.33 main, v3.34 feature) fail in 4-7s with:

> _"The job was not started because recent account payments have failed
> or your spending limit needs to be increased. Please check the
> 'Billing & plans' section in your settings"_

Reran 26403858592 + 26403858547 this tick — still failing identically.

**Action for Sanjay**: https://github.com/settings/billing → update
payment method OR raise the spending limit. Once unblocked:
- `gh run rerun 26403858592` (v3.33 build) → finalize v3.33 draft release.
- `gh run rerun 26403858547` (v3.33 Docker tag) → publish slab-server image.
- v3.34 build will start passing automatically on next push.

## This tick (2026-05-25 07:22–07:37 PT) — MODE C, code-only

**v3.34.0 "Atlas Smart+" shipped end-to-end on a feature branch:**

- `526f1e4` docs(plan): 6-task implementation plan saved at
  `docs/plans/2026-05-25-v3.34.0-atlas-smart-plus.md`.
- `13d8bbc` feat(library): FilterGroup / FilterClause / FilterCombinator
  Rust types — opt-in nested AND/OR/NOT clause tree, tagged serde,
  backward-compatible (legacy query_json still deserializes byte-perfect).
- `a821709` feat(library): recursive `build_group_sql` /
  `build_clause_sql` translators. Tag NOT via NOT IN subquery, folder
  NOT handles NULL, title NOT wraps LIKE pattern. Empty AND→1=1,
  empty OR→0=1. 8 new tests (1781 total now, all pass).
- `83323de` feat(ts): TypeScript mirror types + `emptyFilterGroup()` /
  `migrateFlatFilter()` helpers.
- `8ba6ba3` feat(ui): new `ClauseGroup.svelte` recursive component
  (~320 LOC) — AND↔OR pill, NOT toggle, type/value pickers, +Rule /
  +Group buttons, depth-cycling accent colors, Liquid Glass styling.
  Wired into SmartCollectionBuilder behind an "Advanced ⚡" mode toggle
  that carries over basic rules via migration helper. Live-preview
  pane reacts to nested edits.

Quality gates ALL green before push:
- `cargo fmt --check`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --lib`: **1781 passed** (was 1772 → +9 new)
- `pnpm check`: 0 errors, 104 pre-existing warnings unchanged

Branch pushed to origin. CI failed (5s, same billing block).

## Buy-Button qualification

- **Pick-us** ✅ Adobe / PDF Expert / Foxit ship zero nested-rule UX.
  Smart Mailboxes for PDF with NOT and OR — instant wedge.
- **Tell-a-friend** ✅ Builder screenshot with depth-colored groups
  + live count badge is demo gold.
- **Pay-for-it** ✅ Paralegals + researchers managing hundreds of PDFs
  will pay $49 for nested rules alone.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T14:35:00Z — recursive ClauseGroup builder
with depth-cycling accent colors (violet → sky → emerald → amber) and
NOT-toggle pills that turn rows red. Demo line: "(Tax 2024 OR Tax 2025)
AND NOT Archived". Next BIG wow due by ~2026-05-26 14:35 UTC.

## Recently closed issues

- v3.34.0 code-complete on branch (internal roadmap item, no GH issue).
- v3.33.0 shipped main + draft release (awaiting binaries).

## Next ticks

- **Tick 1 (NEXT)**: Re-poll CI billing status. If unblocked:
  1. `gh run rerun 26403858592 26403858547` → finalize v3.33 release.
  2. Merge `feature/v3.34.0-atlas-smart-plus` → main, tag v3.34.0,
     push, draft release.
- **Tick 2 (if CI still blocked)**: Polish v3.34 — add `LAST_WOW_TICK`
  command-palette entries, keyboard shortcut to toggle Advanced mode
  inside the builder (Cmd+Shift+A), write the v3.34.0 marketing-voice
  release notes file, and a screenshot/GIF placeholder.
- **Tick 3 (if CI still blocked)**: Start v3.35.0 "Atlas Presets" —
  built-in smart-collection templates ("Tax 2025", "Invoices last 30 days",
  "Contracts pending signature"). One-click create from a sidebar
  palette. Vertical slice incl. a backend preset registry.

## Pipeline state

| Branch                                | Status                              | Notes                            |
| ------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                | v3.33.0 merged + tagged + pushed    | Draft release waiting for CI     |
| `feature/v3.34.0-atlas-smart-plus`    | code-complete + pushed (5 commits)  | All gates green; merge when CI up |
| `feature/v3.33.0-atlas-smart`         | merged into main last tick          | Local branch still present       |

## Notes / housekeeping

- Disk: src-tauri/target still ~14 GiB, watch for fill.
- 104 pre-existing svelte a11y warnings — polish tick eligible.
- 1 Dependabot moderate vulnerability (#7) — still investigate.
- GitHub Actions billing is the persistent blocker — flagged again
  this tick. Re-flag every delivery until resolved.
