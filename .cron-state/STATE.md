# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.5.0 SHIPPED + RELEASE PUBLISHED — Slice 12 Citations plan staged, ready to execute next tick

**Main HEAD**: `e65dba6` — `docs(plans): Beacon Slice 12 Citations — full TDD-structured plan`
**Latest release**: v1.5.0 "Smart Outline" — https://github.com/Sanjays2402/slab/releases/tag/v1.5.0 (6 assets uploaded)
**Next plan**: `docs/plans/2026-05-18-beacon-slice-12-citations.md` (8 tasks, ~50 min, 18 tests)

---

## TICK 2026-05-18 01:06 PT — v1.5.0 finalized + Slice 12 plan authored ✦

Two things shipped this tick:

1. **MODE B finalize for v1.5.0** — CI run 26020447055 turned green
   right at tick start (all 7 jobs success: 3× cargo-test + 4× bundle).
   Downloaded artifacts to `/tmp/slab-release-v1.5.0/`, curated the
   standard 6 (macos-arm64 dmg, macos-x64 dmg, linux deb + AppImage,
   windows msi + nsis), ran `gh release create v1.5.0 --notes-file
   docs/release-notes/v1.5.0.md` with all six. Release URL:
   https://github.com/Sanjays2402/slab/releases/tag/v1.5.0.
   Cleaned up `/tmp/slab-release-v1.5.0/` after.

2. **MODE C planning** — Sanjay invoked the `writing-plans` skill, so
   followed the precedent set last tick (where the Slice 11 plan
   executed cleanly in 8 commits). Authored the full TDD-structured
   implementation plan for the next big feature: **Beacon Bonus Slice
   12 (Citations — find inline cites, extract a structured References
   table, link them)**. Saved to
   `docs/plans/2026-05-18-beacon-slice-12-citations.md` (commit
   `e65dba6` on main). 8 tasks, 18 unit tests planned, ~1100 LoC, ~50
   minutes of focused work — clean handoff for next tick.

The plan mirrors the architectural pattern of Slice 11 (scaffold →
regex → parser → validate → link → entry → command → UI). Architectural
fit: identical layering to `ai::outline`, so a maintainer reading both
modules side-by-side sees the family resemblance immediately.

---

## NEXT TICK PLAYBOOK — MODE C execute Slice 12

1. Pull main: `git fetch origin && git checkout main && git pull --ff-only`
2. `git checkout -b feature/v1.6.0-beacon-bonus-12-citations`
3. Open `docs/plans/2026-05-18-beacon-slice-12-citations.md` and walk
   it task-by-task (each task has its own commit message in a heredoc).
4. After Task 8, push the branch (use the `gh auth token` credential
   helper — plain `git push` fails in cron).
5. Update STATE.md to:
   `STATUS: ✦ v1.6.0 Beacon Bonus Slice 12 "Citations" DONE on
    feature/v1.6.0-beacon-bonus-12-citations — MERGE next tick`
6. Following tick: MODE A merge, bump to v1.6.0, tag, push, kick CI.

**Time estimate**: Slice 11 was 8 commits in one tick. Slice 12 has
similar shape (8 tasks). Should fit one tick if the LLM doesn't hit
unexpected pnpm-check warnings.

---

## ROADMAP

### v0.8.1 → v1.4.0 — RELEASED (see git history)
### v1.5.0 "Smart Outline" ✦ — **RELEASED 2026-05-18** (this tick)
### v1.6.0 "Citations" 📑 — plan staged, executing next tick

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## POST-v1.6 ROADMAP REMINDERS

After Citations lands, the Bonus track continues:

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
