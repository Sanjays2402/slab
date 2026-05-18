# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.7.0 "Study Mode" 🎓 RELEASED + v1.8.0 "Glossary" 📖 in flight

**Main HEAD**: `521f8ec` (chore(release): v1.7.0 "Study Mode" 🎓)
**Latest release**: **v1.7.0 "Study Mode" 🎓** — https://github.com/Sanjays2402/slab/releases/tag/v1.7.0
**Active dev branch**: `feature/v1.8.0-beacon-bonus-14-glossary` (6 commits, pushed)

---

## TICK 2026-05-18 06:17 PT — MODE B (v1.7.0 finalize) + MODE C (v1.8.0 Slice 14)

**MODE B — v1.7.0 finalize complete:**
- CI run `26034343468` finished with conclusion `success`.
- Downloaded all artifacts to `/tmp/slab-release-v1.7.0`.
- `gh release create v1.7.0` with 6 curated artifacts:
  - macos-arm64 dmg, macos-x64 dmg, linux deb + AppImage, windows nsis + msi.
- Release notes from `docs/release-notes/v1.7.0.md` posted.
- v1.7.0 now public: https://github.com/Sanjays2402/slab/releases/tag/v1.7.0

**MODE C — v1.8.0 "Glossary" 📖 Slice 14 shipped end-to-end this tick:**
Branch `feature/v1.8.0-beacon-bonus-14-glossary` pushed with 6 commits:
- `21745b9` scaffold + types + 2 tests
- `2ea20b8` regex-based candidate detection (4 patterns) + 7 tests
- `2c2d18c` LLM definition extraction + validator + 4 tests
- `ae00094` JSON sidecar cache (load/save/clear) + 5 tests
- `15f5901` Tauri commands (build / load_cache / clear_cache) + invoke_handler
- `be55b4e` BeaconGlossaryPanel.svelte (cache-first UX, filter chips, copy)
- `3dc0e50` Mount panel in `+page.svelte` + i18n key
- `cbda2b1` Clippy `sort_by_key` cleanup

**Slice 14 is functionally complete + STATUS: DONE.** Quality gates:
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **652 passed; 0 failed** (+18 new vs main)
- `pnpm check` — 0 errors, 23 pre-existing warnings

E2E smoke (Task 8 of the plan) deferred — no Ollama running in cron;
will run manually by Sanjay or in the next interactive session.

---

## NEXT TICK PLAYBOOK — MODE A merge v1.8.0

Slice 14 is DONE on `feature/v1.8.0-beacon-bonus-14-glossary`. Next tick
runs MODE A:

1. `git fetch origin && git checkout main && git pull --ff-only`
2. `git merge --no-ff feature/v1.8.0-beacon-bonus-14-glossary -m "Merge v1.8.0 'Glossary' 📖 — auto-extract jargon with definitions"`
3. Version bump 1.7.0 → 1.8.0 in `package.json`, `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.lock` (cargo update -p slab-app --offline).
4. Write `docs/release-notes/v1.8.0.md` (model on v1.6.0 / v1.7.0).
5. Quality gates on main (batched).
6. `git tag -a v1.8.0 -m 'v1.8.0 Glossary'`
7. `git push origin main --follow-tags` via gh token.
8. RELEASE_PENDING: v1.8.0 — write to STATE.md, watch CI.

Then MODE C in same tick: start Slice 15 "Voice Mode" 🔊.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18** (this tick)
### v1.8.0 "Glossary" 📖 — DONE on branch, ready for MODE A next tick
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

## POST-v1.8 ROADMAP REMINDERS

**Slice 15 — Voice Mode** 🔊
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
