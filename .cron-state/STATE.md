# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🎉🎉🎉 v1.0.0 "Glass" MERGED + TAGGED — RELEASE_PENDING

**Merge SHA**: `509008c` on `main`
**Tag**: `v1.0.0`
**CI run**: `26000852284` (in_progress as of 12:50 PT)
**Branch**: `feature/v1.0.0-glass` ready to delete post-release

This is **Slab's first stable release**. The v0.x train (14 feature
releases since v0.8.1 "Polyglot" on 2026-05-16) closes here. Stable-API
promise applies starting now.

---

## TICK 2026-05-17 ~12:40 PT — v1.0.0 RELEASE PREP + MODE C → MODE A CHAIN (3 commits → merge)

Used the writing-plans skill to author the release prep plan, then
executed the whole thing in the same tick. Plan saved at
`docs/plans/2026-05-17-v1.0.0-glass-release-prep.md`.

- ✅ **Plan committed + version bump + release notes** (`3f1774d`) —
  lockstep 0.15.0 → 1.0.0 across `src-tauri/Cargo.toml`, `package.json`,
  `src-tauri/tauri.conf.json`, sidebar pill in `+page.svelte`, and
  `Cargo.lock` refresh via `cargo check`. Comprehensive release notes
  at `docs/releases/v1.0.0.md` (~8.6 KB) covering all 7 Glass slices +
  stable-API promise + deferred items (floating panels, perf pass,
  Vim/a11y/i18n) + upgrade notes + stats + next-version roadmap
  (v1.1.0 Cabinet, v1.2.0 Glass II, v1.3.0 Foundry).
- ✅ **MODE A merge** (`509008c`) — `git merge --no-ff
  feature/v1.0.0-glass -F /tmp/merge-msg-v1.0.0.md`. Rich merge
  message lists all 15 Glass commits across the 7 slices with their
  SHAs. No conflicts.
- ✅ **Tag v1.0.0 + push main + tag** — `git tag -a v1.0.0 -m "Slab
  v1.0.0 — Glass 🪟"`, then `git push origin main --follow-tags`.

**Quality gates green on main at HEAD `509008c`:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (451 passed)
- `pnpm exec svelte-check` ✓ (0 errors / 28 baseline warnings)

(Gates also ran green pre-merge on `feature/v1.0.0-glass` HEAD `3f1774d`.)

**Next tick (MODE B finalize):**
- Poll CI run `26000852284`.
- If green: download artifacts to `/tmp/slab-release-v1.0.0`, curate the
  6 best (macos-arm64 dmg, macos-x64 dmg renamed `_x64_macos.dmg`,
  linux x64 deb + AppImage, windows msi + nsis-setup.exe).
- `gh release create v1.0.0 --title 'v1.0.0 — Glass 🪟' --notes-file
  docs/releases/v1.0.0.md <assets>`. Watch for AppImage 60s timeout
  gotcha — may need background process + `gh release upload` fallback
  then `gh release edit --draft=false --latest`.
- Remove `RELEASE_PENDING` from STATE.md once release is up.

**After v1.0.0 ships:**
- Pivot to v1.0.1 (floating panels OR perf pass — pick one, ship in
  1-2 ticks).
- Or v1.1.0 "Cabinet" if floating panels expand to multi-window etc.

---

## PRIOR TICK STATE (kept for reference)

## STATUS-PRIOR: v1.0.0 "Glass" IN PROGRESS — 15 commits on `feature/v1.0.0-glass` (Slice 7 DONE end-to-end)

**v0.15.0 Theater RELEASED** 2026-05-17 ~10:35 PT — https://github.com/Sanjays2402/slab/releases/tag/v0.15.0 (Theater 🎭) — 6 assets up, isLatest=true.

**Active branch**: `feature/v1.0.0-glass` (15 commits ahead of main, pushed to origin)

## TICK 2026-05-17 ~12:15 PT — Slice 7 frontend half (Customizable Shortcuts) — END-TO-END DONE

Tasks 5-8 shipped this tick. Slice 7 is now complete in 6 commits across two ticks.

- ✅ **Task 5: $lib/keymap.ts** (`d29c806`) — runtime store + matches() module (~290 lines).
- ✅ **Task 6: Wire global shortcuts to live keymap** (`923280a`).
- ✅ **Task 7: KeymapPanel.svelte** (`5740179`) — Linear-style customisation UI (~458 lines).
- ✅ **Task 8: Command Palette entry + STATE update** (`4d99e5a`).

**Quality gates green on `feature/v1.0.0-glass` HEAD `4d99e5a`:** fmt ✓ clippy ✓ 451 tests ✓ svelte-check 0 errors.

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
### v1.0.0 "Glass" — RELEASE_PENDING (this tick) 🎉

### v1.0.1 / v1.1.0 candidates
- **Floating panels** — multi-window Beacon/Library (2-3 ticks)
- **Performance pass** — 100-page open <500ms (2-3 commits)

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
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁 ✦ ⌕ 🔒 ✂ ≣ ✎ ⌨
- `gh release create` with 6 assets including the 76MB AppImage often times out at 60s in foreground. Run in `background=true` or upload AppImage with follow-up `gh release upload` and then `gh release edit --draft=false --latest`.

### Release asset naming
- Mac x64 dmg needs `_x64_macos.dmg` rename (disambiguate from Windows x64).
- Standard set: 1 dmg per mac arch + 1 deb + 1 AppImage (linux) + 1 msi + 1 setup.exe (windows).

---

## NOTES FROM PRIOR SESSIONS

(Older notes pruned to keep STATE.md focused on the current arc. See
git log on STATE.md or .cron-state/sessions/*.md for the full history
back to 2026-05-16.)

- 2026-05-17 12:15 (Cake/cron): Slice 7 frontend half — Customizable Shortcuts end-to-end DONE in 4 commits (Tasks 5-8 of the keymap plan).
- 2026-05-17 12:40 (Cake/cron): 🎉🎉🎉 v1.0.0 "Glass" RELEASE PREP + MODE A MERGE in one tick. Version bumped lockstep, release notes written (~8.6 KB), 4 quality gates green pre-merge AND post-merge, merged with --no-ff, tagged v1.0.0, pushed main + tag. CI run `26000852284`. Next tick: MODE B finalize when CI green.
