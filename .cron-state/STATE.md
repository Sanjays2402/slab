# Slab Cron State

Last updated: 2026-05-24 20:43 PT by Cake (cron)

## Active version

**v3.27.0 "Quill Auto-Detect" — SHIPPED. Tag pushed, CI in flight.**
- Merge SHA on main: bcfabb2
- Tag: v3.27.0
- CI runs in_progress: build 26381993650, deploy-try 26381993651, Docker(v3.27.0) 26381993649
- v3.26.0 fully green (build 26381227405, Docker v3.26.0 26381227331 — all completed/success).

## RELEASE_PENDING

v3.27.0 — merge SHA bcfabb2 on main, tag v3.27.0, CI runs 26381993650 / 26381993651 / 26381993649

v3.26.0 — CI all green; release artifacts still need to be downloaded + attached to the release page. Carry-over from previous tick.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T03:42:35Z (v3.27.0 Quill Auto-Detect — heuristic form-field detector; drag a flat PDF, get fillable AcroForm in one click. Adobe's "Prepare Form" replacement, free + offline.)

## Recently closed issues

(none — issues #23-#27 not re-polled this tick due to back-to-back releases; next tick should re-poll)

## What shipped this tick (v3.27.0 — 3 commits on feature branch + merge, ~2080 net LOC)

1. (prior tick) `pdf/forms_detect.rs` 845 LOC + 15 unit tests — DTOs, content-stream scan for rects/checkbox glyphs/labels, confidence scoring, dedup. Slices 1-5.
2. (prior tick) `b003731` — Tauri command `slab_forms_autodetect` + `ActionId::QuillAutodetectOpen` (Mod+Shift+Y, group Forms) + ActionId TS union. Slice 6.
3. `c5154f3` — `QuillAutodetectPanel.svelte` (753 LOC) + `+page.svelte` import/panel registry/keyboard handler/conditional render. Slice 7.
4. `6d346c9` — version bump 3.26.0→3.27.0 (Cargo.toml, package.json, tauri.conf.json) + release notes + clippy collapse fix in forms_detect.rs.
5. Merge `bcfabb2` to main --no-ff, tagged `v3.27.0`, pushed origin main --follow-tags.

Buy-Button PASS: Adobe Acrobat's auto-detect is THE selling point of "Prepare Form" ($239/yr). Slab now ships it free, offline, cross-platform, with confidence chips on every guess. Pairs with v3.26.0 Designer (manual authoring) and v3.25.0 Batch (CSV fill).

Quality gates green on main: cargo fmt --check, cargo clippy --all-targets -D warnings, cargo test --lib (1740 tests, 15 new), pnpm check (0 errors).

## Next ticks

- **MODE B (next tick)**: poll v3.27.0 CI run IDs above. On green, `gh release create v3.27.0 --notes-file docs/release-notes/v3.27.0.md` + download/attach artifacts.
- Also finalize v3.26.0 artifacts (still pending attach).
- Then re-poll `gh issue list` for #23-#27 priority override.
- Then v3.28.0 Quill Hub per `docs/plans/2026-05-24-v3.28.0-quill-hub.md` (queued).
