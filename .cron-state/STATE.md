# Slab Cron State

Last updated: 2026-05-25 08:55 PT by Cake (cron) — MODE A merge done, awaiting CI/billing.

## Active version

**v3.35.0 "Atlas Smart+ & Atlas Presets" — MERGED + TAGGED on main.**
Rolled v3.34.0 (nested AND/OR/NOT) + v3.35.0 (preset templates) into a
single tagged release. Draft GitHub release will exist once CI builds.

- v3.35 merge SHAs: `abb285c` (v3.34) + `331ba35` (v3.35) on `main`
- v3.35 release commit: `1342191`
- v3.35 tag: `v3.35.0` pushed to origin
- v3.33 draft release: https://github.com/Sanjays2402/slab/releases (still open)

## ⚠️ CI STILL BLOCKED — needs Sanjay

All workflows fail in 4-6s with:

> _"The job was not started because recent account payments have failed
> or your spending limit needs to be increased."_

This tick's failed runs (all on v3.35.0 push):
- 26408974836 (build, main)
- 26408974874 (deploy-try, main)
- 26408974773 (Docker slab-server, v3.35.0 tag)

**Action for Sanjay**: https://github.com/settings/billing
→ update payment method OR raise spending limit. Then:
- `gh run rerun 26408974836 26408974874 26408974773` to finalize v3.35.0.
- `gh run rerun 26403858592 26403858547` to finalize v3.33.0 draft.
- `gh release edit v3.35.0 --draft=false` once binaries upload.

## This tick (2026-05-25 08:40–08:55 PT) — MODE A merge tick

**v3.34.0 + v3.35.0 merged into main and tagged as v3.35.0.**

- `abb285c` Merge v3.34.0 'Atlas Smart+' — nested AND/OR/NOT rules
  (+1378 LOC, 6 files: recursive SQL builder, ClauseGroup.svelte,
  SmartCollectionBuilder rewire, TS types, plan doc)
- `331ba35` Merge v3.35.0 'Atlas Presets' — built-in templates
  (+1260 LOC, 11 files: presets.rs registry, PresetPicker modal,
  3 Tauri commands, sidebar ★ button, palette + shortcut entries)
- `1342191` chore(release): bump to 3.35.0

**Net to main this tick**: +2638 non-test LOC (well over 600 floor),
4 commits, 2 end-to-end vertical-slice features shipped (BIG-tier).

Quality gates ALL green on main:
- `cargo fmt --check`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --lib`: **1790 passed** (no regressions from v3.33)
- `pnpm check`: 0 errors, 105 warnings (unchanged)

## Buy-Button qualification (v3.35.0 combined)

- **Pay-for-it** ✅ Paralegal/legal-ops customers see "Contracts pending
  signature" as a built-in preset, AND can build arbitrary nested
  AND/OR/NOT rules — capabilities Adobe charges $239/yr for.
- **Notice-it** ✅ Golden ★ in sidebar + new "Advanced rules" mode in
  smart-collection builder.
- **Pick-us** ✅ macOS Smart Folders are PDF-blind. Adobe has no
  presets and only flat AND rules.
- **Tell-a-friend** ✅ "⌘⇧P → Tax 2025 → done" 5-second demo.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T15:25:00Z — Preset Picker modal still
holds the title. Next wow due by ~2026-05-26 15:25 UTC.

## Recently closed issues

- v3.35.0 (rolls up v3.34 + v3.35) — merged + tagged on main, awaiting CI.
- v3.33.0 shipped main + draft release (still awaiting binaries).

## Next ticks

- **Tick 1 (NEXT)**: Re-poll CI billing status. If unblocked:
  1. `gh run rerun 26408974836 26408974874 26408974773` → v3.35.0 release.
  2. `gh run rerun 26403858592 26403858547` → v3.33.0 release.
  3. `gh release edit v3.35.0 --draft=false && gh release edit v3.33.0 --draft=false`.
- **Tick 2 (if CI still blocked)**: v3.36.0 "Atlas Personal Presets" —
  Save current smart collection as a personal preset; export/import
  preset packs (.slabpresets JSON). Vertical slice incl. file
  picker UX + sample pack file shipped with the app.
- **Tick 3**: Add `IsUntagged` / `IsOcr` / `FileSize` clause variants
  to the filter language and surface them in the builder + presets.

## Pipeline state

| Branch                                  | Status                              | Notes                            |
| --------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                  | v3.35.0 merged + tagged + pushed    | Draft releases waiting on CI     |
| `feature/v3.34.0-atlas-smart-plus`      | merged → main                       | Safe to delete next tick         |
| `feature/v3.35.0-atlas-presets`         | merged → main                       | Safe to delete next tick         |
