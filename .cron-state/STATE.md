# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.5.0 "Smart Outline" MERGED + TAGGED — CI in_progress, MODE B finalize next tick

**Main HEAD**: `f4f07b7` — `chore(release): bump to v1.5.0 "Smart Outline" ✦`
**Last MERGE commit**: `f4c74af` (merge of feature/v1.5.0-beacon-bonus-11-smart-outline → main, this tick)
**Tag pushed**: `v1.5.0` (lightweight)
**CI run for MODE B**: `26020447055` (started ~07:47 UTC, ~12min expected)
**RELEASE_PENDING**: v1.5.0 — wait for CI green, then `gh release create v1.5.0 --notes-file docs/release-notes/v1.5.0.md` with curated 6 assets

---

## TICK 2026-05-18 00:36 PT — Beacon Slice 11 Smart Outline SHIPPED end-to-end ✦

Executed the entire 8-task plan in `docs/plans/2026-05-18-beacon-slice-11-smart-outline.md`
in one tick, then promoted it through MODE A merge. Total: **8 commits**,
**698 lines added**, **12 new unit tests**, **0 quality-gate regressions**.

**Commits on feature/v1.5.0-beacon-bonus-11-smart-outline:**
1. `3e948d7` feat(beacon): scaffold ai::outline module with ProposedOutline types
2. `ff6110d` feat(beacon/outline): add liberal LLM JSON parser
3. `5184348` feat(beacon/outline): validate, dedupe, and tree-ify LLM proposals
4. `854c94e` feat(beacon/outline): add propose_outline async entry point + path wrapper
5. `6fbc5c2` feat(beacon/outline): expose slab_beacon_propose_outline Tauri command
6. `17bef60` feat(beacon/outline-ui): Smart Outline button + proposal review in editor

**MODE A merge on main:**
- merge `--no-ff` of feature/v1.5.0-beacon-bonus-11-smart-outline (4 files +698)
- `f4f07b7` chore(release): bump to v1.5.0 "Smart Outline" ✦ (package/Cargo/tauri.conf + Cargo.lock + release notes)
- tag `v1.5.0` (lightweight) pushed to origin
- main pushed with `--follow-tags`, tag pushed separately because it's lightweight

**Quality gates ALL green on the merged main:**
- `cargo fmt --all -- --check` → exit 0
- `cargo clippy --all-targets -- -D warnings` → no error: lines
- `cargo test --lib` → **593 passed** (581 baseline + 12 new outline tests)
- `pnpm check` → **0 errors, 23 warnings** (baseline preserved)
- `pnpm build` → built in 4.21s

**The feature itself** — what shipped to users:
- `✦ Suggest (Beacon)` button in the Outline Editor header
- One click → Beacon proposes hierarchical H1/H2/H3 TOC from PDF body text
- Page-number validation (drops out-of-range), dedupe (near-duplicate titles within ±1 page), level clamping (1..=3), max 80 nodes
- Proposal panel: model name, pages used, dropped count, accept/reject
- Accept replaces working tree; user still clicks Save to persist (two-step)
- Reuses `slab_write_outline` save path — NO new save plumbing (YAGNI)

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

---

## NEXT TICK PLAYBOOK — MODE B finalize v1.5.0

1. Poll CI: `gh run view 26020447055` — if in_progress, skip to MODE C (start writing Slice 12 plan).
2. If CI green:
   - `mkdir -p /tmp/slab-release-v1.5.0 && gh run download 26020447055 --dir /tmp/slab-release-v1.5.0`
   - **DISK CHECK FIRST**: `df -h /tmp` — clean up `/tmp/slab-*-release/` from prior versions if free < 3G.
   - Curate 6 best assets:
     - macos-arm64 `.dmg`
     - macos-x64 `.dmg`
     - linux-x64 `.deb`
     - linux-x64 `.AppImage`
     - windows-x64 `.msi`
     - windows-x64 setup `.exe` (nsis)
   - `gh release create v1.5.0 --title 'v1.5.0 — Smart Outline ✦' --notes-file docs/release-notes/v1.5.0.md <6 asset paths>`
   - Remove `RELEASE_PENDING` line from STATE.md.
3. If CI failed:
   - `RELEASE_FAILED: v1.5.0 — run 26020447055, job <name>`
   - Decide: fix on `feature/v1.5.1-hotfix` or revert merge.

**Note on the workflow**: `build.yml` triggers on push to main +
PRs only. **It does NOT auto-create GH releases on tag push.** MODE B must run
`gh release create` explicitly.

---

## ROADMAP

### v0.8.1 → v1.4.0 — RELEASED (see git history)
### v1.5.0 "Smart Outline" ✦ — **MERGED + TAGGED, CI in_progress for MODE B finalize**

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## POST-v1.5 ROADMAP REMINDERS

Next candidates (recommend Beacon Bonus Slices 12-15 in order — quick AI
wins riding existing infra):

**Slice 12 — Citations**
- Beacon scans PDFs for `(Author 2024)` style citations and links them to
  a built References / Bibliography table. Re-uses chunker + provider.

**Slice 13 — Study Mode**
- Generate flashcards (Q&A pairs) + auto-quiz from a doc section.
  Persists to `~/.slab/study.db`. UI: panel similar to Beacon Chat.

**Slice 14 — Glossary**
- LLM extracts domain-specific terms and definitions from the doc, builds
  a sidebar glossary, links inline mentions on hover.

**Slice 15 — Voice Mode**
- TTS playback of Beacon answers + STT for asking questions. Provider-
  agnostic — local Whisper for STT, system TTS for output. v1 ships
  buttons-only, no wake-word.

After Bonus Slices land, **v1.6.0 candidates**:

**Option A — v1.6.0 "TypeScript Plugins"**
- `script.js` contribution kind running in an embedded V8/QuickJS
  sandbox. Lets plugins do real frontend work (custom panels).
  Risk: sandbox security is hard.

**Option B — v1.6.0 "Forge" (author-controlled signing)**
- Lets plugin authors sign their own releases with their own keys instead
  of routing through the maintainer. Want at least 10 plugins in the
  curated index before considering this.

Other parked items:
- AI provider hook-up of plugin-contributed providers through Beacon's
  runtime (planned v1.3.x patch — currently they appear in the
  palette + boot log but aren't yet selectable in chat)
- Slab CLI `slab plugin install <url>` command (post-Bench)
- The leftover `docs/screenshots-v1.3.1/` directory in repo root is
  Sanjay's intermediate working copy; harmless, can be `rm -rf`'d.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins`
  GH repo, drop seed files from `docs/marketplace-seed/`, sign the
  hello-slab plugin and post the first real `index.json`.
