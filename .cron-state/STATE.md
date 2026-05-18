# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.8.0 "Glossary" 📖 RELEASE_PENDING + v1.9.0 "Voice Mode" 🔊 RELEASE_PENDING

**Main HEAD**: `3ed93c0` (Merge v1.9.0 'Voice Mode' 🔊)
**Latest published release**: **v1.7.0 "Study Mode" 🎓** — https://github.com/Sanjays2402/slab/releases/tag/v1.7.0
**Pending releases (need MODE B)**:
- **v1.8.0** "Glossary" 📖 — merge SHA `41c6a37`, tag `v1.8.0`, CI run `26037405085` (in_progress at end of this tick)
- **v1.9.0** "Voice Mode" 🔊 — merge SHA `3ed93c0`, tag `v1.9.0`, CI run `26038422918` (queued at end of this tick)

---

## TICK 2026-05-18 06:59 PT — MODE A (v1.8.0 merge) + MODE C (ship v1.9.0)

**MODE A — v1.8.0 "Glossary" 📖 merged to main, tagged, pushed:**
- Merge `--no-ff` of `feature/v1.8.0-beacon-bonus-14-glossary` into main.
- Version bump 1.7.0 → 1.8.0 across package.json, src-tauri/Cargo.{toml,lock}, tauri.conf.json.
- Release notes at `docs/release-notes/v1.8.0.md`.
- Quality gates all green on main.
- Tag `v1.8.0`, push origin main --follow-tags via gh token credential helper.
- CI run id `26037405085`. RELEASE_PENDING.

**MODE C — v1.9.0 "Voice Mode" 🔊 Slice 15 (TTS-first) shipped end-to-end:**
Branch `feature/v1.9.0-beacon-bonus-15-voice-mode` (5 commits, merged):
- Voice scaffold + cross-platform TTS engines (say / espeak-ng / PowerShell) + 20 tests
- Single-slot VoiceSession with kill-prev-on-speak + 5 tests
- Tauri command surface (6 commands) + VoiceConfig persistence + 5 config tests
- `8e7d0f3` Frontend BeaconVoicePanel.svelte + nav + i18n + clippy cleanup
- `a080fc5` chore(release): v1.9.0 "Voice Mode" 🔊 (version bump + release notes)
- Merged into main as `3ed93c0` with --no-ff.
- Tag `v1.9.0` pushed. CI run id `26038422918`. RELEASE_PENDING.

**Quality gates on main after both merges (v1.8.0 then v1.9.0):**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **682 passed; 0 failed** (+30 voice tests vs v1.8.0)
- `pnpm check` — 0 errors, 23 pre-existing warnings

**Key decisions:**
- TTS-first slice; STT (mic + whisper.cpp) deferred to **v1.9.1** — impossible to validate STT in CI without audio HW + external binaries.
- TTS via shell-out to native engines (no audio crate bindings) for portability + CI-friendly unit-tests of command builders.
- `[beacon.voice]` config section uses `skip_serializing_if = is_empty` so existing user configs are not perturbed.

---

## NEXT TICK PLAYBOOK — MODE B x2 (finalize v1.8.0 + v1.9.0)

Both releases pending. Next tick should:

1. `gh run view 26037405085` — if green, download artifacts, `gh release create v1.8.0` with 6 curated artifacts + notes from `docs/release-notes/v1.8.0.md`. Remove v1.8.0 from RELEASE_PENDING.
2. `gh run view 26038422918` — if green, download artifacts, `gh release create v1.9.0` with 6 curated artifacts + notes from `docs/release-notes/v1.9.0.md`. Remove v1.9.0 from RELEASE_PENDING.
3. If either CI fails, write `RELEASE_FAILED:` line with run id + failing job; consider revert or fix-forward on a follow-up branch.
4. After both releases are public, MODE C: open `feature/v1.9.1-beacon-voice-stt` and start mic-input + whisper.cpp integration. STT spec scratch:
   - whisper.cpp CLI bundled per-platform (small.en model, 39MB)
   - `slab_beacon_voice_record_start/stop` Tauri commands
   - Inline mic button on BeaconChatPanel
   - Privacy-first: never persist audio bytes, never network

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **MERGED + TAGGED + CI in flight (run 26037405085)**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **MERGED + TAGGED + CI queued (run 26038422918)**
### v1.9.1 "Voice Mode: Listen" 🎙️ — next: STT + mic + whisper.cpp
### v2.0.0 — TBD (TypeScript Plugins vs. Forge signing)

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## POST-v1.9 ROADMAP REMINDERS

**Slice 15.1 — Voice Mode STT** 🎙️ (v1.9.1)
- whisper.cpp CLI bundled per-platform (small.en, ~39MB)
- New `slab_beacon_voice_record_*` Tauri commands
- Inline mic button on BeaconChatPanel
- Never persist audio, never network

**v2.0.0 candidates:**

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
- CommandPalette DETACHABLE_PANELS drift: missing citations/study/glossary
  entries pre-existed; voice was added this tick but the other three
  remain — quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins`
  GH repo, drop seed files from `docs/marketplace-seed/`, sign the
  hello-slab plugin and post the first real `index.json`.
