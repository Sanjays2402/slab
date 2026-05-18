# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.8.0 + v1.9.0 RELEASE_PENDING (CI in flight) + v1.9.1 DONE on branch (ready for MODE A merge)

**Main HEAD**: `3ed93c0` (Merge v1.9.0 'Voice Mode' 🔊)
**Latest published release**: **v1.7.0 "Study Mode" 🎓** — https://github.com/Sanjays2402/slab/releases/tag/v1.7.0

**Pending releases (need MODE B)**:
- **v1.8.0** "Glossary" 📖 — merge SHA `41c6a37`, tag `v1.8.0`, CI run `26037405085`
- **v1.9.0** "Voice Mode" 🔊 — merge SHA `3ed93c0`, tag `v1.9.0`, CI run `26038422918`

**Ready for MODE A merge**:
- **v1.9.1** "Beacon Voice Mode: Listen" 🎙 — branch `feature/v1.9.1-beacon-voice-stt` — STATUS: DONE — all 6 tasks shipped

---

## TICK 2026-05-18 07:57 PT — MODE C (ship v1.9.1 Tasks 4–6)

**Plan**: `docs/plans/2026-05-18-v1.9.1-voice-stt.md` (466 lines)

**Tasks 1–3 (backend)** were already complete pre-tick on branch `feature/v1.9.1-beacon-voice-stt`:
- `4ed4b74` SttEngine + Transcript + capability probe
- `15e0ecf` WAV recorder shell-out (sox/arecord/PowerShell)
- `270464c` SttSession single-slot recorder + transcribe

**This tick shipped Tasks 4–6**:
- `1f39bc2` feat(beacon/voice): Tauri command surface for STT (4 commands)
  - `slab_beacon_voice_stt_capabilities` / `_start(engine?)` / `_stop` (CmdResult<Transcript>) / `_is_recording`
  - `Arc<SttSession>` managed state alongside existing `VoiceSession`
- `8224b86` feat(beacon/voice): mic button + Listen settings (frontend)
  - `BeaconChatPanel.svelte`: pulsing red mic button between composer + Send. Transcript appends with space-pad to existing question; caret to end; focus restored. Mic hidden entirely when sttCapable=false.
  - `BeaconVoicePanel.svelte`: new 🎙 Listen fieldset with engine/recorder status badges + privacy callout + per-OS install hints (`brew install whisper-cpp` etc.).
- (pending commit this tick): chore(release): v1.9.1 — Beacon Voice Mode: Listen
  - Version bump 1.9.0 → 1.9.1 across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`
  - Release notes at `docs/release-notes/v1.9.1.md`

**Quality gates on `feature/v1.9.1-beacon-voice-stt`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **711 passed; 0 failed** (+29 vs v1.9.0)
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**Key decisions:**
- Two independent session slots (`Arc<SttSession>` + `Arc<VoiceSession>`) — user can dictate input while TTS is speaking output without collision.
- `slab_beacon_voice_stt_start(engine: Option<String>)` — `None` → `SttEngine::platform_default()`. Unknown id → user-grade error, no panic.
- Mic button **hidden** when not capable (rather than disabled-with-tooltip) — no broken affordances; install hints live in settings panel only.
- Transcript **appends** to existing composer text with space-pad — supports "Summarise: <dictated>" workflow.
- Pulsing red `mic-pulse` 1.4s keyframe — constant feedback that recording is live.
- Audio bytes never persist beyond transcription call — WAV unlinked unconditionally even on error paths.

---

## NEXT TICK PLAYBOOK — MODE A (merge v1.9.1) + MODE B x2 (finalize v1.8.0 + v1.9.0)

1. **MODE A — merge v1.9.1 to main**:
   - `git fetch origin && git checkout main && git pull`
   - `git merge --no-ff feature/v1.9.1-beacon-voice-stt -m "Merge v1.9.1 'Beacon Voice Mode: Listen' 🎙 — on-device STT via whisper.cpp"`
   - Run quality gates on main (fmt/clippy/test --lib/pnpm check)
   - `git tag v1.9.1` then push: `git push origin main --follow-tags`
   - Record CI run id in STATE.md as RELEASE_PENDING for v1.9.1

2. **MODE B — finalize v1.8.0**:
   - `gh run view 26037405085` — if green, download artifacts, `gh release create v1.8.0` with notes from `docs/release-notes/v1.8.0.md` + 6 curated artifacts. Remove from RELEASE_PENDING.

3. **MODE B — finalize v1.9.0**:
   - `gh run view 26038422918` — if green, download artifacts, `gh release create v1.9.0` with notes from `docs/release-notes/v1.9.0.md` + 6 curated artifacts. Remove from RELEASE_PENDING.

4. After all three are published, MODE C: pick next slice. Candidates:
   - **v1.9.2** "Voice Mode: Polish" — native Windows STT, recording-cancel affordance, voice-driven Beacon commands (dictate → auto-send)
   - **v2.0.0** — TypeScript Plugins vs Forge (signing)

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **MERGED + TAGGED + CI in flight (run 26037405085)**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **MERGED + TAGGED + CI queued (run 26038422918)**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **DONE on feature branch, ready for MODE A merge**
### v1.9.2 "Voice Mode: Polish" — Windows STT, cancel-affordance, voice→send
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

**Slice 15.2 — Voice Mode Polish** 🎙 (v1.9.2)
- Native Windows STT (whisper.cpp + WASAPI recorder)
- Mid-recording cancel/discard affordance (not just stop+transcribe)
- Voice-driven Beacon commands: dictate → auto-send when keyword detected
- Voice selection UI for STT models (small vs base vs medium)

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
