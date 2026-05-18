# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.1 "Beacon Voice Mode: Listen" 🎙 **RELEASED** — pipeline idle, awaiting next slice

**Main HEAD**: `4a84ed4` (fix(beacon/voice): deflake is_speaking_reaps_exited_child on Windows CI) — STATE update commit pending this tick.
**Latest published release**: **v1.9.1 "Beacon Voice Mode: Listen" 🎙** — https://github.com/Sanjays2402/slab/releases/tag/v1.9.1
**Prior release**: **v1.9.0 "Voice Mode" 🔊** — https://github.com/Sanjays2402/slab/releases/tag/v1.9.0

**No RELEASE_PENDING.** No DONE feature branches. Pipeline is idle — next tick is MODE C (DEVELOP).

---

## TICK 2026-05-18 08:41 PT — MODE B finalize v1.9.1

CI run `26042940723` came back **fully green** (7/7 jobs, all platforms — windows/linux/macos-arm64/macos-x64 cargo test + bundle). Flake fix-forward (`4a84ed4`) worked: poll loop with 5s budget / 25ms cadence held up across all runners.

**Finalize actions:**
- `gh run download 26042940723 --dir /tmp/slab-release-1.9.1` — all 4 platform bundles fetched.
- `gh release create v1.9.1 --title 'v1.9.1 — Beacon Voice Mode: Listen 🎙' --notes-file docs/release-notes/v1.9.1.md` with 6 curated assets:
  - `Slab_1.9.1_aarch64.dmg` (8.4 MB) — macOS arm64
  - `Slab_1.9.1_x64.dmg` (9.1 MB) — macOS x64
  - `Slab_1.9.1_amd64.deb` (10.1 MB) — Linux x64
  - `Slab_1.9.1_amd64.AppImage` (84.2 MB) — Linux x64
  - `Slab_1.9.1_x64_en-US.msi` (8.9 MB) — Windows x64
  - `Slab_1.9.1_x64-setup.exe` (5.7 MB) — Windows x64
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v1.9.1
- Temp release dir cleaned.

**Tick ends here.** Local time 08:41 PT — well before the 09:00 PT business-hours blackout. No development this tick (release-only). Next tick (18:00 PT or later) starts MODE C on v1.9.2 OR v2.0.0 (decision below).

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

**New CI run:** `26042940723` → **GREEN** (finalized in next tick).

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
- `8224b86` feat(beacon/voice): mic button + Listen settings (frontend)
- `39d2caf` chore(release): v1.9.1 — version bumps + release notes
- `a98eb5a` chore(cron): STATE.md update

**MODE A — merged to main**:
- `git merge --no-ff feature/v1.9.1-beacon-voice-stt` into main → merge commit `e0e7b0b`
- Tag `v1.9.1` pushed; CI eventually green at `26042940723`.

---

## NEXT TICK PLAYBOOK — MODE C (DEVELOP)

**v1.9.1 is shipped.** No pending releases. Next tick is fresh-slice territory.

### Recommended slice — **v1.9.2 "Voice Mode: Polish"** 🎙

Logical follow-on to v1.9.0/v1.9.1. Sanjay's directive: ship BIG vertical slices, not 1–2 fixes. Polish slice should include:

1. **Native Windows STT** (whisper.cpp + WASAPI recorder)
   - Cargo feature `windows-stt` gating a WASAPI recorder module.
   - `SttEngine::WhisperWindows` variant; capability probe returns `Available` on Win when binary present.
   - End the "Windows = not installed" placeholder; ship full parity with macOS/Linux.
2. **Mid-recording cancel/discard affordance**
   - Currently mic button is `Start` → `Stop+Transcribe`. Add right-click or long-press → `Cancel (discard)`.
   - Backend: new `slab_beacon_voice_stt_cancel` command that drops the WAV without transcribing.
   - Frontend: ESC while recording = cancel; visual feedback "Recording discarded".
3. **Voice-driven Beacon commands** (dictate → auto-send on keyword)
   - Trailing-phrase detection on stop: if transcript ends with "send it" / "go" → auto-submit composer.
   - Configurable trigger word in Voice settings, default "send it".
4. **STT model picker UI** (small / base / medium)
   - whisper-cpp accepts `-m <model.bin>`. Surface model selector in Voice settings.
   - Default `base.en` if available else `small.en`.

**Branch**: `feature/v1.9.2-voice-polish`. Plan file: `docs/plans/2026-05-XX-v1.9.2-voice-polish.md` (write before coding).

### Alternative — **v2.0.0 "TypeScript Plugins" or "Forge"**

If voice polish feels stale, jump to v2.0.0. Two candidates parked below in POST-v1.9 ROADMAP REMINDERS. Pre-flight: v2.0.0 needs a real spec doc first (write in `.cron-state/proposals/`).

**Tick decision rule**: default to v1.9.2 (clear scope, natural arc). Only divert to v2.0.0 if Sanjay has flagged a preference.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **NEXT** (Windows STT, cancel, voice→send, model picker)
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

**Slice 15.2 — Voice Mode Polish** 🎙 (v1.9.2) — see NEXT TICK PLAYBOOK above

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
  entries pre-existed; voice was added in v1.9.0 but the other three
  remain — quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins`
  GH repo, drop seed files from `docs/marketplace-seed/`, sign the
  hello-slab plugin and post the first real `index.json`.
