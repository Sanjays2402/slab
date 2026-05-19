# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.2 RELEASED 🎙 — v2.0.0 "Workshop" Slice 2 SHIPPED on `feature/v2.0.0-workshop`

**Main HEAD**: `18d4877` (README catch-up for v1.9.2).
**Latest tag**: `v1.9.2` (annotated, pushed).
**Latest release**: https://github.com/Sanjays2402/slab/releases/tag/v1.9.2 (6 assets: macOS arm64 DMG, macOS x64 DMG, Linux deb, Linux AppImage, Windows MSI, Windows exe).
**Active dev branch**: `feature/v2.0.0-workshop` — HEAD `b3706d5`, 3 commits ahead of `main`.
**RELEASE_PENDING**: *(none)*

---

## TICK 2026-05-18 19:38 PT — MODE B finalize v1.9.2 + MODE C Slice 2 of v2.0.0 (vertical slice)

Three-mode tick: finalized the v1.9.2 release pending from prior tick, synced local README back to main, and shipped Slice 2 of v2.0.0 Workshop end-to-end.

**Commits this tick:**
- `11ba4fa` docs(README): bring front door up to v1.9.2 (on feature branch; cherry-picked to main as `18d4877`)
- `18d4877` cherry-pick README catch-up onto main (resolved 2 conflict hunks taking incoming version)
- `7915880` docs(plan): v2.0.0 Workshop — TypeScript Plugins implementation plan (`docs/plans/2026-05-18-v2.0.0-workshop.md`, 12 slices)
- `43479e4` feat(plugins/runtime): manifest schema bump for v2.0.0 — RuntimeManifest, Capabilities, FsCap/NetCap/UiCap/BeaconCap, validate_runtime, +12 tests
- `b3706d5` feat(plugins/runtime): hash-pinned script loading at discovery time — load_and_verify_script, hex_sha256, ScriptOutcome enum, +6 registry tests

**v1.9.2 release finalized:**
- CI run `26071609861` finished 7/7 green
- Downloaded all 6 bundles via `gh run download` to `/tmp/slab-release-1.9.2/`
- `gh release create v1.9.2 --title 'v1.9.2 — Voice Mode: Polish 🎙'` with notes from `docs/release-notes/v1.9.2.md`
- All 6 assets uploaded

**Quality gates on `feature/v2.0.0-workshop`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **754 passed / 0 failed** (up from 736: +12 manifest tests, +6 registry runtime tests)
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**Push:** `feature/v2.0.0-workshop` pushed to origin. README cherry-pick pushed to main.

**Slice 2 design notes:**
- Trust-on-first-use today; pinned SHA-256 in manifest, loader enforces at discovery time
- Hash mismatch = hard failure (parse-error-equivalent: `manifest = None`, `enabled = false`, descriptive error)
- `Plugin.script_bytes: Option<Vec<u8>>` is `#[serde(skip)]` so raw source never reaches frontend
- Forge author signing parked for v2.1 (gated on 10+ plugins in curated index)
- Default-deny capabilities: every `*Cap::Default` is the most restrictive variant
- Strict sha256 validation (64 lowercase hex) in manifest layer means loader does direct string compare

---

## NEXT TICK PLAYBOOK

### Step 1 — MODE C continue v2.0.0 "Workshop"

Slice 1 (rquickjs C-FFI embedding) is the next deliverable. **Reserve a full tick** — initial cargo build with the rquickjs/quickjs-ng FFI is 3-5 minutes the first time, don't share budget with other work.

Slice 1 deliverables (from `docs/plans/2026-05-18-v2.0.0-workshop.md`):
1. Add `rquickjs = { version = "0.6", features = ["loader"] }` to `src-tauri/Cargo.toml`
2. Create `src-tauri/src/plugins/runtime/mod.rs` with a `Runtime` newtype wrapping `rquickjs::Runtime`
3. Create `src-tauri/src/plugins/runtime/sandbox.rs` — fresh `Context` per script, install minimal `console.{log,warn,error}` that pipes into Slab's `tracing` (target=`plugin`, fields include plugin_id)
4. `execute_script(plugin_id, source) -> Result<(), RuntimeError>` API
5. Memory limit (16 MB) + interrupt handler (1 s wall clock) per spec §3
6. Tests: `console_log_pipes_to_tracing`, `script_syntax_error_returned_as_error`, `script_throws_propagates_as_error`, `memory_limit_kills_script`, `time_limit_interrupts_script`, `each_script_gets_fresh_context`
7. Add `cargo test --lib plugins::runtime` to per-tick gate list

Slice 3 (capability-prompt scaffold) can ride along if Slice 1 finishes fast.

### Step 2 — Watch for sibling subagent activity

Note: `/tmp/msg.txt` was written by a sibling subagent (id `5b5e3304-...`) during this tick. No conflicts surfaced, but if future ticks see unexpected edits, check `process(action="list")` for active siblings.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **RELEASED 2026-05-18** (6 assets on GH)
### v1.9.3 "Voice Mode: Windows-native" — Windows WASAPI recorder via cpal (T6 from v1.9.2 plan, plus real impl)
### v2.0.0 "Workshop" — TypeScript Plugins (rquickjs). **In flight on `feature/v2.0.0-workshop`. Slice 2/12 shipped. Plan at `docs/plans/2026-05-18-v2.0.0-workshop.md`.**

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

**v1.9.3** — Windows-native STT (WASAPI via cpal). Real implementation, not the `todo!()` scaffold from v1.9.2 T6. Cargo feature `windows-stt`. ~3-4 commits + integration tests.

**v2.0.0 "Workshop" slice progress:**
- ✅ Slice 2 (manifest schema + hash-pinned loader) — shipped 2026-05-18
- ⏭ Slice 1 (rquickjs embedding + sandboxed console) — NEXT
- ⏭ Slices 3-12 — see plan doc

Slices in target order: 1→rquickjs+console, 2→manifest schema ✅, 3→capability prompt, 4→`slab` global, 5→Beacon tool registration, 6→panel registration, 7→fetch shim, 8→storage, 9→SDK npm pkg, 10→sample plugin+docs, 11→AI provider registration, 12→release.

**v2.1.0 candidates (post-Workshop):**
- **Forge** — author-signed plugins. Wants 10+ plugins in curated index before considering (Sanjay's flag).
- **Slab CLI** — `slab plugin install <url>`.
- **Plugin author cookbook** — recipes for common plugin patterns.

**Parked items (pre-existing):**
- `docs/screenshots-v1.3.1/` working copy in repo root — harmless, can `rm -rf` someday.
- CommandPalette DETACHABLE_PANELS drift — missing citations/study/glossary entries pre-existed v1.9.0; voice was added but the other three remain. Quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins` GH repo, drop seed files from `docs/marketplace-seed/`, sign the hello-slab plugin, post first real `index.json`.
