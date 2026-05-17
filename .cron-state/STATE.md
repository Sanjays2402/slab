# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🗄 v1.1.0 "Cabinet" Slices 1+2 SHIPPED — 6 commits on `feature/v1.1.0-cabinet` (pushed)

**Branch**: `feature/v1.1.0-cabinet` (pushed to origin, 6 commits ahead of main)

**HEAD**: `8c84e9e` — `feat(cabinet): detached-mode route + conditional panel render`

**Quality gates green** on branch HEAD:
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (463 passed — 12 new windows::tests added on top of 451 baseline)
- `pnpm exec svelte-check` ✓ (0 errors / 28 baseline warnings)

---

## TICK 2026-05-17 ~13:30 PT — v1.1.0 Cabinet Slices 1 + 2 in one tick (6 commits)

Shipped the **entire backend window-registry plumbing + the frontend
detached-mode route** end-to-end. The plan in `docs/plans/2026-05-17-v1.1.0-cabinet.md`
estimated 2 ticks for these two slices; pulled both into one as
directed by Sanjay ("ship BIG things every tick").

### Slice 1 — Backend (Tauri WebviewWindow plumbing)

- ✅ **Task 1.1 `09db0bb`** — `src-tauri/src/windows.rs` created with
  `WindowState` + `Geometry` serde shapes. `mod windows;` added to
  `lib.rs`. 2 roundtrip tests.
- ✅ **Task 1.2 `dfdfb97`** — `WindowRegistry` (Mutex<HashMap>) with
  upsert/get/remove/list/next_label. `next_label("beacon")` returns
  `panel-beacon-N` where N is one higher than the max existing N
  (closed-window slots are NOT recycled mid-session — stable labels).
  5 unit tests.
- ✅ **Task 1.3 `9b476a9`** — Three Tauri commands wired:
  `slab_window_open(panel_id, target_doc?)` spawns a child
  `WebviewWindowBuilder`, `slab_window_close(label)` graceful close,
  `slab_window_list()` snapshot. Auto-removal on `WindowEvent::Destroyed`
  so the registry stays accurate when the user clicks X. Helpers:
  `default_geometry_for_panel` (Beacon 520×760 tall, Library 1000×720
  wide, Reader 900×760), `encode_doc_param` (minimal URL escape for
  space/#/&/?/%), `title_case` ("beacon"→"Beacon", "pii"→"PII"). 5
  more unit tests. `.manage(WindowRegistry::new())` on the builder.
- ✅ **Task 1.4 `27c0998`** — `capabilities/default.json` `windows`
  array changed from `["main"]` to `["main", "panel-*"]` so detached
  windows inherit the full `slab_*` command surface.

### Slice 2 — Frontend (detached route + shell component)

- ✅ **Task 2.2 `32355f6`** — `src/lib/components/DetachedShell.svelte`
  created. Thin 32px titlebar (panel name + "Slab" badge) + flex body
  that lets the hosted panel fill the window. Uses Svelte 5 Snippets
  for children. No runtime JS beyond props.
- ✅ **Task 2.1 + 2.3 `8c84e9e`** — `+page.svelte` flips into
  detached mode on URL params:
  - New `$state` vars: `detached`, `detachedPanel`, `detachedWindowId`,
    `detachedDoc`.
  - `onMount` parses `?panel=&windowId=&doc=` from
    `window.location.search`; if `panel` + `windowId` are present,
    `detached = true` and `active = panel`.
  - Whole template wrapped in `{#if detached}...{:else}<existing
    sidebar+tabstrip>{/if}`. Detached branch renders one of:
    BeaconChatPanel, LibraryPanel, BeaconSearchPanel, BeaconPiiPanel,
    or ReaderPanel(initialPath=detachedDoc, tabId="detached"). Unknown
    panel ids show a friendly hint (no crash).
  - `titleForPanel(id)` helper for the DetachedShell titlebar.

Main-window behaviour is byte-for-byte unchanged (detached defaults to
false). Backwards-compat preserved.

### What's still needed before Cabinet release

**Next tick (Slice 3 + 4)**:
- Slice 3: `src/lib/windows.ts` (typed Tauri wrapper) + the actual
  "Detach" `⤢` button on each detachable sidebar item. End-to-end
  smoke: click on the Beacon row → new native window opens with a
  working Beacon chat. (3 commits estimated.)
- Slice 4: Persistence to `~/.slab/windows.json` so a relaunch
  restores last-session windows. Hook geometry-save on resize/move +
  load on app boot. (4 commits estimated.)

**After that (Slices 5-8)**:
- Slice 5: parameterised detach for *any* panel.
- Slice 6: cross-window events (Library row click → open file in
  detached Reader window via `tauri::emit_to`).
- Slice 7: Window menu + Command Palette entries
  ("Detach: Beacon", "Close all panel windows", etc.).
- Slice 8: version bump → 1.1.0 + release notes + `gh release create`.

Be aggressive: aim to ship Slices 3+4 in tick #2, Slices 5+6 in tick
#3, then Slice 7+8 (release) in tick #4. That's 4 ticks total to
v1.1.0 if the world cooperates.

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
### v1.1.0 "Cabinet" — IN PROGRESS (Slices 1+2 of 8 done) 🗄

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
