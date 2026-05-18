# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.1 "Beacon Voice Mode: Listen" 🎙 RELEASE_PENDING (CI re-running after Windows flake fix)

**Main HEAD**: `4a84ed4` (fix(beacon/voice): deflake is_speaking_reaps_exited_child on Windows CI)
**Latest published release**: **v1.9.0 "Voice Mode" 🔊** — https://github.com/Sanjays2402/slab/releases/tag/v1.9.0

**Pending releases (need MODE B)**:
- **v1.9.1** "Beacon Voice Mode: Listen" 🎙 — tag `v1.9.1` moved to `4a84ed4`, CI run `26042940723` (in_progress)

**Last CI failure on v1.9.1 (run `26041745207`):**
- Windows cargo test failed on `ai::voice_session::tests::is_speaking_reaps_exited_child` — 50ms fixed sleep too tight on Windows runners (Git Bash spawn latency ~100–300ms). Pre-existing v1.9.0 test, not a v1.9.1 regression. Fixed with poll-up-to-5s/25ms-cadence — deterministic, fast on every healthy host. 711/711 lib tests pass locally.

---

## TICK 2026-05-18 08:24 PT — MODE B triage + flake fix-forward

CI run `26041745207` for v1.9.1 came back **red**: Windows `cargo test (windows-x64)` failed on a single test, `ai::voice_session::tests::is_speaking_reaps_exited_child` (panic at `voice_session.rs:162` — `assertion failed: !s.is_speaking()`). All 3 other test legs (linux-x64, macos-arm64, windows clippy/fmt) passed.

**Diagnosis:** flake, not a regression. Test plants a `sh -c "exit 0"` child, sleeps a fixed 50ms, asserts the child has been reaped. On Windows runners Git Bash imposes ~100–300ms of spawn latency, so the child is occasionally still alive at the 50ms mark. The test ships with v1.9.0 (released cleanly) but rolled the dice unfavorably on this v1.9.1 run.

**Fix (`4a84ed4`)** — `fix(beacon/voice): deflake is_speaking_reaps_exited_child on Windows CI`:
- Replaced fixed 50ms sleep with poll loop: up to 5s budget, 25ms cadence.
- On a healthy host first iteration suffices; on slow CI a few extra hops.
- Deterministic, fast in the common case, robust under load.
- Local run: 5/5 voice_session tests pass in 0.18s; full lib 711/711 in 3.04s.

**Tag reshuffle:**
- `git tag -d v1.9.1` locally (was at `e0e7b0b`).
- Re-created `v1.9.1` at `4a84ed4` (the fix commit).
- `git push --force origin refs/tags/v1.9.1` — moved on remote (no published release attached, so safe).

**Quality gates on main (after fix):**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **711 passed; 0 failed**
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**New CI run:** `26042940723` (in_progress at end of this tick).

Tick ends right at the start of the weekday business-hours blackout window (09:00 PT), so MODE B finalize (`gh release create v1.9.1` + artifact upload) happens next tick (evening 18:00 PT or later).

---

## TICK 2026-05-18 07:57 PT — MODE C (ship v1.9.1 Tasks 4–6) + MODE A (merge v1.9.1 to main)

**Plan**: `docs/plans/2026-05-18-v1.9.1-voice-stt.md` (466 lines)

**MODE C — feature/v1.9.1-beacon-voice-stt finished**:
Tasks 1–3 (backend) already shipped pre-tick:
- `4ed4b74` SttEngine + Transcript + capability probe
- `15e0ecf` WAV recorder shell-out (sox/arecord/PowerShell)
- `270464c` SttSession single-slot recorder + transcribe

This tick shipped Tasks 4–6:
- `1f39bc2` feat(beacon/voice): Tauri command surface for STT (4 commands)
  - `slab_beacon_voice_stt_capabilities` / `_start(engine?)` / `_stop` (CmdResult<Transcript>) / `_is_recording`
  - `Arc<SttSession>` managed state alongside existing `VoiceSession`
- `8224b86` feat(beacon/voice): mic button + Listen settings (frontend)
  - `BeaconChatPanel.svelte`: pulsing red mic button between composer + Send. Transcript appends space-padded; caret to end; focus restored. Mic hidden entirely when sttCapable=false.
  - `BeaconVoicePanel.svelte`: 🎙 Listen fieldset with engine/recorder status badges + privacy callout + per-OS install hints.
- `39d2caf` chore(release): v1.9.1 — version bumps + release notes (`docs/release-notes/v1.9.1.md`)
- `a98eb5a` chore(cron): STATE.md update

**MODE A — merged to main**:
- `git merge --no-ff feature/v1.9.1-beacon-voice-stt` into main → merge commit `e0e7b0b`
- Resolved STATE.md conflict (took feature-branch version)
- Tag `v1.9.1` pushed
- CI run id `26041745207` (in_progress)

**Quality gates on main after merge:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **711 passed; 0 failed** (+29 vs v1.9.0)
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**Key decisions:**
- Two independent session slots (`Arc<SttSession>` + `Arc<VoiceSession>`) — dictate input while TTS speaks output without collision.
- `slab_beacon_voice_stt_start(engine: Option<String>)` — `None` → `SttEngine::platform_default()`. Unknown id → user-grade error, no panic.
- Mic button **hidden** when not capable — no broken affordances; install hints live in settings panel only.
- Transcript **appends** to existing composer text with space-pad — supports "Summarise: <dictated>" workflow.
- Audio bytes never persist beyond transcription call — WAV unlinked unconditionally even on error paths.

---

## NEXT TICK PLAYBOOK — MODE B (finalize v1.9.1)

1. **MODE B — finalize v1.9.1**:
   - `gh run view 26042940723` — if green:
     - `mkdir -p /tmp/slab-release-1.9.1 && gh run download 26042940723 --dir /tmp/slab-release-1.9.1`
     - `gh release create v1.9.1 --title 'v1.9.1 — Beacon Voice Mode: Listen 🎙' --notes-file docs/release-notes/v1.9.1.md` with 6 curated artifacts (macos arm64+x64 dmg, linux x64 deb+AppImage, windows msi+nsis).
     - Remove RELEASE_PENDING line from STATE.md.
   - If CI fails again → write `RELEASE_FAILED:` line with run id + failing job; consider revert or fix-forward.

2. After v1.9.1 published, MODE C — pick next slice:
   - **v1.9.2** "Voice Mode: Polish" — native Windows STT (whisper.cpp + WASAPI recorder), recording-cancel/discard affordance, voice-driven Beacon commands (dictate → auto-send on keyword)
   - **OR v2.0.0** — TypeScript Plugins vs Forge (signing)

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **MERGED + TAGGED + CI in flight (run 26041745207)**
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
