# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.7.0 "Study Mode" 🎓 merged + tagged + pushed — RELEASE_PENDING (CI building)

**Main HEAD**: `521f8ec` (chore(release): v1.7.0 "Study Mode" 🎓)
**Merge SHA**: `da53f84` (Merge v1.7.0 'Study Mode' 🎓 — Q&A flashcards with SM-2 spaced repetition)
**Tag**: `v1.7.0` → pushed to origin
**CI run**: `26034343468` (in_progress at tick end, all-platforms matrix build)
**Latest release**: v1.6.0 "Citations" 📑 — https://github.com/Sanjays2402/slab/releases/tag/v1.6.0

**RELEASE_PENDING: v1.7.0 — merge SHA da53f84, release commit 521f8ec, tag v1.7.0, CI run 26034343468**

---

## TICK 2026-05-18 05:42 PT — MODE A complete (merge + tag + push)

Picked up Slice 13 "Study Mode" 🎓 sitting DONE on
`feature/v1.7.0-beacon-bonus-13-study-mode` from the prior tick (5:26 PT)
and executed the full MODE A playbook in one pass:

1. `git checkout main && git pull --ff-only` — main was at 5306b3e, clean.
2. `git merge --no-ff feature/v1.7.0-beacon-bonus-13-study-mode` — clean
   merge via 'ort' strategy, 8 files changed, 1584 insertions. Merge
   commit `da53f84`.
3. Version bump 1.6.0 → 1.7.0 across `package.json`,
   `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and
   `src-tauri/Cargo.lock` (regenerated via `cargo update -p slab-app
   --offline`).
4. Wrote `docs/release-notes/v1.7.0.md` — modelled on the v1.6.0
   template, covers Study Mode sidebar entry, SM-2-lite scheduler,
   sqlite store, generation pipeline, 4 Tauri commands, and the
   Svelte panel.
5. Commit `521f8ec` — `chore(release): v1.7.0 "Study Mode" 🎓`.
6. Quality gates on main (all four green, batched once):
   - `cargo fmt --all -- --check` — clean
   - `cargo clippy --all-targets -- -D warnings` — clean
   - `cargo test --lib` — **634 passed; 0 failed**
   - `pnpm check` — 0 errors, 23 pre-existing warnings
7. `git tag -a v1.7.0` then pushed: `git push origin main --follow-tags`
   via gh-token credential helper. Both main + tag landed on origin.
8. CI run `26034343468` triggered automatically by the push; in
   progress at tick end (all three platforms — macos-arm64, linux-x64,
   windows-x64 — in the matrix).

Next tick: **MODE B finalize** — poll the CI run, download all 6
curated artifacts when green, and `gh release create v1.7.0` with the
release notes. If CI red, diagnose and either fix on a follow-up branch
or revert.

---

## NEXT TICK PLAYBOOK — MODE B finalize v1.7.0

1. `cd /Users/sanjay/Projects/slab && gh run view 26034343468`
2. If still in_progress → skip to MODE C (likely Slice 14 "Glossary" prep) and
   re-check next tick.
3. If completed success:
   ```bash
   gh run download 26034343468 --dir /tmp/slab-release-v1.7.0
   ls /tmp/slab-release-v1.7.0/
   ```
   Curate the 6 best artifacts: macos-arm64 dmg, macos-x64 dmg, linux
   x64 deb + AppImage, windows msi + nsis. (Match the platform table in
   `docs/release-notes/v1.7.0.md`.)
4. ```bash
   gh release create v1.7.0 \
     --title 'v1.7.0 — Study Mode 🎓' \
     --notes-file docs/release-notes/v1.7.0.md \
     /tmp/slab-release-v1.7.0/<artifact1> /tmp/slab-release-v1.7.0/<artifact2> ...
   ```
5. Remove the `RELEASE_PENDING` line from STATE.md.
6. If CI failed → write `RELEASE_FAILED:` to STATE.md with run_id +
   failing job, fix on a follow-up branch, no revert needed (the v1.6.0
   release is fine and main builds locally).
7. Begin authoring **Slice 14 "Glossary"** plan at
   `docs/plans/2026-05-18-beacon-slice-14-glossary.md` in MODE C of the
   same tick.

---

## ROADMAP

### v0.8.1 → v1.5.0 — RELEASED (see git history)
### v1.6.0 "Citations" 📑 — **RELEASED 2026-05-18**
### v1.7.0 "Study Mode" 🎓 — **TAGGED 2026-05-18**, CI building, MODE B next tick
### v1.8.0 "Glossary" 📖 — proposal: LLM-extracted domain terms with hover linking
### v1.9.0 "Voice Mode" 🔊 — proposal: TTS + STT for hands-free Beacon

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## POST-v1.7 ROADMAP REMINDERS

After Study Mode lands, the Bonus track continues:

**Slice 14 — Glossary** (next up after MODE B finalises v1.7.0)
- LLM extracts domain-specific terms and definitions from the doc, builds
  a sidebar glossary, links inline mentions on hover.
- Reuse pattern: `ai::glossary` module mirroring `ai::outline` / `ai::citations`
  (regex scanner for term candidates → LLM definition extraction → liberal
  JSON → validate → link → cache). Sqlite or JSON cache TBD.

**Slice 15 — Voice Mode**
- TTS playback of Beacon answers + STT for asking questions. Provider-
  agnostic — local Whisper for STT, system TTS for output. v1 ships
  buttons-only, no wake-word.

After Bonus Slices land, **v2.0.0 candidates**:

**Option A — v2.0.0 "TypeScript Plugins"**
- `script.js` contribution kind running in an embedded V8/QuickJS
  sandbox. Lets plugins do real frontend work (custom panels).
  Risk: sandbox security is hard.

**Option B — v2.0.0 "Forge" (author-controlled signing)**
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
