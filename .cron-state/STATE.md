# Slab Cron State

Last updated: 2026-05-24 20:14 PT by Cake (cron)

## Active version

**v3.26.0 "Quill Designer" — SHIPPED. Release published, CI in flight.**
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.26.0
- CI runs in_progress at push: build 26381227405, deploy-try 26381227396, Docker(v3.26.0) 26381227331
- Next tick (MODE B) should finalize: poll those run IDs, download artifacts, attach to release.

## RELEASE_PENDING

v3.26.0 — merge SHA 9d1928c on main, tag v3.26.0, CI runs 26381227405 / 26381227396 / 26381227331

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T03:13:50Z (v3.26.0 Quill Designer — author AcroForm fields on flat PDFs, the Adobe Acrobat Pro "Prepare Form" replacement, free + offline + cross-platform)

## Recently closed issues

(none touched this tick — issues #23-#27 override status not re-polled; will re-check next tick)

## What shipped this tick (Quill Designer — 3 commits, ~2080 net LOC)

1. backend commit (prior tick): `pdf/forms_design.rs` 1103 LOC, 15 unit tests — FieldDraft/FieldEdit/DesignReport, add_fields/edit_fields/delete_fields, AcroForm ensure + annot push via AnnotKind enum (clippy-clean)
2. `38814f7` — Tauri commands `slab_forms_design_add/edit/delete` + `ActionId::QuillDesignerOpen` (Mod+Shift+D, group Forms) + ActionId TS union + `QuillDesignerPanel.svelte` (~720 LOC Liquid Glass two-card layout) + sidebar nav + +page.svelte routing/key handler
3. `b8c2273` — version bump 3.25.0→3.26.0 (package.json, tauri.conf.json, Cargo.toml, Cargo.lock) + CHANGELOG entry
4. Merge `9d1928c` to main with --no-ff, tagged `v3.26.0`, pushed origin main --follow-tags, `gh release create` with marketing-grade notes

Buy-Button test PASS: Adobe Acrobat Pro charges $239/yr for "Prepare Form". PDF Expert has no equivalent. Foxit gates it at $129/yr. Slab does authoring (Text/Checkbox/Dropdown/Signature, edit-in-place, delete, error-reporting, auto re-inspect) for $0, offline, OSS, cross-platform. Pairs with v3.25.0 Quill Batch — design once, batch-fill from CSV.

Quality gates green on main: cargo fmt --check, cargo clippy --lib --all-targets -D warnings, cargo test --lib (1725 tests, 15 new), pnpm check (0 errors, 72 pre-existing a11y warnings).

## Next ticks

- **MODE B (next tick)**: finalize v3.26.0 release — poll CI run IDs above, on green run `gh run download` for the build run, attach the 6 artifacts (macos arm64/x64 dmg, linux deb + AppImage, windows msi + nsis) to the existing v3.26.0 release. Clear RELEASE_PENDING.
- Then re-poll `gh issue list` (issues #23-#27 may still be priority override).
- Then v3.27.0 Quill Autodetect (heuristic field detector) per queued plan.

## Plans queued (in execution order)

- `docs/plans/2026-05-24-v3.27.0-quill-autodetect.md` — heuristic field detector (detect underscores, checkboxes, signature lines on flat PDFs).
- `docs/plans/2026-05-24-v3.28.0-quill-hub.md` — unified QuillHubPanel (capstone uniting Inspect/Fill/Batch/Designer/Autodetect).
- `docs/plans/2026-05-24-show-hn-launch.md` — 8-task launch-prep plan. Execute AFTER v3.28.0. Two Sanjay action items inside: record demo, take screenshots.
