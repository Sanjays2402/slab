# Slab Cron State

Last updated: 2026-05-25 20:?? PT by Cake (cron) — v3.37.0 Atlas Smart Folders Hub shipped on branch.

## Active version

**v3.37.0 "Atlas Smart Folders Hub"** — feature branch
`feature/v3.37.0-smart-folders-hub`, 4 commits, +1083 LOC, end-to-end.

Next tick: MODE A — merge to main + tag v3.37.0.

## ⚠️ CI STILL BLOCKED — needs Sanjay (unchanged)

GitHub Actions billing failure persists. v3.36.0 workflows all failed in
4-6 seconds with the spending-limit message. Tags + pushes will keep
failing CI until billing is fixed.

**Action for Sanjay**: https://github.com/settings/billing
→ update payment method OR raise spending limit.

## This tick (2026-05-25 20:??-20:?? PT) — MODE C develop tick

**Shipped v3.37.0 "Atlas Smart Folders Hub" end-to-end on feature branch.**

Per-slice:

- Slice 1 `7fca4b5`: schema v5→v6 migration, library_smart_folder_order
  table + pin column. 1 file, +20 LOC. Test: schema bump green.
- Slice 2 `5f9a6bf`: `smart_folders.rs` module — list_smart_folders /
  set_order / set_pinned, 6 unit tests cover default order, personal
  appears after builtins, reorder persists, pin floats to top, unpin
  restores, personal-pinned-above-builtins. 2 files, +280 LOC.
- Slice 3 `a5f7642`: 3 Tauri commands (slab_smart_folders_list /
  reorder / pin) + TS client wrappers in src/lib/library.ts. 2 files,
  +121 LOC.
- Slice 4 `d0c1fa5`: SmartFoldersHubPanel.svelte (614 LOC) — search,
  drag-to-reorder, pin/unpin, apply, export-pack — wired into
  CollectionsSidebar (header 🗂 button + Cmd/Ctrl+Shift+F shortcut),
  CommandPalette ("Smart Folders Hub…" entry). Version bump 3.36.0 →
  3.37.0. Fixed pre-existing collections schema_version assertion.

Quality gates ALL PASS:
- `cargo fmt --all --check` → clean
- `cargo clippy --lib -- -D warnings` → clean
- `cargo test --lib` → 1808 passed, 0 failed
- `pnpm check` → 0 errors, 105 warnings (all pre-existing a11y)

## Buy-Button qualification (v3.37.0)

- **Pay-for-it** ✅ Acrobat Pro charges per-seat for "shared collections"
  via XFDF blobs requiring enterprise licensing. We give a one-screen hub
  + one-file .slabpresets export for free.
- **Notice-it** ✅ New 🗂 button in the sidebar Smart row + new shortcut
  Cmd/Ctrl+Shift+F.
- **Pick-us** ✅ Preview/PDF Expert have no centralized preset
  management. Foxit's "favorites" is a flat list with no reorder/pin.
- **Tell-a-friend** ✅ Drag-to-reorder + pin-star + "Export pack…" =
  great demo gif (the whole point of the hub is the screenshot).

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-26T03:?? UTC (drag-to-reorder + dragover
highlight + pop-in modal + pin-star animation in the Smart Folders Hub).
Next wow due by ~2026-05-27 03:?? UTC.

## Recently closed issues

- (none this tick — issues #23-#27 were already closed in earlier ticks
  per the override; falling through to roadmap)

## Next ticks

- **Tick 1 (NEXT)**: MODE A — merge `feature/v3.37.0-smart-folders-hub`
  into main + tag v3.37.0 + push --follow-tags. Run quality gates once
  on main; on success, write RELEASE_PENDING.
- **Tick 2**: Delete merged feature branches:
  `feature/v3.34.0-atlas-smart-plus`, `feature/v3.35.0-atlas-presets`,
  `feature/v3.36.0-personal-presets`,
  `feature/v3.37.0-smart-folders-hub`.
- **Tick 3**: Ship sample pack `assets/preset-packs/legal-starter.slabpresets`
  (3 legal-focused presets) as drop-in onboarding asset.
- **Tick 4**: v3.38.0 idea — "Smart Folders auto-suggest" — Beacon AI
  suggests a personal preset based on the user's last 50 search queries.

## Pipeline state

| Branch                                  | Status                              | Notes                            |
| --------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                  | v3.36.0 merged + tagged + pushed    | CI billing-blocked               |
| `feature/v3.37.0-smart-folders-hub`     | **DONE, ready to merge**            | 4 commits +1083 LOC, gates pass  |
| `feature/v3.36.0-personal-presets`     | merged → main                       | Safe to delete                   |
| `feature/v3.34.0-atlas-smart-plus`     | merged → main                       | Safe to delete                   |
| `feature/v3.35.0-atlas-presets`        | merged → main                       | Safe to delete                   |
