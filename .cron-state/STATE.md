# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.2 RELEASED 🎙 — v2.0.0 "Workshop" Slices 1+2+3+4 SHIPPED on `feature/v2.0.0-workshop`

**Main HEAD**: `18d4877` (README catch-up for v1.9.2).
**Latest tag**: `v1.9.2` (annotated, pushed).
**Latest release**: https://github.com/Sanjays2402/slab/releases/tag/v1.9.2 (6 assets).
**Active dev branch**: `feature/v2.0.0-workshop` — HEAD `6bd4cf7`, 8 commits ahead of `main`.
**RELEASE_PENDING**: *(none)*

---

## TICK 2026-05-18 20:44 PT — MODE C v2.0.0 Slice 4 (slab global skeleton + lifecycle + grant cmds + TS bindings)

Big vertical slice: backend `slab` JS API + lifecycle + Tauri commands + TS adapter — three commits in one tick.

**Commits this tick:**
- `0bd7b75` feat(plugins/runtime): slab global skeleton + enable_plugin lifecycle (v2.0.0 Slice 4a)
- `7cdec0e` feat(plugins/grants): plugin_grants_get/set/reset Tauri commands (v2.0.0 Slice 4b)
- `6bd4cf7` feat(plugins/ts): TypeScript grant bindings (v2.0.0 Slice 4b cont'd)

**Slice 4a (`0bd7b75`) — backend `slab` global + lifecycle:**
- New `plugins/runtime/host_api.rs` — `Registrations`, `BeaconToolReg`, `BeaconAiProviderReg`, `UiPanelReg`, `UiToolReg`, `NotifyCall`, `NotifyLevel` (derive Default with Info variant)
- New `plugins/runtime/slab_global.rs` — `install_slab()` builds the JS-side `slab` global with capability-gated `slab.beacon.{registerTool, registerAiProvider}`, `slab.ui.{registerPanel, registerTool, notify}`, plus reserved `slab.document.*` / `slab.storage.*` / `slab.fetch` that throw labelled "ships in Slice N" errors
- `HostBindings { plugin_id, declared, granted, registrations }` cloned into each JS closure; descriptors round-trip through `JSON.stringify` for `.toJSON()` semantics
- `Runtime::enable_plugin(plugin_id, declared, granted, source) -> Result<EnableOutput, RuntimeError>` mirrors `execute_script` lifecycle (16MB cap, 1s interrupt, fresh Context::full) and returns `{ logs, registrations }`
- 8 new contract tests covering slab presence, capability gates, descriptor round-trip, reserved-surface throws, fresh-context isolation → 30/30 runtime tests pass
- Persistent-function event dispatch (slab.document.onOpen handlers) deferred to a future slice — synchronous registration phase only

**Slice 4b (`7cdec0e`) — Tauri grant commands:**
- `plugin_grants_get(plugin_id) -> { has_decision, grants }` — bundled response so the consent modal can distinguish "never asked" from "user said no"
- `plugin_grants_set(plugin_id, grants)` — RMW the toml file
- `plugin_grants_reset(plugin_id)` — forget decision (uninstall path)
- All three registered in `generate_handler!`; no new Rust tests (façades over already-tested `read_grants`/`write_grants`)

**Slice 4b cont'd (`6bd4cf7`) — TS adapter:**
- `PluginGrants` / `PluginGrantsResponse` / `emptyPluginGrants()` exported from `src/lib/plugins.ts`
- `getPluginGrants` / `setPluginGrants` / `resetPluginGrants` — async, gracefully degrade in browser dev mode
- Pre-existing `Manifest.permissions` type kept; new grants are separate axis

**Quality gates on `feature/v2.0.0-workshop` HEAD `6bd4cf7`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **805 passed / 0 failed** (up from 785)
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**Push:** pushed to origin/feature/v2.0.0-workshop.

---

## NEXT TICK PLAYBOOK

### Step 1 — MODE C continue v2.0.0 "Workshop"

**Slice 5 (Cabinet consent modal + plugin enable integration):**
1. New Svelte component `src/lib/panels/PluginConsentModal.svelte` — reads manifest `[capabilities]` + calls `getPluginGrants(id)`; only renders when `has_decision === false`
2. Render per-capability rows (fs, net, ui, beacon) with toggle/select controls; pre-fill with manifest's declared bounds
3. On Save → call `setPluginGrants(id, grants)`; on Deny → `setPluginGrants(id, emptyPluginGrants())` and force-disable the plugin
4. Wire into `PluginsPanel.svelte` enable toggle: before calling `setPluginEnabled(id, true)`, await consent; if `has_decision === false`, show modal first
5. Add "Reset permissions" button in plugin detail view → `resetPluginGrants(id)`

ETA: 3-4 commits. Pure frontend slice — Rust already exposes everything needed.

**Slice 6 (real `slab.document` lifecycle + event dispatch):**
- Per-plugin `Persistent<Function>` table in a `RuntimeRegistry` keyed by plugin_id
- Fire `onOpen`/`onClose` from PDF loader sites
- Requires `unsafe impl JsLifetime` for owned types (tricky — write tests first)

ETA: full tick, write tests upfront.

### Step 2 — Watch for sibling subagent activity

(Same warning as last tick — sibling subagents can touch `/tmp/msg.txt`. Always overwrite right before commit.)

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **RELEASED 2026-05-18** (6 assets on GH)
### v1.9.3 "Voice Mode: Windows-native" — Windows WASAPI recorder via cpal (T6 from v1.9.2 plan, plus real impl)
### v2.0.0 "Workshop" — TypeScript Plugins (rquickjs). **In flight on `feature/v2.0.0-workshop`. Slices 1+2+3+4 shipped (4/12). Plan at `docs/plans/2026-05-18-v2.0.0-workshop.md`.**

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
- ✅ Slice 1 (rquickjs embedding + sandboxed console) — shipped 2026-05-18
- ✅ Slice 3 (capability grants backend + enforce()) — shipped 2026-05-18
- ✅ Slice 4 (`slab` global + lifecycle + Tauri grant cmds + TS bindings) — shipped 2026-05-18
- ⏭ Slice 5 (Cabinet consent modal + enable integration) — NEXT
- ⏭ Slice 6 (Persistent<Function> event dispatch — onOpen/onClose) — after Slice 5
- ⏭ Slices 7-12 — see plan doc

Slices in target order: 1→rquickjs+console ✅, 2→manifest schema ✅, 3→capability backend ✅, 4→`slab` global + lifecycle ✅, 5→Cabinet consent modal, 6→event dispatch, 7→fetch shim, 8→storage, 9→SDK npm pkg, 10→sample plugin+docs, 11→AI provider registration, 12→release.

**v2.1.0 candidates (post-Workshop):**
- **Forge** — author-signed plugins. Wants 10+ plugins in curated index before considering (Sanjay's flag).
- **Slab CLI** — `slab plugin install <url>`.
- **Plugin author cookbook** — recipes for common plugin patterns.

**Parked items (pre-existing):**
- `docs/screenshots-v1.3.1/` working copy in repo root — harmless, can `rm -rf` someday.
- CommandPalette DETACHABLE_PANELS drift — missing citations/study/glossary entries pre-existed v1.9.0; voice was added but the other three remain. Quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins` GH repo, drop seed files from `docs/marketplace-seed/`, sign the hello-slab plugin, post first real `index.json`.
