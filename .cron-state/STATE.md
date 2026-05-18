# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.8.0 + v1.9.0 SHIPPED 🎉 — v1.9.1 "Voice Mode: Listen" 🎙️ in flight (3 commits)

**Main HEAD**: `3ed93c0` (Merge v1.9.0 'Voice Mode' 🔊)
**Latest published releases**:
- **v1.9.0 "Voice Mode" 🔊** — https://github.com/Sanjays2402/slab/releases/tag/v1.9.0
- **v1.8.0 "Glossary" 📖** — https://github.com/Sanjays2402/slab/releases/tag/v1.8.0
- **v1.7.0 "Study Mode" 🎓** — https://github.com/Sanjays2402/slab/releases/tag/v1.7.0

**Active dev branch**: `feature/v1.9.1-beacon-voice-stt` (pushed, NOT MERGED, 3 backend commits — Tasks 1-3 of 6 done)

---

## TICK 2026-05-18 07:18 PT — MODE B x2 (finalize v1.8.0 + v1.9.0) + MODE C (ship v1.9.1 Tasks 1-3)

**MODE B — v1.8.0 finalized:**
- CI run 26037405085 → green.
- 6 artifacts uploaded (mac arm/x64 dmg, linux deb/AppImage, win msi/exe).
- Release notes from `docs/release-notes/v1.8.0.md`.
- Published as **v1.8.0 "Glossary" 📖**.

**MODE B — v1.9.0 finalized:**
- CI run 26038422918 → green (finished mid-tick).
- 6 artifacts uploaded (same matrix).
- Release notes from `docs/release-notes/v1.9.0.md`.
- Published as **v1.9.0 "Voice Mode" 🔊**.

**MODE C — v1.9.1 Voice Mode: Listen 🎙️ Tasks 1-3 shipped (backend complete):**
Branch `feature/v1.9.1-beacon-voice-stt` (4 commits, pushed):
- `937240b` docs(plan): v1.9.1 Voice Mode: Listen (STT) implementation plan (6 tasks)
- `4ed4b74` feat(beacon/voice): SttEngine + Transcript + capability probe (9 tests)
- `15e0ecf` feat(beacon/voice): WAV recorder shell-out (sox/arecord/PowerShell) (8 tests)
- `270464c` feat(beacon/voice): SttSession single-slot recorder + transcribe (12 tests)

**Backend pipeline end-to-end:**
1. `SttEngine` enum + `Transcript` payload + `capabilities()` probe ✓
2. WAV recorder shell-out (16-kHz mono S16_LE WAV, 3 OSes) ✓
3. `SttSession` start/stop with whisper-cli transcribe, privacy-first WAV unlink ✓
4. Tauri command surface — NEXT TICK
5. Frontend mic button + Listen settings — NEXT TICK
6. Release housekeeping (bump 1.9.1, release notes) — NEXT TICK

**Quality gates on `feature/v1.9.1-beacon-voice-stt`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **711 passed; 0 failed** (+29 STT tests vs v1.9.0)
- `pnpm check` — 0 errors, 23 pre-existing warnings

**Key decisions on STT slice:**
- whisper.cpp via shell-out to `whisper-cli`. No FFI bindings; user installs via `brew install whisper-cpp` / `apt`. Auto-bundling deferred to v1.9.2.
- Recorder via shell-out to sox / arecord / PowerShell (Windows.Media.Capture script). No `cpal` audio crate — same hermetic-CI pattern as v1.9.0 TTS.
- WAV format pinned at 16-kHz mono 16-bit PCM (what whisper.cpp expects natively — no resampling cost).
- Privacy: WAV is ALWAYS unlinked in `stop()`, even on whisper failure. Audio bytes never persist, never go off-device.
- Single-slot session model mirrors v1.9.0 `VoiceSession` — kill prior recording on new `start()`.
- `$WHISPER_CLI` env override for users with custom paths; `$WHISPER_MODEL` to pick the GGUF model file.

---

## NEXT TICK PLAYBOOK — MODE C (continue v1.9.1)

The v1.9.1 backend is complete. Three tasks remain on `feature/v1.9.1-beacon-voice-stt`:

### Task 4: Tauri command surface (4 commands)
Wire into `src-tauri/src/lib.rs`:
- `slab_beacon_voice_stt_capabilities() -> SttCapabilities`
- `slab_beacon_voice_stt_start(engine: Option<String>) -> CmdResult<()>`
- `slab_beacon_voice_stt_stop() -> CmdResult<Transcript>`
- `slab_beacon_voice_stt_is_recording() -> bool`

Add `Arc<SttSession>` to managed state alongside the existing `Arc<VoiceSession>` (TTS). Look at `slab_beacon_voice_speak` for the shape — symmetrical.

### Task 5: Frontend (Svelte 5)
- `src/lib/beacon/BeaconChatPanel.svelte`: mic button next to send. Toggles voice_stt_start/_stop. On stop, fills the prompt textarea with returned transcript. Show "recording…" indicator while active.
- `src/lib/beacon/BeaconVoicePanel.svelte`: add "Listen" section with engine selector + "whisper-cli not installed — `brew install whisper-cpp`" hint when capabilities.engines[0].installed=false.
- i18n keys in `src/lib/i18n/en.json`: `beacon.voice.listen.title`, `beacon.voice.listen.mic_button_label`, `beacon.voice.listen.not_installed`, `beacon.voice.listen.recording`.

### Task 6: Release housekeeping
- Bump 1.9.0 → 1.9.1 in: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`.
- Write `docs/release-notes/v1.9.1.md` — emphasise:
  - STT counterpart to v1.9.0 TTS.
  - Privacy-first (audio never persisted, never network).
  - Local-first (whisper.cpp on-device).
  - Prerequisites: `brew install whisper-cpp sox` on macOS / `apt install whisper-cpp alsa-utils` on Debian.
- Mark `STATUS: DONE` so next tick fires MODE A.

### After v1.9.1 merges
- MODE A: merge `feature/v1.9.1-beacon-voice-stt` into main, tag v1.9.1, push --follow-tags, record CI run in STATE.md.
- MODE B (subsequent tick): finalize the release with 6 artifacts.

---

## ROADMAP

### v0.8.1 → v1.7.0 — RELEASED (see git history)
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS) — **RELEASED 2026-05-18**
### v1.9.1 "Voice Mode: Listen" 🎙️ (STT) — **3/6 tasks shipped, branch pushed**
### v1.9.2 — TBD: whisper.cpp auto-bundling (per-platform CLI + small.en model download wizard)
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

**v1.9.2 — Voice Mode bundling**
- Bundle `whisper-cli` binary per-platform in the Tauri sidecar.
- One-click "Download model" wizard (small.en ~39MB).
- Bundled `sox` for macOS too (Homebrew not assumed).

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
  entries pre-existed; voice was added v1.9.0 but the other three
  remain — quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins`
  GH repo, drop seed files from `docs/marketplace-seed/`, sign the
  hello-slab plugin and post the first real `index.json`.
