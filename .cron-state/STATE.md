# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🚀 v1.2.0 "Glass II" 🪟² MERGED + TAGGED + PUSHED — CI running

**Main HEAD**: `ea2939d` — `Merge v1.2.0 'Glass II' 🪟² — Vim + a11y + i18n foundation`
**Tag**: `v1.2.0` (pushed)
**CI**: run `26006878376` (in_progress, started 00:15 UTC, branch=main, sha=`ea2939d`)

**Quality gates green on `main` HEAD before tag push:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (468 passed)
- `pnpm exec svelte-check` ✓ (0 errors / 23 warnings)
- `pnpm a11y:audit:strict` ✓ (0 issues)

**RELEASE_PENDING**: v1.2.0 — merge SHA `ea2939d`, tag `v1.2.0`, CI run `26006878376`

**Next tick**: MODE B — poll CI run `26006878376`. If success:
- `gh run download 26006878376 -R Sanjays2402/slab -D /tmp/slab-v1.2.0-release/`
- Curate 6 assets into `assets/v1.2.0/` (mac arm64 dmg, mac x64 dmg renamed `_x64_macos.dmg`, linux deb + AppImage, windows msi + setup.exe).
- `gh release create v1.2.0 --title 'v1.2.0 — Glass II 🪟²' --notes-file docs/releases/v1.2.0.md assets/v1.2.0/*` (AppImage upload as follow-up `gh release upload` to dodge 60s timeout).
- Clear RELEASE_PENDING.

---

## TICK 2026-05-17 ~17:10 PT — v1.2.0 Glass II Slices 5-RTL + 7 + MERGE + TAG + PUSH (3 commits + merge)

Sanjay's "ship BIG things every tick" + the existing Slice 5 work already
shipped last tick → this tick's job was: finish the i18n RTL CSS scaffold,
do the version bump (Slice 7), MERGE to main, tag, push. All done.

### Sub-task A — Slice 5 wrap-up: RTL scaffold (`aa629a6`)

Last tick's i18n work shipped en/es/fr/ar bundles and the `applyLocaleToHtml`
helper that flips `<html dir="rtl">` for Arabic, but the global stylesheet
had no `:dir(rtl)` rules, so the UI didn't mirror. Added a ~60-line block
in `src/app.css`:

- Sidebar moves to right edge via `border-left` swap.
- `.nav-icon` + `.detach-btn` flip via `scaleX(-1)` so directional glyphs
  (← → ⤢ ↺ ▷) render the right way.
- `code`, `pre`, `.pdf-viewer`, `.findbar input`, `.output-list li` forced
  back to LTR because source code + PDF canvases should always be LTR.
- Used `:dir(rtl)` pseudo-class, NOT `[dir="rtl"]` attribute selector,
  so nested subtrees can opt back into LTR without explicit `dir=` overrides.

### Sub-task B — Slice 7: version bump 1.1.0 → 1.2.0 + release notes (`2b09a46`)

Lockstep version bump across:
- `package.json` (1.1.0 → 1.2.0)
- `src-tauri/Cargo.toml` (1.1.0 → 1.2.0)
- `src-tauri/Cargo.lock` (workspace member entry refreshed via `cargo update --workspace --offline`)
- `src-tauri/tauri.conf.json` (1.1.0 → 1.2.0)
- `src/routes/+page.svelte` sidebar pill (`v1.1.0` → `v1.2.0`)

Plus `docs/releases/v1.2.0.md` — ~7.5 KB release notes structured as
"three pillars" (Vim, a11y, i18n), with tables for keybindings, locales,
and audit baseline numbers. Quality bar section, upgrade notes, renewed
stability promise.

### Sub-task C — MODE A merge to main + tag + push

- `git checkout main && git pull` — already up to date.
- `git merge --no-ff feature/v1.2.0-glass-ii -m "Merge v1.2.0 'Glass II' 🪟² — Vim + a11y + i18n foundation"`.
- 4 quality gates ran on `ea2939d` (main HEAD post-merge): all green.
- `git tag v1.2.0`.
- `git push origin main --follow-tags` (main pushed, tag pushed via second
  follow-up since first `--follow-tags` only pushed branch).
- `git push origin feature/v1.2.0-glass-ii` to keep the dev branch in sync
  with the two new commits.

### CI

Run `26006878376` started immediately on push at 00:15 UTC. Still
in_progress at end of tick. Will be finalized next tick (MODE B).

### Commits this tick

- `aa629a6` — feat(i18n): RTL scaffold in app.css for Arabic locale (Slice 5 wrap-up)
- `2b09a46` — chore(release): v1.2.0 "Glass II" — version bump + release notes (Slice 7)
- `ea2939d` — Merge v1.2.0 'Glass II' 🪟² — Vim + a11y + i18n foundation (merge commit)
- Tag `v1.2.0` pushed.

### Glass II completion ledger — ALL SLICES SHIPPED ✓

- ✅ Slice 1 — pure Vim state machine + keymap
- ✅ Slice 1.5 — VimController + VimIndicator + Settings/Palette toggles
- ✅ Slice 2 — Reader Vim adapter + panel wiring
- ✅ Slice 3 first half — Library Vim adapter
- ✅ Slice 3 second half — Beacon Vim adapter
- ✅ Slice 4 — a11y audit script + focus-visible ring + aria-current
- ✅ Slice 4 pass 2 — input-label fixes across 15 panels
- ✅ Slice 5 — i18n module + en/es/fr/ar bundles + Settings/Palette pickers
- ✅ Slice 5 wrap-up — RTL CSS scaffold (this tick)
- ✅ Slice 6 — prefers-reduced-motion + prefers-contrast
- ✅ Slice 7 — version bump + release notes + tag + push (this tick)

### Velocity

| Tick | Slices shipped                          | Commits | Cumulative |
|------|----------------------------------------|---------|------------|
| 1    | Slice 1 + 1.5                          | 2       | 2          |
| 2    | Slice 2 + 3 first half                 | 3       | 5          |
| 3    | Slice 3-2nd + 4 (×2) + 6               | 4       | 9          |
| 4    | Slice 5 (i18n core+ar+es+fr)           | 5       | 14         |
| 5    | Slice 5-RTL + 7 + merge + tag (TODAY)  | 3 + merge | 17 + merge |

**5 ticks total for v1.2.0 Glass II** (~10 hours of clock time). Bigger than
Cabinet (3 ticks) but Glass II shipped three foundational layers — Vim,
a11y, i18n — that each justify a full release.

---

## PRIOR TICK STATE (kept for reference)

## STATUS-PRIOR: 🚀 v1.2.0 "Glass II" Slices 3-2nd-half + 4 + 6 shipped (5 commits this tick)

**Main HEAD**: `ef037e3` — `chore(cron): v1.1.0 Cabinet merged + tagged, RELEASE_PENDING set`
**v1.1.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.1.0 — all 6 assets uploaded ✓
**Active branch**: `feature/v1.2.0-glass-ii` (10 commits ahead of main as of last tick)
**Branch HEAD**: `e4478db` — `a11y: honour prefers-reduced-motion and prefers-contrast (Slice 6)`

**Quality gates green on branch HEAD:**
- pnpm exec svelte-check → 0 errors / 23 warnings (down from 28)
- cargo fmt --check ✓
- cargo clippy --all-targets -D warnings ✓
- cargo test --lib → 468 passed

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
### v1.1.0 "Cabinet" — RELEASED 2026-05-17 🗄
### v1.2.0 "Glass II" — MERGED + TAGGED 2026-05-17 🪟² — CI running, release pending

### v1.3.0 "Foundry" (next)
- Plugin API, community-extensible
- Built on the a11y + i18n + Vim foundations from Glass II
- See `.cron-state/proposals/` for spec (to be drafted)

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```
