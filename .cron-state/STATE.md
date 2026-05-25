# Slab Cron State

Last updated: 2026-05-25 08:20 PT by Cake (cron) — MODE C (CI still blocked, v3.35 shipped on branch)

## Active version

**v3.35.0 "Atlas Presets" — feature branch pushed, code-complete.**
One-click smart-collection templates (11 built-in) + ⌘⇧P shortcut +
command-palette entries. Not merged to main (CI still blocked).

**v3.34.0 "Atlas Smart+" — feature branch pushed, code-complete.**
Nested AND/OR/NOT clause tree + recursive ClauseGroup UI. Not merged.

**v3.33.0 "Atlas Smart" — MERGED + TAGGED on main.** Draft GitHub
release exists, awaiting binaries. Blocked on GitHub Actions billing.

- v3.33 merge SHA: `7d4a673` on `main`
- v3.33 tag: `v3.33.0`
- v3.33 draft release: https://github.com/Sanjays2402/slab/releases
- v3.34 branch: `feature/v3.34.0-atlas-smart-plus` (5 commits, +962 LOC)
- v3.35 branch: `feature/v3.35.0-atlas-presets` (4 commits this tick + plan)

## ⚠️ CI BLOCKED — still needs Sanjay

All workflows on every push (v3.33 main, v3.34, v3.35) fail in 4-7s with:

> _"The job was not started because recent account payments have failed
> or your spending limit needs to be increased. Please check the
> 'Billing & plans' section in your settings"_

Last failed runs this tick: 26405865562 (v3.34), and v3.35 push
imminent (will fail identically until billing fixed).

**Action for Sanjay**: https://github.com/settings/billing → update
payment method OR raise the spending limit. Once unblocked:
- `gh run rerun 26403858592` (v3.33 build) → finalize v3.33 draft release.
- `gh run rerun 26403858547` (v3.33 Docker tag) → publish slab-server image.
- Merge `feature/v3.34.0-atlas-smart-plus` → main, tag v3.34.0.
- Merge `feature/v3.35.0-atlas-presets` → main, tag v3.35.0.

## This tick (2026-05-25 08:07–08:25 PT) — MODE C

**v3.35.0 "Atlas Presets" shipped end-to-end on a feature branch:**

- `72f1f3c` feat(library): built-in smart-collection presets registry
  (11 templates, 9 tests, auto-create tags, uses v3.34 clause tree).
- `abc5feb` feat(commands): 3 Tauri commands
  (slab_preset_list / apply / already_applied) + invoke handler reg.
- `08293ab` feat(ui): PresetPicker.svelte modal (~280 LOC) + sidebar
  ★ button + Cmd/Ctrl+Shift+P shortcut + TS bindings.
- (slice 4 pending commit): command palette entries
  ("Add smart collection from preset…" / "New smart collection…") +
  window-event bridge + ShortcutsOverlay Library section + plan doc.

Quality gates ALL green:
- `cargo fmt --check`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --lib`: **1790 passed** (was 1781 → +9 new)
- `pnpm check`: 0 errors, 105 warnings (was 104, +1 minor a11y stylistic)

## Buy-Button qualification

- **Pick-us** ✅ Adobe / PDF Expert / Foxit ship zero library presets.
  macOS Smart Folders are PDF-blind.
- **Notice-it** ✅ Golden ★ in sidebar — impossible to miss.
- **Tell-a-friend** ✅ "Open Slab → ⌘⇧P → click Tax 2025 → done." 5s demo.
- **Pay-for-it** ✅ A paralegal sees "Contracts pending signature" as
  a built-in preset and asks IT to switch from $239/yr Acrobat.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T15:25:00Z — Preset Picker modal:
search + click → fully-rigged smart collection appears in sidebar
with its tag auto-created. Demo line: "Slab knows what a tax folder
looks like." Next wow due by ~2026-05-26 15:25 UTC.

## Recently closed issues

- v3.35.0 code-complete on branch (internal roadmap item, no GH issue).
- v3.34.0 code-complete on branch (internal roadmap item, no GH issue).
- v3.33.0 shipped main + draft release (awaiting binaries).

## Next ticks

- **Tick 1 (NEXT)**: Re-poll CI billing status. If unblocked:
  1. `gh run rerun 26403858592 26403858547` → finalize v3.33 release.
  2. Merge `feature/v3.34.0-atlas-smart-plus` → main, tag v3.34.0.
  3. Merge `feature/v3.35.0-atlas-presets` → main, tag v3.35.0.
- **Tick 2 (if CI still blocked)**: v3.36.0 "Atlas Personal Presets" —
  Save current smart collection as a personal preset; export/import
  preset packs (.slabpresets JSON). Vertical slice incl. file
  picker UX + sample pack file shipped with the app.
- **Tick 3**: Add `IsUntagged` / `IsOcr` / `FileSize` clause variants
  to the filter language and surface them in the builder + presets.

## Pipeline state

| Branch                                | Status                              | Notes                            |
| ------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                | v3.33.0 merged + tagged + pushed    | Draft release waiting for CI     |
| `feature/v3.34.0-atlas-smart-plus`    | code-complete + pushed (5 commits)  | All gates green; merge when CI up |
| `feature/v3.35.0-atlas-presets`      | code-complete (4 commits, this tick) | All gates green; merge after v3.34 |
| `feature/v3.33.0-atlas-smart`         | merged into main last tick          | Local branch still present       |

## Notes / housekeeping

- Disk: src-tauri/target still ~14 GiB, watch for fill.
- 105 pre-existing svelte a11y warnings — polish tick eligible.
- 1 Dependabot moderate vulnerability (#7) — still investigate.
- GitHub Actions billing is the persistent blocker — flagged again
  this tick. Re-flag every delivery until resolved.
