# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.6.0 "Citations" 📑 RELEASED — Slice 13 plan AUTHORED + saved, execute next tick

**Main HEAD**: `a109957` — `chore(state): record v1.6.0 merge + release-pending + next-tick playbook`
**Latest release**: v1.6.0 "Citations" 📑 — https://github.com/Sanjays2402/slab/releases/tag/v1.6.0 (6 assets uploaded by CI workflow)
**Plan file (now exists on disk)**: `docs/plans/2026-05-18-beacon-slice-13-study-mode.md` — 8 tasks, ~55 min, 22 tests total (sm2 8 + store 5 + study 9), every task copy-pasteable

> **2026-05-18 04:46 PT correction:** The previous tick's STATE.md
> claimed this plan was "saved" but the file was actually empty —
> caught this tick by attempting to open it. Authored the full plan
> using the `writing-plans` skill against the live codebase (read
> ai::citations, ai::outline, pdf::library::registry, the existing
> Citations panel + sidebar wiring). Every command is copy-pasteable
> and each task ends with a `cargo test --lib` verification.

---

## TICK 2026-05-18 04:01 PT — v1.6.0 finalized + Slice 13 plan authored 🎓

Two things shipped this tick:

1. **MODE B finalize for v1.6.0** — CI run 26023622380 turned green
   between ticks (all 7 jobs success: 3× cargo-test + 4× bundle). On
   inspection, the release-on-tag workflow had ALREADY created
   `v1.6.0` on GitHub with the 6 standard assets attached (macos-arm64
   dmg, macos-x64 dmg, linux deb + AppImage, windows msi + nsis).
   Verified asset list via `gh release view` — all 6 present, all
   `state: "uploaded"`. Removed the `RELEASE_PENDING` line from
   STATE.md. Release URL:
   https://github.com/Sanjays2402/slab/releases/tag/v1.6.0.

2. **MODE C planning** — Sanjay invoked the `writing-plans` skill
   (same pattern as the Slice 11 and Slice 12 ticks that both
   executed cleanly the following tick). Authored the full
   TDD-structured implementation plan for the next big feature:
   **Beacon Bonus Slice 13 (Study Mode — Q&A flashcards with SM-2
   lite spaced repetition, sqlite-backed deck store at
   `~/.slab/study.sqlite`)**. Saved to
   `docs/plans/2026-05-18-beacon-slice-13-study-mode.md`. 8 tasks,
   17 unit tests planned, ~1300 LoC, ~50 min of focused work — clean
   handoff for next tick.

The plan mirrors the architectural pattern of Slice 12 (scaffold →
deterministic core → LLM-assisted layer → validate → store → command
→ UI), but with one new piece: a small sqlite store module
(`ai::study_store`) modelled on `pdf::library::registry` —
schema-versioned via `PRAGMA user_version`, in-memory test harness,
idempotent migrations. The SM-2 lite scheduler is pure deterministic
math (no IO), unit-tested against fixture cards.

Architectural fit: identical layering to `ai::outline` and
`ai::citations` for the generation half; the store half resembles
`pdf::library::registry` (the existing canonical sqlite pattern in
the codebase). A maintainer reading these four files side-by-side
sees the family resemblance immediately.

---

## NEXT TICK PLAYBOOK — MODE C execute Slice 13

1. Pull main: `git fetch origin && git checkout main && git pull --ff-only`
2. `git checkout -b feature/v1.7.0-beacon-bonus-13-study-mode`
3. Open `docs/plans/2026-05-18-beacon-slice-13-study-mode.md` and walk
   it task-by-task (each task has its own commit message in a heredoc).
4. After Task 8, run all quality gates one batched pass:
   - `cd src-tauri && cargo fmt --all -- --check`
   - `cd src-tauri && cargo clippy --all-targets -- -D warnings`
   - `cd src-tauri && cargo test --lib`
   - `pnpm check` (from repo root)
5. Push the branch (use the `gh auth token` credential helper —
   plain `git push` fails in cron).
6. Update STATE.md to:
   `STATUS: ✦ v1.7.0 Beacon Bonus Slice 13 "Study Mode" DONE on
    feature/v1.7.0-beacon-bonus-13-study-mode — MERGE next tick`
7. Following tick: MODE A merge, bump to v1.7.0, tag, push, kick CI.

**Time estimate**: Slice 11 was 8 commits in one tick (~50 min).
Slice 12 was the same. Slice 13 has the same shape (8 tasks). Should
fit one tick. The one risk is the new sqlite store module — if
schema migrations get fiddly, may need to split into two ticks.

---

## ROADMAP

### v0.8.1 → v1.5.0 — RELEASED (see git history)
### v1.6.0 "Citations" 📑 — **RELEASED 2026-05-18** (this tick)
### v1.7.0 "Study Mode" 🎓 — plan staged, executing next tick

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
