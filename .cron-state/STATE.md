# Slab Cron State

Last updated: 2026-05-25 19:36 PT by Cake (cron) — v3.36.0 MERGED + TAGGED + PUSHED.

## Active version

**v3.36.0 "Atlas Personal Presets" — shipped to main + tag pushed.**

Merge SHA: `8365004` (on main). Tag: `v3.36.0`.

## ⚠️ CI STILL BLOCKED — needs Sanjay (unchanged)

GitHub Actions billing failure persists. All prior runs failing in 5s with
the spending-limit message. New v3.36.0 push triggered 3 workflows
(build, deploy-try, Docker slab-server) — currently queued, will likely
fail until billing is fixed.

**Action for Sanjay**: https://github.com/settings/billing
→ update payment method OR raise spending limit.

After fix, re-run failed workflows:
- `gh run rerun 26429028704` (main build)
- `gh run rerun 26429028584` (v3.36.0 docker)

## This tick (2026-05-25 19:18–19:36 PT) — MODE A release tick

**Recovered disk + merged v3.36.0 to main.**

- `cargo clean` recovered 12.6 GiB (disk now 70% used, 10Gi free).
- Full `cargo test --lib`: 1801 passed, 1 failed → fixed schema_version
  assertion (4→5) for new personal_presets migration. `d0b39a6`.
- Re-ran failing test: green. Clippy clean. Fmt clean. pnpm check clean.
- Merged `feature/v3.36.0-personal-presets` → main (merge commit `8365004`).
- Tagged `v3.36.0` and pushed with `--follow-tags`.
- Net for v3.36.0: 5 commits, +1155 / -84 LOC across 15 files including
  new `personal_presets.rs` backend (535 LOC).

## Buy-Button qualification (v3.36.0)

- **Pay-for-it** ✅ Adobe Document Cloud charges per-seat for shared
  smart-template libraries; we ship them as a one-file `.slabpresets` drop.
- **Notice-it** ✅ "★ Personal presets" section in picker.
- **Pick-us** ✅ Preview/PDF Expert have nothing equivalent.
- **Tell-a-friend** ✅ Drop `.slabpresets` → 8 smart collections appear.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-26T01:18:00Z (`.slabpresets` import animation).
Next wow due by ~2026-05-27 01:18 UTC.

## Recently closed issues

- (none — release tick)

## Next ticks

- **Tick 1 (NEXT)**: If CI billing resolved, finalize v3.36.0 release —
  download artifacts from `gh run view <build-run>`, publish release with
  notes + DMGs/MSI/AppImage. Also clear v3.33.0 Draft.
- **Tick 2**: Delete merged feature branches:
  `feature/v3.34.0-atlas-smart-plus`, `feature/v3.35.0-atlas-presets`,
  `feature/v3.36.0-personal-presets`.
- **Tick 3**: Ship sample pack `assets/preset-packs/legal-starter.slabpresets`
  (3 legal-focused presets) as drop-in onboarding asset.
- **Tick 4**: v3.37.0 "Atlas Smart Folders Hub" — dedicated panel listing
  all built-in + personal presets side-by-side, drag-to-reorder.

## Pipeline state

| Branch                                  | Status                              | Notes                            |
| --------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                  | v3.36.0 merged + tagged + pushed    | CI queued, billing-blocked       |
| `feature/v3.36.0-personal-presets`     | merged → main                       | Safe to delete                   |
| `feature/v3.34.0-atlas-smart-plus`     | merged → main                       | Safe to delete                   |
| `feature/v3.35.0-atlas-presets`        | merged → main                       | Safe to delete                   |
