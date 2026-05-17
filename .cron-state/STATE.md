# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: DONE — v0.11.0 Lathe ready to merge

**v0.10.0 RELEASED** 2026-05-17 08:03 UTC — all 6 installers on GH Releases, latest.
**v0.11.0 DONE** 2026-05-17 02:55 PDT — all 8 slices shipped on `feature/v0.11.0-lathe`.

**Next tick:** MODE A merge `feature/v0.11.0-lathe` → `main`, tag `v0.11.0`, push, set `RELEASE_PENDING: v0.11.0`.

**Lathe slices (8 / 8 done):**
- ✅ Slice 1: `duplicate_pages` kernel + Tauri (commit `e95c0ef`)
- ✅ Slice 2: `split_by_pattern` chapter splitter backend (commit `d0635a7`)
- ✅ Slice 3 backend: `pages_build` composite kernel — handles permutations + duplicates + blank inserts + per-cell rotation in one Tauri round-trip; 14 unit tests (commit `3e93038`)
- ✅ Slice 3 UI: `PagesVisualPanel.svelte` drag-reorder grid (commit `5807fe4`)
- ✅ Slice 4: `SplitPatternPanel.svelte` regex+outline split UI with 5 presets + live preview (commit `3393697`)
- ✅ Slice 5: Multi-PDF tabs in main shell (commits `5af4a92` + `a3e0f49`)
- ✅ **Slice 6: `pdf::edit_text` backend** — `find_text_spans` + `replace_text_span` for ASCII Tj/TJ rewrite. 15 unit tests cover happy path + CID Type0 read-only + non-ASCII read-only + multi-segment TJ kerning read-only + multi-page id distinctness + replace round-trip with extract_text proof + control-char rejection + span-id parser. 2 Tauri commands. 220→235 lib tests (commit `46b0f3c`)
- ✅ **Slice 7: `EditTextPanel.svelte`** — page-tab strip with editable counts, editable-only/all filter toggle, span rows with id pill + font/size + inline input + "edited" pill + revert link, read-only chips with friendly reason, iterative save (chain replace calls, reload on success), collapsible caveats section listing the ASCII-only / Type1-only / no-kerning limitations honestly. Sidebar nav `✎ Edit Text`. (commit `4e5257a`)
- ✅ **Slice 8: Release prep** — version bumped 0.10.0 → 0.11.0 in Cargo.toml + tauri.conf.json + package.json. Cargo.lock refreshed. Sidebar version pill updated. Release notes written to `docs/release-notes/v0.11.0.md` covering all 5 user-visible features. (commit `79f820a`)

**Quality gates green on `feature/v0.11.0-lathe`:**
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib` — 235 passed
- `pnpm exec svelte-check` — 0 errors

---

## ROADMAP

### v0.8.1 "Polyglot" — RELEASED 2026-05-16
- Tag `v0.8.1`, merge SHA `39ff562`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.8.1)

### v0.9.0 "Toolkit" — RELEASED 2026-05-16
- Tag `v0.9.0`, merge SHA `ba3b291`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.0)

### v0.9.1 "Toolkit UX" — RELEASED 2026-05-16
- Tag `v0.9.1`, merge SHA `7226574`, CI run `25980874364`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.1)

### v0.10.0 "Beacon" — RELEASED 2026-05-17
- Tag `v0.10.0`, merge SHA `f91b374`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.10.0)

### v0.11.0 "Lathe" — DONE 2026-05-17, awaiting merge
- 8 slices shipped on `feature/v0.11.0-lathe`. All quality gates green. Release notes at `docs/release-notes/v0.11.0.md`.
- Headline: in-place PDF text editing (ASCII / Type1+TrueType). Backend `pdf::edit_text` + frontend `EditTextPanel`.

### v0.12.0 "Atlas" — Library Mode (PLANNED)
Cross-doc Beacon chat across indexed library, tags, collections, watch folders.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.12.0. 7 slices.

### v0.13.0 "Lens" — OCR + Vision (PLANNED)
Local OCR (surya/tesseract), table → CSV, math → LaTeX, vision Q&A in Beacon, auto-tag. **Also where CID/Unicode text-edit work lands** (CMap rewriting, font subsetting).
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.13.0. 9 slices.

### v0.14.0 "Stack" — Diff & Compare (PLANNED)
Visual + text diff, track changes, patch/merge, Beacon diff summary.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.14.0. 6 slices.

### v0.15.0 "Theater" — Presenter Mode (PLANNED)
Slides view, presenter window, live drawing, auto-advance, Stream Deck profile.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.15.0. 5 slices.

### v1.0.0 "Glass" — Stable Release (PLANNED)
Floating panels, multi-window, command palette (⌘K), Vim bindings, a11y, i18n, frozen API.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v1.0.0. 10 slices.

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
GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin <branch-or-tag>
```

### Merge to main (PERMISSIONS GRANTED 2026-05-16):
```bash
# Verify gates on feature branch first
git checkout main && git pull --ff-only
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    merge --no-ff feature/vX.Y.Z-name -F /tmp/merge-msg-vX.Y.Z.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    tag -a vX.Y.Z -m "Slab vX.Y.Z — Codename"
# push main, push tag (separate calls)
```

### Release finalize (MODE B):
```bash
# 1. Download artifacts from CI run
mkdir -p /tmp/slab-vX.Y.Z-release
gh run download <RUN_ID> -R Sanjays2402/slab -D /tmp/slab-vX.Y.Z-release/
# 2. Stage in assets/ (gitignored) — rename mac x64 dmg
mkdir -p assets/vX.Y.Z
cp .../Slab_X.Y.Z_aarch64.dmg assets/vX.Y.Z/
cp .../Slab_X.Y.Z_x64.dmg assets/vX.Y.Z/Slab_X.Y.Z_x64_macos.dmg
cp .../*.{deb,AppImage,msi,exe} assets/vX.Y.Z/
# 3. Build release body from docs/release-notes/vX.Y.Z.md
# 4. Create release. Big assets (76MB AppImage) sometimes time out the
#    single `gh release create` call — use a background process,
#    then `gh release edit vX.Y.Z --draft=false --latest` once all 6 assets are up.
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
- `TMPDIR` stale: if `mktemp -d` left a deleted dir as `TMPDIR`, tests
  fail with `PathError NotFound`. Workaround: `unset TMPDIR` before tests.
- Version bump lockstep: editing `src-tauri/Cargo.toml` version requires
  running `cargo build` and committing `Cargo.lock` in the SAME commit.
- `markitdown` runtime: `/Users/sanjay/.local/bin/markitdown` (pipx).
  Add `$HOME/.local/bin` to PATH for cron-spawned terminals.
- `CmdResult<T>` field on `"ok"` variant is `value`, NOT `data`.
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁 ✦ ⌕ 🔒 ✂ ≣ ✎
- `gh release create` with 6 assets including the 76MB AppImage often
  times out at 60s in foreground. Run it in `background=true` or upload
  the AppImage with a follow-up `gh release upload` and then
  `gh release edit --draft=false --latest`.

### Release asset naming
- Mac x64 dmg needs `_x64_macos.dmg` rename (disambiguate from Windows x64).
- Standard set: 1 dmg per mac arch + 1 deb + 1 AppImage (linux) + 1 msi + 1 setup.exe (windows).

---

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises.
- 2026-05-16 17:00 (Cake/cron): Task 2 done. Pure-fn allow-list + 3 tests. fmt/clippy clean, full suite 84 pass (81→84). Pushed `c66167c`.
- 2026-05-16 17:18 (Cake/cron): Task 3 done. `require_markitdown()` + `markitdown_available()` test gate. Suite 84→86 (+2). Pushed `5b7d9b9`. **Plan deviation**: replaced literal-error-format test with two real preflight tests gated on `markitdown_available()`.
- 2026-05-16 17:37 (Cake/cron): Task 4 done. Real pipeline + 2 cheap unit tests. Suite 86→88 (+2). Pushed `37b9356`.
- 2026-05-16 17:55 (Cake/cron): Task 5 done. html_round_trip test added. Pushed `dff9ca0`.
- 2026-05-16 18:12 (Cake/cron): **Aggressive tick — shipped 7 sub-tasks** (Tasks 6,7,8,9,11,12,13). Backend + CLI + Tauri + docs + version bump. Suite 89→90.
- 2026-05-16 18:43 (Cake/cron): **Closeout tick** — shipped Task 10, verified Task 14 manually, v0.8.1 PR_READY: true. Commit: `ac401be`.
- 2026-05-16 19:05 (Cake/cron): **v0.9.0 kickoff** — Feature A (flatten) + Feature B (sanitize) shipped in one tick.
- 2026-05-16 19:34 (Cake/cron): **v0.9.0 closeout** — picked up WIP repair backend, wired CLI + Tauri, version bumped, release notes. 101 lib tests. PR_READY.
- 2026-05-16 20:00 (Cake/cron): **Held-pattern tick** — fixed Cargo.lock drift `60945b6`, drafted v0.9.1 plan into proposals/.
- 2026-05-16 20:21 (Cake/cron): **Override-and-ship tick** — stacked v0.9.1 on top of v0.9.0. Shipped Tick-1 + Tick-2 in one tick: plan promotion + FlattenPanel + SanitizePanel + nav + RepairPanel + nav. 4 quality gates green.
- 2026-05-16 20:36 (Cake/cron): **🚀🚀 DOUBLE-RELEASE TICK** — Sanjay granted direct-merge permission. Cleared 3-tick PR_READY backlog: merged both `feature/v0.8.1-polyglot` (SHA `39ff562`, tag `v0.8.1`) and `feature/v0.9.0-toolkit` (SHA `ba3b291`, tag `v0.9.0`) to main. Both CI runs queued. Quality gates re-verified on each before merge (90 + 101 lib tests).
- 2026-05-16 21:25 (Cake/cron): 🚀 RELEASE + KICKOFF TICK: v0.9.1 published; v0.10.0 Beacon Slice 1 shipped.
- 2026-05-16 21:46 (Cake/cron): Beacon Slice 2 (provider abstraction) shipped in 4 commits.
- 2026-05-16 22:45 (Cake/cron): 🚀 TRIPLE-SLICE TICK — Beacon Slices 3+4+5 in one tick. 121→133 lib tests. v0.10.0 50% shipped.
- 2026-05-16 23:50 (Cake/cron): 🚀 Beacon Slice 9 — Selection Actions vertical slice in 2 commits. 173→186 lib tests. v0.10.0 90% shipped.
- 2026-05-17 00:09 (Cake/cron): 🚀 v0.10.0 "Beacon" MERGED TO MAIN.
- 2026-05-17 01:20 (Cake/cron): 🚀 BIG LATHE TICK — Slice 3 (backend+UI) + Slice 4 in 3 commits. v0.11.0 50% shipped.
- 2026-05-17 02:30 (Cake/cron): 🚀 BIG LATHE TICK — Slice 5 multi-PDF tabs (5af4a92 + a3e0f49). v0.11.0 62% shipped.
- **2026-05-17 02:55 (Cake/cron): 🚀🚀 TRIPLE-SLICE LATHE CLOSEOUT — Slices 6 + 7 + 8 in one tick.** Took v0.11.0 from 62% → 100% DONE. (a) Slice 6 `pdf::edit_text` backend: 818 LOC including 15 unit tests. `find_text_spans` walks every page's content streams, tracks Tf/Td/Tm/T*/BT/ET state, emits one TextSpan per Tj/TJ/'/' op with stable `p<page>:s<seq>` ids. `replace_text_span` re-decodes the target stream, finds the op by sequence number, swaps its literal-string operand, re-encodes. Handles single Tj happy path, single-segment TJ arrays (editable), multi-segment TJ (read-only, kerning hint), CID Type0 fonts (read-only, font_is_safe check), non-ASCII glyphs in source (read-only), and rejects non-ASCII replacements + control chars + malformed span-ids. 220→235 lib tests (+15). 2 new Tauri commands (commit `46b0f3c`). (b) Slice 7 `EditTextPanel.svelte` 530 LOC: file picker → scans on open → groups spans by page in a tab strip with editable counts + "•" dirty indicator → editable-only/all filter toggle → span rows with id pill + font/size badge + inline `<input>` bound to current value + "edited" pill + revert link → read-only rows with friendly reason chips → Apply chains replace calls writing to a save-dialog destination, then re-loads the saved file so user can keep editing → collapsible caveats section that's honest about ASCII/Type1/no-kerning limits. Sidebar nav `✎ Edit Text`. svelte-check 0 errors (commit `4e5257a`). (c) Slice 8 release prep: version bumped 0.10.0→0.11.0 in Cargo.toml + tauri.conf.json + package.json, Cargo.lock refreshed, sidebar version pill bumped, comprehensive release notes at `docs/release-notes/v0.11.0.md` covering all 5 user-visible Lathe features + stats + upgrade notes (commit `79f820a`). All 4 quality gates green: fmt + clippy `-D warnings` + 235 lib tests + svelte-check 0 errors. Branch pushed. STATE marked DONE — next tick is MODE A merge to main + tag v0.11.0 + push.
