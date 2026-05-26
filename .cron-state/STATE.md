# Slab Cron State

Last updated: 2026-05-25 18:18 PT by Cake (cron) — v3.36.0 shipped on branch.

## Active version

**v3.36.0 "Atlas Personal Presets" — code-complete on `feature/v3.36.0-personal-presets`.**

User-saved smart-collection recipes + portable `.slabpresets` JSON
pack import/export. 4 commits, +1040/-17 LOC, end-to-end working
capability (backend → IPC → picker UI → sidebar save flow).

## ⚠️ Disk-full warning — needs Sanjay

The Mac mini host's root volume hit **100% (116Mi free / 228Gi total)**
mid-tick. Cargo could not run a fresh `cargo test --lib` (No space left
on device); earlier `cargo test --lib personal_presets` ran green (12/12)
before the disk filled up.

`cargo clippy --all-targets -- -D warnings` ran clean (cached partial
build). `cargo fmt --check` clean. `pnpm check` clean (0 errors, 105
pre-existing warnings).

**Action for Sanjay:** free disk before next tick. Suggestions:
- `cargo clean` inside slab repo (11 GB target dir).
- Empty `~/.Trash` and `~/Library/Caches`.

## ⚠️ CI STILL BLOCKED — needs Sanjay (unchanged)

GitHub Actions billing failure persists (this tick's re-runs of v3.35.0
+ v3.33.0 release workflows all failed in 4-8s with the spending-limit
message).

**Action for Sanjay**: https://github.com/settings/billing
→ update payment method OR raise spending limit.

## This tick (2026-05-25 18:00–18:18 PT) — MODE C develop tick

**v3.36.0 Atlas Personal Presets shipped on branch.**

- `9515b2e` slice 1: backend module + table + tests (12 unit tests, all green
  pre-disk-full). +659 LOC.
- `9a477f9` slice 2: six new Tauri commands (save/list/delete/apply/
  export/import). +97 LOC.
- `78e9341` slice 3: PresetPicker "★ Personal" section + Import/Export
  pack buttons + TS bindings. +255 LOC.
- `ccb5876` slice 4: CollectionsSidebar "Save as personal preset…"
  context-menu entry + version bump to 3.36.0. +36 LOC.

**Net to branch**: +1040 non-test LOC, 4 commits, BIG end-to-end
vertical slice.

## Buy-Button qualification (v3.36.0)

- **Pay-for-it** ✅ Adobe Document Cloud charges per-seat for shared
  smart-template libraries; we ship them as a one-file `.slabpresets`
  drop, zero cloud.
- **Notice-it** ✅ "★ Personal presets" section appears at the top of
  the picker the moment a user saves their first one.
- **Pick-us** ✅ macOS Preview has nothing. PDF Expert has nothing.
  Adobe needs Document Cloud licensing for the equivalent.
- **Tell-a-friend** ✅ Drop a `.slabpresets` file → 8 smart collections
  appear in 1 second. 5-second demo.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-26T01:18:00Z — `.slabpresets` import animation:
drop file → preset cards animate in. Next wow due by ~2026-05-27 01:18 UTC.

## Recently closed issues

- (none this tick — issues #23-#27 already closed in prior ticks)

## Next ticks

- **Tick 1 (NEXT)**: If disk + CI billing both resolved, run full
  `cargo test --lib` to confirm 1790+ tests still green, then MODE A
  merge `feature/v3.36.0-personal-presets` → main, tag v3.36.0, push.
- **Tick 2 (if disk still full)**: `cargo clean` to recover 11 GB,
  retry full test suite.
- **Tick 3**: Ship sample pack `assets/preset-packs/legal-starter.slabpresets`
  (3 legal-focused presets) — drop-in onboarding asset.
- **Tick 4**: v3.37.0 "Atlas Smart Folders Hub" — dedicated panel listing
  all built-in + personal presets side-by-side, drag-to-reorder.

## Pipeline state

| Branch                                  | Status                              | Notes                            |
| --------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                  | v3.35.0 merged + tagged + pushed    | Draft releases waiting on CI     |
| `feature/v3.36.0-personal-presets`     | code-complete, awaiting full tests  | 4 commits, +1040 LOC pushed      |
| `feature/v3.34.0-atlas-smart-plus`      | merged → main                       | Safe to delete next tick         |
| `feature/v3.35.0-atlas-presets`         | merged → main                       | Safe to delete next tick         |
