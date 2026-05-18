# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🪑 v1.4.0 "Bench" Slices 9+10 shipped — Slice 11 (release) next

**Main HEAD**: `9f0fd6f` — `Merge v1.3.1 'Foundry Patch'` (released)
**Active dev branch**: `feature/v1.4.0-bench` @ `6bc227a` (Slices 1-10 done)
**Rolled into rollup from**: `feature/v1.4.0-bench-slices-9-10` @ `1ce0d41` (3 commits this tick)

**Quality gates green on feature/v1.4.0-bench:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (581 passed)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

**v1.3.1 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.3.1 — all 6 assets uploaded ✓ (auto-created by tauri-action during CI run 26012655931, no manual `gh release create` needed)

**v1.4.0 status**: 10/11 slices shipped. Backend complete (5 modules,
42 unit tests). UX complete (Browse tab, install/uninstall modals,
update badges, drawer). Docs complete (MARKETPLACE.md + seed). Only
Slice 11 = release pipeline remains.

---

## TICK 2026-05-17 23:10 PT — v1.3.1 finalized + Bench Slices 9+10 (3 commits)

**MODE B closeout — v1.3.1 released:**
- CI 26012655931 finished green between ticks
- All 6 artifacts already uploaded by tauri-action (no manual upload needed)
- Release notes rendered from `docs/release-notes/v1.3.1.md`

**MODE C develop — v1.4.0 Bench Slices 9+10:**

1. `0185360` — feat(marketplace): uninstall confirmation modal (Slice 9)
   - New `UninstallConfirmModal.svelte` (mirrors DecryptModal style)
   - PluginsPanel refactor: `onUninstall` opens modal, `confirmUninstall` runs
     the destructive call; both card + drawer call sites pick this up
     automatically since they already route through `onUninstall`
   - 7 new i18n keys × 4 locales
   - Closes drawer if showing the just-removed plugin

2. `1ce0d41` — docs(marketplace): submission guide + seed slab-plugins repo (Slice 10)
   - `docs/MARKETPLACE.md` — 340-line maintainer/author guide
   - `docs/marketplace-seed/{README.md,index.json.example}` — drop-in
     payload for the future `Sanjays2402/slab-plugins` GH repo
   - `docs/PLUGINS.md` — one bullet pointing to MARKETPLACE.md

3. `6bc227a` — Merge --no-ff on `feature/v1.4.0-bench`

**Sanjay's WIP preserved**: `stash@{0}: On main: WIP from Sanjay: screenshot refresh + dynamic version`. Recover with `git stash pop` on main.

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
### v1.2.0 "Glass II" — RELEASED 2026-05-17 🪟²
### v1.3.0 "Foundry" 🛠 — TAGGED but CI failed, superseded by v1.3.1
### v1.3.1 "Foundry Patch" 🩹 — RELEASED 2026-05-17
### v1.4.0 "Bench" 🪑 — **Slices 1-10 done, Slice 11 (release) next tick**

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — v1.4.0 Bench Slice 11 (release)

This is the close-out tick for Bench. MODE C → MODE A.

1. **Lockstep version bump** on `feature/v1.4.0-bench`:
   - `package.json`: 1.3.1 → 1.4.0
   - `src-tauri/Cargo.toml`: 1.3.1 → 1.4.0 (both top + `[package]`)
   - `src-tauri/tauri.conf.json`: productVersion → 1.4.0
   - `Cargo.lock`: regenerate via `cargo update -p slab-app`

2. **Release notes**: `docs/release-notes/v1.4.0.md`
   - Headline: 🪑 marketplace
   - Sections: Marketplace, Uninstall safety, Schema reference, Docs

3. **README front-door refresh** to v1.4.0 (test count stays 581 unless gates change)

4. **Quality gates** (all four):
   - `cargo fmt --all -- --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test --lib`
   - `pnpm check`

5. **Mark STATUS: DONE** on the feature branch.

6. **MODE A in same tick**:
   - `git checkout main && git pull`
   - **Stash Sanjay's WIP if dirty** (`git stash list` to check)
   - `git merge --no-ff feature/v1.4.0-bench -m "Merge v1.4.0 'Bench' — plugin marketplace + signed manifests"`
   - Re-run gates on the merge commit
   - `git tag v1.4.0 && git push origin main --follow-tags` (with auth helper)
   - Record `RELEASE_PENDING: v1.4.0 — merge SHA <…>, tag v1.4.0, CI run <…>` here

7. **MODE B finalize** next tick after CI completes.

---

## POST-v1.4 ROADMAP REMINDERS

After v1.4.0 ships, candidate next versions:

**Option A — v1.4.1 "Bench seed" (one-time external)**
- Sanjay creates `Sanjays2402/slab-plugins` GH repo, drops the seed
  files from `docs/marketplace-seed/` into it. Maintainer-side: sign
  the hello-slab plugin and post the first real `index.json`. This
  is a Sanjay action, not a Cake one.

**Option B — Beacon Bonus Slices (`.cron-state/proposals/v0.10.0-beacon-bonus-slices.md`)**
- Smart Outline, Citations, Study Mode, Glossary, Voice Mode — five
  AI features riding the existing Beacon infra. Quick wins.

**Option C — v1.5.0 "TypeScript Plugins"** — a `script.js`
contribution kind running in an embedded V8/QuickJS sandbox. Bigger
project; would let plugins do real frontend work (custom panels).
Risk: sandbox security is hard.

**Option D — v1.5.0 "Forge" (author-controlled signing)**
- Lets plugin authors sign their own releases with their own keys
  instead of routing through the maintainer. Bigger trust model
  shift; want at least 10 plugins in the curated index before
  considering this.

My recommendation: **Option B (Beacon Bonus Slices)** next. Quick
wins riding the existing Beacon infra — Smart Outline + Citations
are 2-3 ticks each and visibly bump the AI experience.

Other parked items:
- AI provider hook-up of plugin-contributed providers through Beacon's
  runtime (planned v1.3.x patch — currently they appear in the
  palette + boot log but aren't yet selectable in chat)
- Slab CLI `slab plugin install <url>` command (post-Bench)
