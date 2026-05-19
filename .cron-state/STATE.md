# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.2 RELEASED 🎙 — v2.0.0 "Workshop" Slices 1+3 SHIPPED on `feature/v2.0.0-workshop`

**Main HEAD**: `18d4877` (README catch-up for v1.9.2).
**Latest tag**: `v1.9.2` (annotated, pushed).
**Latest release**: https://github.com/Sanjays2402/slab/releases/tag/v1.9.2 (6 assets).
**Active dev branch**: `feature/v2.0.0-workshop` — HEAD `5c9533a`, 5 commits ahead of `main`.
**RELEASE_PENDING**: *(none)*

---

## TICK 2026-05-18 19:56 PT — MODE C BIG vertical slice: v2.0.0 Slice 1 (rquickjs) + Slice 3 (grants)

Per Sanjay's "ship big things every tick" directive, used the full tick to land **two** non-trivial v2.0.0 slices in one go. Slice 1 was projected as risky (3-5 min first rquickjs build) but actual cold compile was only ~17s on the M-series host, so I had budget for Slice 3 too.

**Commits this tick:**
- `bd00df0` feat(plugins/runtime): sandboxed QuickJS interpreter for plugin scripts (v2.0.0 Slice 1)
- `5c9533a` feat(plugins/grants): capability grant store + enforcement gate (v2.0.0 Slice 3)

**Slice 1 (`bd00df0`) — rquickjs embedding:**
- `rquickjs = "0.11"` (default-features=false, features=["std"]) added to `src-tauri/Cargo.toml`
- New module `plugins/runtime/{mod.rs,sandbox.rs}`
- `Runtime` newtype wraps `rquickjs::Runtime` with 16MB memory cap + 1s wall-clock interrupt
- `execute_script(plugin_id, source)` builds a fresh `Context` per call → drops on return
- `console.{log,warn,error}` wired into a per-call `Arc<Mutex<Vec<LogEntry>>>` shared buffer
- Variadic args coerced via `Rest<Coerced<String>>` (JS `String(x)` semantics, space-joined)
- `RuntimeError` enum: `Init` / `Syntax` / `Thrown` / `TimeLimit { limit_ms }` / `MemoryLimit { limit_bytes }`
- Syntax-vs-thrown discrimination via `Exception.as_object().get(PredefinedAtom::Name) == "SyntaxError"`
- 10 tests, all passing: console pipe, level tagging, type coercion, syntax/throw/time/memory error paths, fresh-context cross-Runtime and intra-Runtime, empty script, plugin_id propagation

**Slice 3 (`5c9533a`) — capability grants:**
- New module `plugins/grants.rs` — pure backend, no IPC yet
- `PluginGrants` — user-granted capabilities (separate axis from declared)
- `PluginGrants::covers(declared)` — detects plugin escalation on update
- `GrantStore` flat-map keyed by plugin_id, persists as TOML at `~/.slab/plugin-grants.toml`
- `read_grants` / `write_grants` mirror enabled-state file pattern; corrupt file → empty (no lockout)
- `CapabilityRequest` enum: `FsRead`, `FsWrite`, `NetFetch{host}`, `UiRegisterPanel`, `UiRegisterTool`, `BeaconRegisterTool`, `BeaconRegisterAiProvider`
- `enforce(declared, granted, req)` — both must permit; returns `Result<(), DenyReason>`
- `DenyReason`: `NotDeclared`, `NotGranted`, `GrantTooNarrow`, `HostNotAllowed`
- Manifest re-exports widened to expose `Capabilities`, `FsCap`, `NetCap`, `UiCap`, `BeaconCap`, `RuntimeManifest` for host shim consumption
- 21 tests, all passing

**Quality gates on `feature/v2.0.0-workshop` HEAD `5c9533a`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean (fixed two `match → matches!()` lints in grants.rs)
- `cargo test --lib` — **785 passed / 0 failed** (up from 754: +10 runtime, +21 grants)
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**Push:** pushed to origin/feature/v2.0.0-workshop.

**Design notes from this tick:**
- rquickjs 0.11 is the current crates.io release; 0.10 (from the plan doc) was wrong, 0.11 was selected. cc-compile of QuickJS is genuinely fast on M-series — incremental rebuilds <1s.
- Fresh-context-per-execution is the v2.0.0 contract (verified by test). Long-lived contexts for stateful event handlers wait for Slice 4 lifecycle work.
- Grants module is *backend only*. The actual Cabinet modal that lets users pick a granted set is folded into a new "Slice 3b" entry on the plan; that's pure Svelte work.
- Manifest declared bound vs user grant are two independent axes. `enforce()` requires both to permit. This catches the "user granted X but plugin's manifest never declared X" case (returns `NotDeclared` — manifest is the contract).

---

## NEXT TICK PLAYBOOK

### Step 1 — MODE C continue v2.0.0 "Workshop"

**Slice 4 (`slab` global skeleton + lifecycle)** is the next big chunk. From the spec proposal:
1. Inside `runtime/sandbox.rs`, install a `slab` global Object on every fresh Context with these subsections (initially stub fns that return `undefined`):
   - `slab.beacon.{registerTool, registerAiProvider}`
   - `slab.ui.{registerPanel, registerTool}`
   - `slab.document.{getActive, onOpen, onClose}`
   - `slab.storage.{get, set, remove}` (Slice 8 wires the real sqlite)
   - `slab.fetch(url, init?)` (Slice 7 wires real reqwest)
2. Plumb the plugin_id + `(declared, granted)` capability refs through to each host shim via closure capture so calls can `enforce()` before doing work.
3. Lifecycle hooks: a plugin's `script.js` should be evaluated once at *enable* time (not at every event). The eval result is the plugin's "registration phase"; subsequent host events (open document, run tool) dispatch to registered handlers via a per-plugin `Persistent<Function>` table.
4. Tauri commands `plugin_grants_get(plugin_id)`, `plugin_grants_set(plugin_id, grants)`, `plugin_grants_reset(plugin_id)` for the Cabinet UI.

ETA: 5-6 commits. Reserve a full tick. Persistent lifetimes can be subtle, write tests early.

### Step 2 — Slice 3b (Cabinet modal) can ride along if budget allows

Svelte modal that reads from `Manifest.runtime.capabilities` and writes a `PluginGrants` via the new Tauri commands. Trivial UI; depends on Slice 4 commands being live first.

### Step 3 — Watch for sibling subagent activity

Note: `/tmp/msg.txt` was touched by a sibling subagent (id `0aa53d4a-...`) during this tick — possibly a parallel Hermes run. No actual conflicts surfaced (the only shared file was `/tmp/msg.txt`, which I overwrote with my own commit message right before each commit). If future ticks see unexpected edits in `.cron-state/` or the active branch, check `process(action="list")` for active siblings.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **RELEASED 2026-05-18** (6 assets on GH)
### v1.9.3 "Voice Mode: Windows-native" — Windows WASAPI recorder via cpal (T6 from v1.9.2 plan, plus real impl)
### v2.0.0 "Workshop" — TypeScript Plugins (rquickjs). **In flight on `feature/v2.0.0-workshop`. Slices 1+2+3 shipped (3/12). Plan at `docs/plans/2026-05-18-v2.0.0-workshop.md`.**

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
- ⏭ Slice 4 (`slab` global + lifecycle + Tauri grant commands) — NEXT
- ⏭ Slice 3b (Cabinet prompt modal) — after Slice 4
- ⏭ Slices 5-12 — see plan doc

Slices in target order: 1→rquickjs+console ✅, 2→manifest schema ✅, 3→capability backend ✅, 4→`slab` global + lifecycle, 5→Beacon tool registration, 6→panel registration, 7→fetch shim, 8→storage, 9→SDK npm pkg, 10→sample plugin+docs, 11→AI provider registration, 12→release.

**v2.1.0 candidates (post-Workshop):**
- **Forge** — author-signed plugins. Wants 10+ plugins in curated index before considering (Sanjay's flag).
- **Slab CLI** — `slab plugin install <url>`.
- **Plugin author cookbook** — recipes for common plugin patterns.

**Parked items (pre-existing):**
- `docs/screenshots-v1.3.1/` working copy in repo root — harmless, can `rm -rf` someday.
- CommandPalette DETACHABLE_PANELS drift — missing citations/study/glossary entries pre-existed v1.9.0; voice was added but the other three remain. Quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins` GH repo, drop seed files from `docs/marketplace-seed/`, sign the hello-slab plugin, post first real `index.json`.
