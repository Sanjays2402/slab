# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: MODE C — `feature/v0.13.0-lens` Slice 1 DONE on branch (4 commits pushed). Next tick: keep developing Lens.

**Active branch:** `feature/v0.13.0-lens`
**Last shipped slice:** v0.13.0 Slice 1 — Scan Audit backend + Auto-OCR Banner (4 commits, branch pushed)

**v0.13.0 Lens slices (1 / 9 done):**
- ✅ **Slice 1: Scan-Audit backend + Auto-OCR Banner.**
  - `pdf::scan_audit` module — `PageClassification {Text,Image,Mixed,Empty}` + `Recommendation {OcrAll,OcrSome,None}` + `ScanAuditReport` (per-page vec + tallies + recommendation). 10 unit tests, no rasterisation, walks PDF object graph only.
  - New test fixtures: `make_image_only_pdf`, `make_mixed_pdf`.
  - 1 new Tauri command `slab_scan_audit(input) -> CmdResult<ScanAuditReport>` + TS bindings at `src/lib/lens.ts`.
  - ReaderPanel: auto-audit on every PDF open → non-modal banner [OCR now] / [Dismiss] when doc looks scanned (Linear-style slate-blue card, top-center, 220ms slide-in).
  - Commits: `885390e` (plan + release-notes mirror), `d4905e2` (backend), `b53e0e5` (IPC + TS), `dfb2061` (Reader UI).
- ⏳ Slice 2: Library auto-OCR queue (`is_scanned` column, background worker)
- ⏳ Slice 3: Table extraction → CSV (`pdftotext -layout` + column aligner)
- ⏳ Slice 4: Equation extraction → LaTeX (pix2tex sidecar)
- ⏳ Slice 5: Vision Q&A in Beacon (llava via Ollama)
- ⏳ Slice 6: Auto-tag on Library import
- ⏳ Slice 7: Vision auto-OCR for Mixed pages (invisible-text overlay)
- ⏳ Slice 8: `slab lens audit / ocr / tables` CLI
- ⏳ Slice 9: Release prep (bump 0.12.0 → 0.13.0 + notes)

**Quality gates green on `feature/v0.13.0-lens`:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓ (clean)
- `cargo test --lib` — 283 passed (273 → 283, +10 in `pdf::scan_audit`)
- `pnpm exec svelte-check` — 0 errors / 28 (pre-existing) warnings

**Atlas slices (2 / 5 done — for reference, v0.12.0 RELEASED):**
- ✅ Slice 1: Library backend foundation (registry + scanner + query + 8 IPC + TS bindings)
- ✅ Slice 3: LibraryPanel UI + sidebar nav + Reader handoff
- ⏳ Slice 2: `library::watch` daemon (deferred to v0.12.1)
- ⏳ Slice 4: Cross-doc Beacon chat (deferred)
- ⏳ Slice 5: Saved searches (deferred)

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

### v0.11.0 "Lathe" — RELEASED 2026-05-17
- Tag `v0.11.0`, merge SHA `76cd7ed`, CI run `25987817724`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.11.0)

### v0.12.0 "Atlas" — RELEASED 2026-05-17
- Tag `v0.12.0`, merge SHA `1ca71a2`, CI run `25989140987`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.12.0)
- Headline: PDF Library (browse, tag, search local PDFs).

### v0.13.0 "Lens" — OCR + Vision (IN PROGRESS — Slice 1 / 9)
Spec: `.cron-state/proposals/v0.13.0-lens.md`. 9 slices.

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
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁 ✦ ⌕ 🔒 ✂ ≣ ✎ ❐
- `gh release create` with 6 assets including the 76MB AppImage often
  times out at 60s in foreground. Run it in `background=true` or upload
  the AppImage with a follow-up `gh release upload` and then
  `gh release edit --draft=false --latest`.

### Release asset naming
- Mac x64 dmg needs `_x64_macos.dmg` rename (disambiguate from Windows x64).
- Standard set: 1 dmg per mac arch + 1 deb + 1 AppImage (linux) + 1 msi + 1 setup.exe (windows).

---

## NOTES FROM PRIOR SESSIONS
- 2026-05-17 03:32 (Cake/cron): 🚀 ATLAS SLICE 1 SHIPPED — Library Backend Foundation (registry+scanner+query+8 IPC+TS).
- 2026-05-17 (Cake/cron): 🚀 ATLAS SLICE 3 SHIPPED — LibraryPanel UI + sidebar nav + Reader handoff. v0.12.0 merged to main (1ca71a2), tag pushed, CI 25989140987 in progress.
- **2026-05-17 04:30 (Cake/cron): 🚀🚀🚀 v0.12.0 ATLAS RELEASED + v0.13.0 LENS SLICE 1 SHIPPED IN ONE TICK.**
  - MODE B: CI `25989140987` green → downloaded 6 artifacts → `gh release create v0.12.0 --latest` with all 6 installers (aarch64.dmg, x64_macos.dmg, amd64.deb, amd64.AppImage, x64_en-US.msi, x64-setup.exe). Release live at https://github.com/Sanjays2402/slab/releases/tag/v0.12.0
  - MODE C: started v0.13.0 Lens. Wrote 9-slice plan. Branched `feature/v0.13.0-lens`. Shipped Slice 1 (scan-audit backend + auto-OCR banner) in 4 commits:
    - `885390e` docs(lens): plan + v0.12.0 release-notes mirror
    - `d4905e2` feat(lens): scan_audit backend — 10 unit tests, 0 dep cost, walks PDF object graph
    - `b53e0e5` feat(lens): slab_scan_audit Tauri command + lens.ts TS bindings
    - `dfb2061` feat(lens): Reader auto-OCR banner — non-modal, top-center, [OCR now]/[Dismiss], slate-blue Linear-style
  - All 4 quality gates green on the branch: fmt, clippy `-D warnings`, 283 lib tests (+10), svelte-check 0 errors.
  - Branch pushed to origin. Next tick: continue v0.13.0 — Slice 2 (Library auto-OCR queue) or Slice 3 (Table extraction → CSV).
