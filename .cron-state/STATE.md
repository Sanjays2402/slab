# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: RELEASE_PENDING — v0.10.0 (waiting on retag CI)

**RELEASE_PENDING: v0.10.0** — tag pushed at commit `8c757bf` after a hotfix; CI run `25984687462` still bundling at tick end.

**Why the retag**: prior CI run `25984334503` (at merge SHA `f91b374`) failed because the merge resolution updated `pnpm-lock.yaml` to svelte 5.55.7 but left `package.json` saying `^5.0.0`. `pnpm install --frozen-lockfile` rejected the mismatch. Fixed via `git commit -am` bumping the manifest, `git tag -d v0.10.0 && git tag v0.10.0 HEAD`, force-pushed the tag. New CI run is green on all 4 `cargo test` jobs + 1 of 4 bundle jobs at tick end (the other 3 still building).

**Active dev branch:** `feature/v0.11.0-lathe` — Slices 1+2 shipped, 6 more to go.

**Next tick should:**
1. MODE B: poll CI run `25984687462`. If success: download artifacts, `gh release create v0.10.0`, clear RELEASE_PENDING. If failure: investigate.
2. MODE C: continue v0.11.0 Slice 3 (PagesVisualPanel.svelte — drag-reorder thumbnails) on `feature/v0.11.0-lathe`.

---

## ROADMAP

### v0.8.1 "Polyglot" — RELEASED 2026-05-16
- Tag `v0.8.1`, merge SHA `39ff562`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.8.1)

### v0.9.0 "Toolkit" — RELEASED 2026-05-16
- Tag `v0.9.0`, merge SHA `ba3b291`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.0)

### v0.9.1 "Toolkit UX" — RELEASED 2026-05-16
- Tag `v0.9.1`, merge SHA `7226574`, CI run `25980874364`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.1)

### v0.10.0 "Beacon" — MERGED 2026-05-17, RELEASE_PENDING (retag CI in flight)
- Original tag at merge SHA `f91b374` — CI failed (lockfile/manifest svelte mismatch).
- Hotfix commit `8c757bf` (`fix(release): bump svelte specifier in package.json to ^5.55.7 to match lockfile`), tag `v0.10.0` repointed at it.
- New CI run `25984687462` queued at retag time. 3 of 4 `cargo test` green + macos-x64 bundle green at tick end; mac-arm64/linux-x64/windows-x64 bundles still in progress.
- Release notes: `docs/release-notes/v0.10.0.md`.

### v0.11.0 "Lathe" — IN PROGRESS (Slices 1+2 done)
- Branch: `feature/v0.11.0-lathe` (cut from `main@8c757bf`).
- Plan: `docs/plans/2026-05-17-v0.11.0-lathe-edit-mode.md` (8 slices).
- **Slice 1 DONE 2026-05-17 00:36 PDT** (commit `e95c0ef`): `pdf::duplicate::duplicate_pages` kernel + `slab_duplicate_pages` Tauri command + 7 tests (single-dup, multi-dup, dedup repeated input, out-of-range/zero/empty/missing-input rejection). 186 → 193 lib tests.
- **Slice 2 DONE 2026-05-17 00:50 PDT** (commit `d0635a7`): `pdf::split_pattern` chapter splitter. `find_matching_pages` (regex preview), `outline_top_level_pages` (TOC-based fallback), `split_by_pattern` (orchestrator that always packs preface pages into chunk #1), `ranges_from_chapter_starts` helper. 3 Tauri commands: `slab_split_by_pattern`, `slab_find_matching_pages`. 13 new tests (range building incl. mid-doc start/page-1 start/single/dedup/oor/empty/zero-total, regex validation, end-to-end split with regex, no-match error, missing input). 193 → 206 lib tests.
- **Remaining slices:** 3 = PagesVisualPanel.svelte (drag-reorder thumbnails), 4 = SplitPatternPanel.svelte, 5 = Multi-PDF tabs, 6 = in-place text editing kernel, 7 = TextEdit overlay in ReaderPanel, 8 = release prep.

### v0.12.0 "Atlas" — Library Mode (PLANNED)
Cross-doc Beacon chat across indexed library, tags, collections, watch folders.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.12.0. 7 slices.

### v0.13.0 "Lens" — OCR + Vision (PLANNED)
Local OCR (surya/tesseract), table → CSV, math → LaTeX, vision Q&A in Beacon, auto-tag.
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

### Retag if CI fails on first try
```bash
# Make the fix commit on main.
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit -am "fix(release): ..."
# Delete & re-create the local tag at HEAD.
git tag -d vX.Y.Z
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' tag -a vX.Y.Z -m "..." HEAD
# Push main, delete remote tag, push new tag.
GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin main
GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin :refs/tags/vX.Y.Z
GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin vX.Y.Z
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
- **Merge-conflict lockstep**: if you resolve a `pnpm-lock.yaml` conflict
  by re-running `pnpm update <pkg>@latest`, the merge commit MUST also
  update `package.json` to match — otherwise CI's `pnpm install
  --frozen-lockfile` will reject the mismatch. Bit Cake on v0.10.0; fix
  required a hotfix commit + retag.
- `markitdown` runtime: `/Users/sanjay/.local/bin/markitdown` (pipx).
  Add `$HOME/.local/bin` to PATH for cron-spawned terminals.
- `CmdResult<T>` field on `"ok"` variant is `value`, NOT `data`.
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁 ✦ ⌕ 🔒
- `gh release create` with 6 assets including the 76MB AppImage often
  times out at 60s in foreground. Run it in `background=true` or upload
  the AppImage with a follow-up `gh release upload` and then
  `gh release edit --draft=false --latest`.
- `OutlineNode` has fields `title: String`, `page_index: Option<u32>`
  (0-based!), `children: Vec<OutlineNode>` — NOT `page` like I almost
  wrote in v0.11.0 Slice 2.
- `make_n_page_pdf` test fixture stamps "Slab page {n}" (not "Page {n}")
  on each page — useful for regex-match unit tests on extract_text.

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
- **2026-05-16 21:25 (Cake/cron): 🚀 RELEASE + KICKOFF TICK**: (1) v0.9.1 "Toolkit UX" published on GitHub Releases with all 6 installers, latest release. (2) v0.10.0 **Beacon Slice 1** shipped on `feature/v0.10.0-beacon`: `AiProvider` trait + `OllamaProvider` impl (chat + embeddings) + 5 mockito unit tests (no real Ollama in CI). New deps reqwest/async-trait/futures-util/bytes (runtime), mockito (dev). 101→106 lib tests. All quality gates green. Commit `154c008`. Also added `assets/` to `.gitignore` (cron-staged release binaries live on GitHub Releases, not in git).
- 2026-05-16 21:46 (Cake/cron): Beacon Slice 2 (provider abstraction) shipped in 4 commits: plan promoted to `docs/plans/`, `OpenAiCompatibleProvider` + 6 mockito tests, `BeaconConfig` TOML + `make_provider` + 7 tests, 4 Tauri commands wired. 106→121 lib tests. New runtime dep: `toml` 0.8.
- **2026-05-16 22:45 (Cake/cron): 🚀 TRIPLE-SLICE TICK — Beacon Slices 3+4+5 in one tick.** (a) Slice 3 backend `ai/chat.rs` — page-aware context builder + citation extractor + `beacon_chat()` + 7 tests via in-memory MockProvider + Tauri `slab_beacon_chat` command (commit `cc5a6ea`). (b) Slice 4 frontend `BeaconChatPanel.svelte` — conversation view, citation chips that dispatch goto-page events, sample-prompt grid, friendly-error mapping, Enter-to-send composer; nav entry "✦ Beacon AI" between Reader and Merge (commit `9abc425`). (c) Slice 5 summary — `ai/summary.rs` with Tldr/Short/Long enum + low-temp prompts + 5 tests; Tauri `slab_beacon_summary`; 3 quick-action chips in chat panel that push results as assistant turns (commit `21960ce`). Total: 121→133 lib tests (+12), 1 new module + 1 new component + 2 new Tauri commands. All quality gates green (fmt, clippy `-D warnings`, lib tests, svelte-check). Pushed to `feature/v0.10.0-beacon`. v0.10.0 is now 50% shipped (5 of 10 slices done).
- **2026-05-16 23:50 (Cake/cron): 🚀 Beacon Slice 9 — Selection Actions vertical slice in 2 commits.** Backend `ai::selection_action` module + Tauri command + 13 tests (commit `14b6e7d`). Frontend `BeaconSelectionBubble.svelte` (~620 LOC) + ReaderPanel hook (commit `4825c93`). 173→186 lib tests. All quality gates green. v0.10.0 is now 90% shipped (9 of 10 slices).
- **2026-05-17 00:09 (Cake/cron): 🚀 v0.10.0 "Beacon" MERGED TO MAIN.** MODE A tick: re-ran 4 quality gates on `feature/v0.10.0-beacon` (fmt, clippy `-D warnings`, 186 lib tests, svelte-check 0 errors). Pulled main with dependabot svelte security bump; resolved `pnpm-lock.yaml` conflict by taking feature-branch lockfile then `pnpm update svelte@latest`. Merge SHA `f91b374`. Tagged `v0.10.0`. Pushed via gh-token helper. CI run `25984334503` queued.
- **2026-05-17 00:50 (Cake/cron): 🚀 RESCUE + LAUNCH TICK** — (a) v0.10.0 CI `25984334503` had failed with `ERR_PNPM_OUTDATED_LOCKFILE` (merge resolution updated `pnpm-lock.yaml` to svelte 5.55.7 but left `package.json` saying `^5.0.0`). Verified locally with `pnpm install --frozen-lockfile` after the manifest bump → green. Committed `fix(release): bump svelte specifier in package.json to ^5.55.7 to match lockfile` (`8c757bf`), deleted local + remote `v0.10.0` tag, retagged at `8c757bf`, force-pushed. New CI run `25984687462` started; 3/3 `cargo test` jobs green at tick end, 3/4 bundles still building. (b) Started v0.11.0 "Lathe" on `feature/v0.11.0-lathe` cut from `main@8c757bf`. Wrote 8-slice implementation plan to `docs/plans/2026-05-17-v0.11.0-lathe-edit-mode.md`. **Slice 1 done** (commit `e95c0ef`): `pdf::duplicate::duplicate_pages` kernel + `slab_duplicate_pages` Tauri command + 7 tests. **Slice 2 done** (commit `d0635a7`): `pdf::split_pattern` chapter splitter — regex-based + outline-fallback, `find_matching_pages`/`outline_top_level_pages`/`split_by_pattern`/`ranges_from_chapter_starts`, 2 Tauri commands (`slab_split_by_pattern`, `slab_find_matching_pages`), 13 new tests. 186 → 193 → 206 lib tests across the two slices. All quality gates green (fmt, clippy `-D warnings`, lib tests, svelte-check 0 errors). Branch pushed. 6 slices remaining for v0.11.0.
