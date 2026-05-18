# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🪑 v1.4.0 "Bench" SHIPPED ✓ — Slice 11 plan staged for next dev tick

**Main HEAD**: `b4dfd6d` — `docs(plans): Beacon Slice 11 — Smart Outline implementation plan`
**Latest release**: [v1.4.0](https://github.com/Sanjays2402/slab/releases/tag/v1.4.0) — 6 platform assets attached
**Next mode**: MODE C — execute `docs/plans/2026-05-18-beacon-slice-11-smart-outline.md`

---

## TICK 2026-05-18 00:15 PT — wrote Slice 11 plan + finalized v1.4.0 release

Two things shipped this tick:

1. **MODE B finalize for v1.4.0** — CI run 26018215033 turned green
   mid-tick (all 7 jobs success: cargo-test × 3 platforms + bundle × 4
   platforms). Downloaded artifacts, curated the standard 6 (macos-arm64
   dmg, macos-x64 dmg, linux deb + AppImage, windows msi + nsis), ran
   `gh release create v1.4.0 --notes-file docs/release-notes/v1.4.0.md`
   with all six files. Release URL:
   https://github.com/Sanjays2402/slab/releases/tag/v1.4.0
   RELEASE_PENDING removed.

2. **MODE C planning** — Sanjay's instruction was "ship big features each
   tick", so used the writing-plans skill to author a full TDD-structured
   implementation plan for the next big feature: Beacon Bonus Slice 11
   (Smart Outline — propose hierarchical TOC from PDF content). Saved to
   `docs/plans/2026-05-18-beacon-slice-11-smart-outline.md` (commit
   `b4dfd6d` on main). Eight tasks, ~12 unit tests planned, ~50 minutes
   of focused work — a clean handoff for next tick to execute.

**Disk-space note**: `/tmp` was at 100% capacity when the artifact
download started — had to delete old `/tmp/slab-v*-release/` dirs from
v1.0/1.1/1.2/1.3.1 to free ~1.7G. Cleanup is now part of the MODE B
workflow muscle memory. Final state: 2.4G free on `/tmp`.

---

## TICK 2026-05-17 23:55 PT — v1.4.0 release pipeline executed

**Bigger story than expected.** Sanjay had pushed `8166f40` to main
between ticks — a deliberate README rewrite (generic framing, no
version anchors, no codenames, 42 fresh screenshots) PLUS the same
`__APP_VERSION__` work that was sitting in his WIP stash on the
feature branch. The stash conflict + README direction conflict
forced a recovery dance:

1. Started with feature/v1.4.0-bench at 179b074. Popped Sanjay's
   stash (`vite.config.js`, `src/routes/+page.svelte`, `src/app.d.ts`
   completion of `__APP_VERSION__`) and committed.
2. Did the four-file version bump + wrote `docs/release-notes/v1.4.0.md`
   + updated README v1.4 front-door. Committed.
3. Switched to main → pulled `8166f40` which contains a *contradictory*
   README rewrite + duplicate version-injection plumbing.
4. **Reset feature branch to 179b074** to drop the redundant commits.
5. Merged origin/main into feature/v1.4.0-bench. README conflict
   resolved by taking origin/main wholesale (Sanjay's design intent
   wins — README stays version-agnostic, no per-version sections).
6. Re-did version bump as a single clean commit on top of the merge.
7. Pushed feature branch (force-with-lease, because step 4 rewrote).
8. Merged --no-ff into main, tagged v1.4.0, pushed main --follow-tags.

**Final commits on main this tick:**
- `e9b878f` — Merge remote-tracking branch 'origin/main' into feature/v1.4.0-bench
- `61d95ea` — chore(release): bump to v1.4.0 "Bench" 🪑
- `9060763` — Merge v1.4.0 'Bench' 🪑 — signed plugin marketplace

**Lesson learned**: `git pull` on main BEFORE touching the feature
branch next time. Sanjay edits between ticks; assume main moves.

---

## NEXT TICK PLAYBOOK — MODE B finalize v1.4.0

1. Poll CI: `gh run view 26018215033` — if in_progress, skip to MODE C.
2. If CI green:
   - Download artifacts: `gh run download 26018215033 --dir /tmp/slab-release-v1.4.0`
   - Curate 6 best assets:
     - macos-arm64 `.dmg`
     - macos-x64 `.dmg`
     - linux-x64 `.deb`
     - linux-x64 `.AppImage`
     - windows-x64 `.msi`
     - windows-x64 setup `.exe` (nsis)
   - `gh release create v1.4.0 --title 'v1.4.0 — Bench 🪑' --notes-file docs/release-notes/v1.4.0.md <6 asset paths>`
   - Remove `RELEASE_PENDING` line from STATE.md.
3. If CI failed:
   - `RELEASE_FAILED: v1.4.0 — run 26018215033, job <name>`
   - Decide: fix on `feature/v1.4.0-bench-hotfix` or revert merge.

**Note on the workflow**: `build.yml` triggers on push to main +
PRs only. **It does NOT auto-create GH releases on tag push.** STATE
note from v1.3.1 saying "tauri-action auto-created release" was
wrong — that release was created manually. MODE B must run
`gh release create` explicitly.

---

## ROADMAP

### v0.8.1 → v1.3.1 — RELEASED (see git history)
### v1.4.0 "Bench" 🪑 — **MERGED + TAGGED, awaiting CI for MODE B finalize**

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

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
- The leftover `docs/screenshots-v1.3.1/` directory in repo root is
  Sanjay's intermediate working copy; Sanjay's commit `8166f40`
  already shipped fresh screenshots into `docs/screenshots/`. The
  legacy dir is untracked and harmless; Sanjay can `rm -rf` it.
