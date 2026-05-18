# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.7.0 Beacon Bonus Slice 13 "Study Mode" 🎓 DONE on feature/v1.7.0-beacon-bonus-13-study-mode — MERGE next tick

**Main HEAD**: `5306b3e` (unchanged)
**Feature branch HEAD**: `fa32144` — `fix(beacon/study): use std::io::Error::other (clippy 1.95)`
**Plan executed**: `docs/plans/2026-05-18-beacon-slice-13-study-mode.md` (8 tasks, all green)
**Latest release**: v1.6.0 "Citations" 📑 — https://github.com/Sanjays2402/slab/releases/tag/v1.6.0

---

## TICK 2026-05-18 05:26 PT — Slice 13 "Study Mode" 🎓 shipped end-to-end

Beacon Bonus Slice 13 landed in one tick: 9 commits (8 from the plan + 1
clippy fix), the full vertical slice (ai::study + ai::study_store +
ai::sm2 + 4 Tauri commands + BeaconStudyPanel.svelte + sidebar nav).
Each layer TDD'd against existing patterns:

- **ai::sm2** — pure scheduler (8 tests covering ease ladder + EF
  floor/cap). `Ease::{Again,Hard,Good,Easy}` enum with snake_case serde.
- **ai::study_store** — sqlite at ~/.slab/study.sqlite, schema-versioned
  via `PRAGMA user_version`, modelled on `pdf::library::registry`
  (6 tests, in-memory harness via `:memory:` connection).
- **ai::study** — generate_deck pipeline (extract → chunk → LLM Q&A →
  parse → validate → insert; 9 tests for parser + validator + dedupe
  + caps + budget).
- **4 Tauri commands**: `slab_beacon_generate_deck`, `slab_beacon_study_due`,
  `slab_beacon_study_review`, `slab_beacon_study_stats` — all hashed
  per-PDF via reused `EmbeddingIndex::hash_file`.
- **BeaconStudyPanel.svelte** — Svelte 5 ($state runes), pick PDF +
  generate + review-one-at-a-time UI with 4-button ease scale,
  reveal-then-rate flow, footer stats, jump-to-page event dispatch.
- **Sidebar nav + detach support** — both main and detached-window
  branches of `+page.svelte`, added to `DETACHABLE_PANELS`.

Quality gates: all four green (fmt clean, clippy -D warnings clean
after fixing one `io_other_error` lint, `cargo test --lib` 634/634,
`pnpm check` 0 errors / 23 pre-existing warnings).

**Commits** (oldest first):
- `0fb046b` feat(beacon/study): scaffold ai::study module + Flashcard type
- `5c1fee8` feat(beacon/sm2): SM-2-lite spaced-repetition scheduler
- `0021dc8` feat(beacon/study-store): sqlite store for flashcards + review log
- `5328bc6` feat(beacon/study): card generation pipeline + validator
- `a0b5c12` feat(beacon/study): expose 4 Tauri commands for Study Mode
- `cbfcf6b` feat(beacon/study-ui): BeaconStudyPanel — pick PDF, generate, review
- `5a3342a` feat(beacon/study-nav): Study panel sidebar entry + detach support
- `fa32144` fix(beacon/study): use std::io::Error::other (clippy 1.95)

Branch pushed: `feature/v1.7.0-beacon-bonus-13-study-mode` is up on origin.

---

## NEXT TICK PLAYBOOK — MODE A merge + tag v1.7.0

1. `git fetch origin && git checkout main && git pull --ff-only`
2. `git merge --no-ff feature/v1.7.0-beacon-bonus-13-study-mode -m "Merge v1.7.0 'Study Mode' 🎓 — Q&A flashcards with SM-2 spaced repetition"`
3. Bump version to 1.7.0 in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json` + add CHANGELOG entry. Commit:
   `chore(release): v1.7.0 "Study Mode"`
4. Run all 4 quality gates on main:
   - `cd src-tauri && cargo fmt --all -- --check`
   - `cd src-tauri && cargo clippy --all-targets -- -D warnings`
   - `cd src-tauri && cargo test --lib`
   - `pnpm check` (from repo root)
5. If gates pass → `git tag v1.7.0` then push:
   `git push origin main --follow-tags` (use credential helper).
6. Find CI run id via `gh run list --branch main --limit 1`.
7. Record in STATE.md: `RELEASE_PENDING: v1.7.0 — merge SHA <hash>, tag v1.7.0, CI run <id>`
8. Tick after that: MODE B finalize (`gh release create v1.7.0 ...` with curated artifacts).

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
