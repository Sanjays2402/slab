# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🗄 v1.1.0 "Cabinet" Slices 1+2+3+4 SHIPPED — 9 commits on `feature/v1.1.0-cabinet` (pushed)

**Branch**: `feature/v1.1.0-cabinet` (pushed to origin, 9 commits ahead of main)

**HEAD**: `91561da` — `feat(cabinet): persist + restore detached windows to ~/.slab/windows.json (Slice 4)`

**Quality gates green** on branch HEAD:
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (468 passed — 5 new persistence tests on top of 463)
- `pnpm exec svelte-check` ✓ (0 errors / 28 baseline warnings)

---

## TICK 2026-05-17 ~13:55 PT — v1.1.0 Cabinet Slices 3 + 4 in one tick (3 commits)

Sanjay's "ship BIG things every tick" + "add very big features" directive
in full force: pulled Slices 3 *and* 4 into a single tick on top of the
already-shipped 1+2. Halfway through Cabinet in two ticks.

### Slice 3 — Detach action end-to-end (Beacon + 10 more panels)

- ✅ **Task 3.1 `3b8fa56`** — `src/lib/windows.ts` created. Type-safe
  wrapper around `slab_window_open/close/list`. Converts snake_case
  serde shape → camelCase on the way out. Lazy-imports
  `@tauri-apps/api/webviewWindow` for the focus helper so non-Tauri
  builds (vite dev / SSR / vitest) pay nothing. Public API:
  `openPanelWindow`, `closePanelWindow`, `listPanelWindows`,
  `focusPanelWindow`. All four are graceful no-ops outside Tauri so
  callers don't need to guard `isInTauri()` at every site.

- ✅ **Task 3.2 `ded6db2`** — `⤢` detach button on the **active**
  sidebar entry. Sidebar markup grew a wrapper `<div class="nav-row">`
  per feature so a non-nested `<button class="detach-btn">` could live
  beside the existing `.nav-item` button. Hovers and focus-visible
  styles match the rest of the sidebar. The button only renders when:
  the panel is currently active, `f.ready === true`, the id is in
  `DETACHABLE_PANELS` (11 panels — durable ones, no one-shot wizards),
  AND `isInTauri()` (so it never shows in the dev shell).

  `DETACHABLE_PANELS` = reader, library, beacon, search, pii, pages,
  pages-list, diff, slides, tables, markdown.

### Slice 4 — Persistence to `~/.slab/windows.json` + launch restore

All three Slice 4 tasks landed as one commit `91561da` because they're
tightly coupled — restore without autosave is meaningless, and autosave
without restore wastes disk:

- **Persistence helpers** in `windows.rs`: `windows_json_path()`,
  `save_windows()` (atomic-ish — temp file + rename), `load_windows()`
  (Ok([]) on missing file, parse errors propagate with offending path).
  Honours `SLAB_HOME_OVERRIDE` + `SLAB_CONFIG_DIR` env vars for tests.

- **Autosave** via extracted `wire_window_events()` helper that's now
  used by *both* `slab_window_open` *and* the launch-restore path.
  Persists on Destroyed/Moved/Resized events. `flush_to_disk()`
  swallows IO errors with an `eprintln!` — never blocks the in-app
  detach flow.

- **Launch restore** via new `setup(|app| { ... })` hook in
  `lib::run`. Calls `windows::restore_windows(&handle)` which reads
  the JSON, re-spawns each saved window at its persisted geometry,
  re-wires the same event handlers, and re-flushes the registry so
  failed-to-restore entries are dropped from disk.

- **Hard cap**: `MAX_DETACHED_WINDOWS = 6`. Each WebviewWindow burns
  30-60 MB RSS; past 6 we degrade the user's machine. Enforced on
  both `slab_window_open` and `restore_windows`.

- **Tests**: 5 new `windows::tests::*` — `save_then_load_roundtrips`,
  `load_returns_empty_when_file_missing`, `save_creates_parent_directory`,
  `save_overwrites_previous_contents`, `load_returns_error_on_corrupt_json`.
  Share a process-level `ENV_LOCK` Mutex because env mutation is
  global state and Cargo's parallel runner would race them.

### What's still needed before Cabinet release

**Next tick (Slice 5 + 6)**:
- Slice 5: parameterised detach — Library row "Open in new Reader window",
  Beacon "Open chat in new window from selection bubble", and
  Command Palette entries `"Open <panel> in new window"`. Should
  expand the `{#if detached}` branch in `+page.svelte` to cover all
  11 panels in DETACHABLE_PANELS (currently only renders 5 — beacon,
  library, search, pii, reader). (3-4 commits estimated.)
- Slice 6: cross-window events — `slab://recent-changed` so detached
  Library windows refresh when main opens a doc, and a new
  `slab_request_open_in_main(path)` command so a detached Library
  click opens the doc in the main reader. (3 commits estimated.)

**After that (Slices 7 + 8)**:
- Slice 7: Window menu in main sidebar listing detached windows
  (`{panel} — {doc} (×)`) with click-to-focus + close-x, plus toast
  confirmations on detach/close.
- Slice 8: version bump 1.0.0 → 1.1.0 lockstep + release notes +
  `gh release create v1.1.0`.

At current velocity (2 slices/tick) v1.1.0 ships in 2 more ticks.
That's **3 ticks total** for Cabinet — already 1 ahead of the original
estimate.

---

## PRIOR TICK STATE (kept for reference)

## STATUS-PRIOR: ✅ v1.0.0 "Glass" RELEASED 🎉🎉🎉

**Release**: https://github.com/Sanjays2402/slab/releases/tag/v1.0.0
**Published**: 2026-05-17 20:06 UTC
**Tag**: `v1.0.0` (SHA `509008c` on `main`)

Slab 1.0 is shipped. Stable-API promise is now live. v0.x train closed
(14 feature releases between v0.8.1 Polyglot on 2026-05-16 and v1.0.0
Glass on 2026-05-17 — 14 versions in 36 hours).

---

## ROADMAP

### v0.8.1 "Polyglot" — RELEASED 2026-05-16
### v0.9.0 "Toolkit" — RELEASED 2026-05-16
### v0.9.1 "Toolkit UX" — RELEASED 2026-05-16
### v0.10.0 "Beacon" — RELEASED 2026-05-17
### v0.11.0 "Lathe" — RELEASED 2026-05-17
### v0.12.0 "Atlas" — TAGGED, NOT RELEASED (CI artifacts skipped)
### v0.13.0 "Lens" — TAGGED, NOT RELEASED (Windows pdftotext bug)
### v0.13.1 "Lens Patch" — RELEASED 2026-05-17
### v0.14.0 "Stack" — RELEASED 2026-05-17 (diff & compare)
### v0.15.0 "Theater" — RELEASED 2026-05-17 (presenter mode)
### v1.0.0 "Glass" — RELEASED 2026-05-17 🎉🪟
### v1.1.0 "Cabinet" — IN PROGRESS (Slices 1-4 of 8 done — 50% in 2 ticks) 🗄

### v1.2.0 "Glass II" (later)
- Vim bindings, a11y, i18n

### v1.3.0 "Foundry" (much later)
- Plugin API, community-extensible

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. If RELEASE_PENDING set AND CI for that tag is "success":
     → MODE B (download artifacts, gh release create, clear RELEASE_PENDING)
3. Else if any feature branch has STATUS: DONE locally and was not merged:
     → MODE A (merge --no-ff to main, tag, push)
4. Else:
     → MODE C (develop next feature on active branch)
5. Mode chaining is allowed within a tick if there's time.
```

---

## QUICK REFERENCE

### Quality gates (run from `src-tauri/`):
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib`
- `pnpm exec svelte-check` (run from repo root)

### Push (manual auth needed):
```bash
TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' '$TOK'; }; f" push origin <branch-or-tag>
```

### Merge to main (PERMISSIONS GRANTED 2026-05-16):
```bash
git checkout main && git pull --ff-only
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    merge --no-ff feature/vX.Y.Z-name -F /tmp/merge-msg-vX.Y.Z.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    tag -a vX.Y.Z -m "Slab vX.Y.Z — Codename"
```

### Release finalize (MODE B):
```bash
mkdir -p /tmp/slab-vX.Y.Z-release
gh run download <RUN_ID> -R Sanjays2402/slab -D /tmp/slab-vX.Y.Z-release/
mkdir -p assets/vX.Y.Z
cp .../Slab_X.Y.Z_aarch64.dmg assets/vX.Y.Z/
cp .../Slab_X.Y.Z_x64.dmg assets/vX.Y.Z/Slab_X.Y.Z_x64_macos.dmg
cp .../*.{deb,AppImage,msi,exe} assets/vX.Y.Z/
gh release create vX.Y.Z --title 'vX.Y.Z — Codename emoji' --notes-file body.md assets/vX.Y.Z/*
```

### NO PRs.
Direct merge to main is the workflow. Branch protection on main is OFF.
Never run `gh pr create`.

### Commit author:
```bash
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit ...
```

### Gotchas
- `TMPDIR` stale: if `mktemp -d` left a deleted dir as `TMPDIR`, tests fail with `PathError NotFound`. Workaround: `unset TMPDIR` before tests.
- Version bump lockstep: editing `src-tauri/Cargo.toml` version requires running `cargo check` and committing `Cargo.lock` in the SAME commit.
- `markitdown` runtime: `/Users/sanjay/.local/bin/markitdown` (pipx). Add `$HOME/.local/bin` to PATH for cron-spawned terminals.
- `CmdResult<T>` field on `"ok"` variant is `value`, NOT `data`.
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁 ✦ ⌕ 🔒 ✂ ≣ ✎ ⌨ — plus ⤢ reserved for detach button (Slice 3).
- `gh release create` with 6 assets including the 76MB AppImage often times out at 60s in foreground. Run in `background=true` or upload AppImage with follow-up `gh release upload` and then `gh release edit --draft=false --latest`.
- **Cabinet**: Tauri 2 capability glob `panel-*` is in `default.json` so detached windows inherit `slab_*` permissions. Without it the child windows feel "dead" — every invoke silently fails.

### Release asset naming
- Mac x64 dmg needs `_x64_macos.dmg` rename (disambiguate from Windows x64).
- Standard set: 1 dmg per mac arch + 1 deb + 1 AppImage (linux) + 1 msi + 1 setup.exe (windows).

---

## NOTES FROM PRIOR SESSIONS

(Older notes pruned to keep STATE.md focused on the current arc. See
git log on STATE.md or .cron-state/sessions/*.md for the full history
back to 2026-05-16.)

- 2026-05-17 12:40 (Cake/cron): 🎉🎉🎉 v1.0.0 "Glass" RELEASE PREP + MODE A MERGE in one tick. Version bumped lockstep, release notes ~8.6 KB, 4 quality gates green, merged --no-ff, tagged v1.0.0, pushed main + tag. CI run `26000852284`.
- 2026-05-17 13:06 (Cake/cron): MODE B FINALIZE — CI green at 13:04 PT, downloaded all artifacts, curated 6, created v1.0.0 release with all assets (AppImage uploaded separately to dodge 60s timeout). Then used writing-plans skill to author v1.1.0 "Cabinet" plan (~44 KB, 8 slices, ~20 tasks).
- 2026-05-17 13:30 (Cake/cron): v1.1.0 Cabinet Slices 1+2 shipped end-to-end in 6 commits. Backend WindowRegistry + 3 Tauri commands + capability glob; frontend DetachedShell component + URL-driven detached-mode branch in `+page.svelte`. 4 quality gates green (463 cargo tests, 0 svelte-check errors). Branch pushed.
- 2026-05-17 13:55 (Cake/cron): v1.1.0 Cabinet Slices 3+4 shipped in 3 commits. Slice 3: `src/lib/windows.ts` typed wrapper + detach `⤢` button on active sidebar item (only renders for the 11 DETACHABLE_PANELS, only inside Tauri). Slice 4: persistence to `~/.slab/windows.json` (atomic temp+rename) + autosave on Destroyed/Moved/Resized + launch restore via new `setup` hook + `MAX_DETACHED_WINDOWS=6` cap. 5 new persistence tests (`ENV_LOCK` mutex to serialise env mutation). 4 quality gates green (468 cargo tests). Branch pushed. **Cabinet 50% done in 2 ticks** — Slices 5+6 next, then release in tick 4.
