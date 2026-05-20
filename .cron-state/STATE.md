# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.2 RELEASED 🎙 — v2.0.0 "Workshop" Slices 8 + 9 **DONE on `feature/v2.0.0-workshop`** 📦

**Main HEAD**: `18d4877` (README catch-up for v1.9.2).
**Latest tag**: `v1.9.2` (annotated, pushed).
**Latest release**: https://github.com/Sanjays2402/slab/releases/tag/v1.9.2 (6 assets).
**Active dev branch**: `feature/v2.0.0-workshop` — HEAD `fef65de`, **44 commits ahead of `main`**.
**RELEASE_PENDING**: *(none)*
**LAST_WOW_TICK_AT**: 2026-05-19 21:?? PT — `@slab/plugin-sdk` IntelliSense over entire `slab.*` surface (screenshot-able).

---

## TICK 2026-05-19 21:?? PT — MODE C v2.0.0 Slice 9 SHIPPED 📦 (@slab/plugin-sdk npm package, 5 commits, ~1200 LOC)

**Branch**: `feature/v2.0.0-workshop` — pushed to origin.
**Plan**: `docs/plans/2026-05-19-v2.0.0-workshop-slice-9.md` (20.8 KB).

The plugin authoring SDK is **done**. Third-party authors no longer need
to read Rust source to write a Slab plugin — they `npm i -D @slab/plugin-sdk`,
type `import { definePlugin } from "@slab/plugin-sdk"`, and the entire
`slab.*` surface lights up in IntelliSense. **Buy-Button: Pick-us pass** —
Adobe doesn't ship a plugin SDK; PDF Expert/Foxit don't even have a
plugin model. **WOW**: ambient `declare global` propagates into the
emitted `.d.ts` so a single import wires up the whole surface.

**Commits this tick (5):**
- `2c464c5` feat(sdk): @slab/plugin-sdk package skeleton (Slice 9.1)
- `7970e12` feat(sdk): typed mirrors for slab.{manifest,beacon,ui,document,storage,fetch} (Slice 9.2)
- `e0baad9` feat(sdk): definePlugin + assertSlab + global ambient + smoke test (Slice 9.3)
- `b3b7787` feat(sdk): three example plugins — hello-workshop, storage-counter, url-fetch (Slice 9.4)
- `fef65de` docs(sdk): top-level README + CHANGELOG for @slab/plugin-sdk (Slice 9.5)

**What's at `sdk/slab-plugin-sdk/`:**
- `package.json` — `@slab/plugin-sdk@0.1.0`, MIT license, dual ESM+CJS+`.d.ts` exports.
- `src/types/` — 7 modules (manifest, beacon, ui, document, storage, fetch, global) totaling ~23 KB, every type with `@see` JSDoc pointing at the Rust ground-truth file:line.
- `src/define.ts` — `definePlugin(spec)`, `assertSlab(slab)`, `trySlab(slab)` (~85 LOC, ~0.5 KB gzipped).
- `src/index.ts` — public re-exports + ambient `declare global { var slab: SlabGlobal }` block (tsc propagates into emitted `.d.ts`).
- `tests/typecheck-smoke.ts` — 170 LOC exhaustive consumer-side test with positive + `@ts-expect-error` negative cases.
- `examples/{hello-workshop,storage-counter,url-fetch}/` — three reference plugins, < 100 LOC each, manifest + script + README.
- `scripts/rename-cjs.mjs` — post-build CJS extension fixer (TS #54573 workaround); handles `.js → .cjs`, `.js.map → .cjs.map`, and rewrites `require()` + `sourceMappingURL` refs.
- `README.md` (7.5 KB) — install, quickstart, API tour, capability lattice table, security model, examples links.
- `CHANGELOG.md` (3.3 KB) — Keep-a-Changelog 1.1.0 format, 0.1.0 entry documenting every addition.
- 4 tsconfig files: base, `.build.json` (ESM), `.cjs.json`, `.types.json`, `.tests.json` (with path-mapping so examples typecheck in-tree).

**Build verification:** clean dist = 54 files / 220 KB.
- `dist/esm/` 9 `.js` + 9 `.js.map`
- `dist/cjs/` 9 `.cjs` + 9 `.cjs.map`
- `dist/types/` 9 `.d.ts` + 9 `.d.ts.map`

**Quality gates (all green):**
- `tsc --noEmit -p tsconfig.tests.json` → exit 0
- `cargo fmt --all -- --check` → clean
- `cargo clippy --all-targets -- -D warnings` → clean (no rust touched, but verified)
- `pnpm check` → 0 errors, 35 pre-existing CSS warnings (unrelated)

**Key design decisions:**
- **MIT license for SDK** (vs GPL-3.0 parent): keeps plugin-author friction low, no copyleft contamination of third-party plugin source.
- **`declare global` inline in `src/index.ts`** (not a separate `ambient.d.ts`): tsc emits broken runtime `require("./ambient.cjs")` for side-effect imports. Inline declaration propagates cleanly into `dist/types/index.d.ts`.
- **CJS rename via post-build script**: tsc still doesn't honor `outFileExtension` (TS #54573). Tiny node script handles it.
- **Tests path-mapped via `tsconfig.tests.json`**: `paths: { "@slab/plugin-sdk": ["./src/index.ts"] }` so examples can `import from "@slab/plugin-sdk"` and typecheck without `pnpm install`.

**Out of scope (Slice 9 follow-up):**
- `npm publish @slab/plugin-sdk` — requires `@slab` org ownership.
- Foundry Plugin Store UI panel (planned next as part of v2.0.0 finalize).

**Next ticks:**
1. **Merge `feature/v2.0.0-workshop` → main** as v2.0.0 "Workshop" release. 44 commits ahead of main. Run full quality gates on main first.
2. **Foundry Plugin Store UI** — discover/install/uninstall plugin panel; first-party listing for the 3 example plugins from Slice 9.
3. **v0.10.0 "Beacon"** spec is queued at `.cron-state/proposals/v0.10.0-beacon-ai.md`.

---

## TICK 2026-05-19 02:33 PT — MODE C v2.0.0 Slice 8.2-8.7 SHIPPED 🗄️ (slab.storage.* end-to-end live)

Massive vertical slice — Slice 8 of v2.0.0 Workshop is now COMPLETE on
the feature branch. `slab.storage.{get,set,remove,list,clear,usage}`
is fully wired from plugin JS → rquickjs binding → `HostBindings` →
SQLite (`~/.slab/plugin-storage.sqlite`), with per-plugin scoping
enforced in every SQL WHERE clause and a three-axis quota system.
Three commits this tick.

**Commits this tick:**
- `8b10219` feat(plugins/storage): kv CRUD + per-plugin quotas + 16 unit tests (v2.0.0 Slice 8.2-8.3)
- `26487f3` feat(plugins/runtime): plumb SharedPluginStorage through HostBindings (v2.0.0 Slice 8.4)
- `4534aa5` feat(plugins/runtime): slab.storage.* live in JS (v2.0.0 Slice 8.5+8.7)
- `73d2210` feat(plugins/runtime): slab.storage.* E2E tests + microtask pump (v2.0.0 Slice 8.6)

**Slice 8.2-8.3 (`8b10219`) — CRUD + quotas + 16 unit tests:**
- Six methods on `PluginStorage`: `kv_get` / `kv_set` / `kv_remove` /
  `kv_list` / `kv_clear` / `kv_usage_bytes`. All take `&plugin_id` as
  first arg; scoping is enforced in SQL only.
- `kv_set` does the quota dance: `current_total − prev_size + new_size`
  against `MAX_PLUGIN_BYTES`. Overwrite of an existing key doesn't
  double-count (covered by `overwrite_does_not_double_count`).
- 16 new unit tests cover: scoping (cross-plugin invisibility),
  CRUD round-trip, unicode + embedded NUL bytes, overwrite, sorted
  list output, KeyTooLong/ValueTooLarge/QuotaExceeded boundaries,
  usage byte accounting. 24/24 storage tests green.

**Slice 8.4 (`26487f3`) — HostBindings plumbing:**
- New `pub storage: Option<SharedPluginStorage>` field on `HostBindings`.
- `PluginActor::spawn` now calls `shared_storage().ok()`. Filesystem
  failure → `storage = None` → bindings reject Promise per-call
  instead of crashing the actor (better UX than fatal init).
- New `PluginActor::spawn_inner(..., Some(shared))` test seam — used
  by Slice 8.6 E2E tests with `shared_in_memory()` for determinism.
- Ephemeral `enable_plugin` and `execute_script` paths pass `None` so
  declarative-only plugins still load cleanly.

**Slice 8.5+8.7 (`4534aa5`) — live JS bindings + sentinel flip:**
- Six `make_storage_*` factories in `slab_global.rs`. Each: lock the
  mutex, call the corresponding `kv_*` method, wrap result in a
  Promise. When `b.storage` is `None`, return a rejected Promise with
  "slab.storage unavailable outside the per-plugin runtime".
- Module-level docs and the `enable_plugin_reserved_surfaces_throw…`
  sentinel test in `mod.rs` updated to reflect Slice 8 LIVE status —
  rewrote the test to verify the new contract (rejected Promise on
  ephemeral path, NOT a sync throw).

**Slice 8.6 (`73d2210`) — E2E tests + critical microtask pump fix:**
- 7 new E2E tests using `spawn_inner` + `shared_in_memory()`. All green.
- **Two bugs surfaced and fixed in flight:**
  1. `resolve_now` was using `Promise.resolve(value)` via
     `Promise` global; rquickjs 0.11 tuple-arg marshalling treated
     `(value,)` as "not an object". Replaced with `Promise::new(ctx)` +
     immediate `resolve.call((value,))?` — same pattern fetch uses.
  2. **Actor never pumped microtasks after eval.** Plugin scripts
     using the canonical `(async () => { ... })()` IIFE idiom would
     suspend on the first `await` and never wake. Now: after eval
     succeeds, drain `execute_pending_job` to completion (outside
     `ctx.with` to avoid nested-borrow panic). Fetch path still pumps
     after each `dispatch_fetch`.

**Quality gates on `feature/v2.0.0-workshop` HEAD `73d2210`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **902 passed / 0 failed** (879 prior + 16
  storage CRUD unit tests + 7 storage E2E tests; sentinel test
  rewritten in place — no net delta from that file)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Slice 8 status:** COMPLETE except for 8.8 (final push). Pushing this
tick.

---

## TICK 2026-05-19 01:40 PT — MODE C v2.0.0 Slice 8 plan + Slice 8.1 scaffolding

Plan-first tick (writing-plans skill invoked). Slice 8 (`slab.storage.*`)
ships a per-plugin, sqlite-backed key/value store with no manifest
capability gate (scoping IS the security boundary) — a deliberate
departure from the slab.fs/slab.net/slab.ui/slab.beacon pattern. Worth
documenting before writing 1230 LOC of host code.

**Commits this tick:**
- `457503a` docs(plans): v2.0.0 Slice 8 sub-plan — slab.storage.* per-plugin KV store
- `26d498f` feat(plugins/storage): PluginStorage skeleton + sqlite schema (v2.0.0 Slice 8.1)

**Slice 8 plan (`docs/plans/2026-05-19-v2.0.0-workshop-slice-8.md`, 826 lines):**
- 8 sub-tasks (8.1 module+schema ✅, 8.2 CRUD+quotas, 8.3 12 unit tests,
  8.4 plumb into HostBindings, 8.5 5 JS bindings, 8.6 5 E2E tests, 8.7
  flip reserved-surface sentinel, 8.8 gates+push).
- Architecture: process-wide `OnceLock<Arc<Mutex<PluginStorage>>>` over
  one `~/.slab/plugin-storage.sqlite`. Mirrors `fetch::shared_client()`
  shape. Per-plugin scoping enforced in code — every WHERE clause pins
  `plugin_id`.
- Major divergence from Slice 7: NO actor round-trip for the JS API.
  Sqlite k/v ops are microseconds; we run them synchronously inside
  `Mutex::lock()` and wrap results in `Promise.resolve(...)` /
  `Promise.reject(...)`. Forward-compatible — can switch to actor
  bridging later without breaking the JS API.
- Quota model: 8 MiB total per plugin / 1 MiB per value / 64 KiB per key.
  Compile-time `const_assert!` chain enforces invariants between them
  (e.g. value cap ≤ plugin cap, plugin cap is a multiple of value cap).

**Slice 8.1 (`26d498f`) — module scaffolding (~480 LOC, 2 files):**
- New `src-tauri/src/plugins/storage.rs`:
  - `PluginStorage` owning handle wrapping `rusqlite::Connection`.
  - Schema v1: `kv(plugin_id, key, value, value_size, updated_at)` with
    `PRIMARY KEY (plugin_id, key)` + `idx_kv_plugin`. `value` is BLOB
    (not TEXT) so arbitrary UTF-8 with embedded NULs round-trips.
    `value_size` denormalised for fast `SUM()` quota queries.
  - `open()` / `open_in_memory()` / `shared_storage()` / test-only
    `shared_in_memory()`.
  - `StorageError` enum pre-declares `KeyTooLong` / `ValueTooLarge` /
    `QuotaExceeded` variants now (CRUD lands in 8.2; pre-declaration
    keeps the Display assertions testable today).
  - Public quota constants: `MAX_PLUGIN_BYTES` / `MAX_VALUE_BYTES` /
    `MAX_KEY_BYTES`. Compile-time `const _: () = assert!(...)` chain
    enforces the invariants between them.
- Wired into `plugins/mod.rs`: `pub mod storage;` + re-exports.
- 8 unit tests cover schema init, PRAGMA user_version stamping, index
  existence, compound-PK behaviour (same key allowed across plugins),
  init_schema idempotency, in-memory handle isolation, default path
  lives under `~/.slab/`, and StorageError Display messages include the
  numeric size info the JS binding embeds into rejected-Promise messages.

**Why a plan tick (again) instead of full Slice 8:** Slice 8 has one
non-obvious design decision worth pinning before writing code (no
manifest cap), one structural divergence from Slice 7 (sync-then-wrap
vs actor round-trip), and a security invariant (per-plugin scoping in
SQL only — no other layer enforces it) that needs explicit
documentation so a future maintainer doesn't refactor the WHERE
clauses out. Plan first, ship the obvious scaffolding, dispatch
focused sub-task subagents for 8.2-8.8 in future ticks.

**Quality gates on `feature/v2.0.0-workshop` HEAD `26d498f`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **879 passed / 0 failed** (871 prior + 8 new
  storage scaffolding tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 23:30 PT — MODE C v2.0.0 Slice 7 SHIPPED (host fetch + JS binding + E2E)

Two-commit big vertical slice. `slab.fetch` is now a live, capability-gated,
timeout-bounded HTTP client surface for runtime plugins. Plugins can now
`await slab.fetch(url, init?)` and get back a web-Fetch-shaped Response
object (status, headers, text(), json()).

**Commits this tick:**
- `b5aaead` feat(plugins/runtime): host fetch executor — slab.fetch backend infra (v2.0.0 Slice 7.1-7.3)
- `3929d48` feat(plugins/runtime): live slab.fetch JS binding — host-mediated HTTP from plugins (v2.0.0 Slice 7.4)

**Slice 7.1-7.3 (commit 1) — backend (~750 LOC, 3 files):**
- New module `src-tauri/src/plugins/runtime/fetch.rs`: process-global
  `reqwest::Client` (rustls, 30s default timeout, 10-redirect cap,
  cookies off, 16 MiB body cap on both directions). `do_fetch` async,
  `response_to_js` builds the JS Response-like Object with `text()` /
  `json()` methods. URL parsing uses `reqwest::Url` re-export (no new
  direct dep on `url`).
- `actor.rs`: `RuntimeCmd::Fetch { request_id, request }` variant —
  intentionally carries only `Send` data; the `Persistent<Function>`
  resolve/reject callbacks live in a worker-local `PendingFetches`
  table keyed by request_id. Solves `RuntimeCmd: Send + Clone` while
  still routing settlement back into the actor's `Context`.
- `dispatch_fetch` helper: `tokio::Handle::try_current()` → use Tauri's
  runtime if alive, else build a single-threaded current-thread rt for
  unit tests. Wall-clock interrupt set fresh per dispatch.

**Slice 7.4 (commit 2, `3929d48`) — live JS binding (~500 LOC net):**
- `slab_global.rs::make_fetch`: builds `slab.fetch(url, init?)`. Pre-flight
  host parse + capability gate (sync throw on deny like every other
  `slab.*` surface), then mints a Promise via `rquickjs::Promise::new`,
  persists resolve/reject into the worker-local map, sends
  `RuntimeCmd::Fetch` at the actor's own channel.
- `dispatch_fetch` now drains `execute_pending_job` after settling so
  awaiting `.then` bodies run in the same tick (otherwise plugin code
  would starve until the next actor command).
- `HostBindings.{cmd_tx, pending_fetches}` plumbed through `run_actor`
  signature. Both `Option` to keep the ephemeral `enable_plugin` path
  valid; on that path `slab.fetch` returns an already-rejected Promise.
- Flipped `enable_plugin_reserved_surfaces_throw_with_slice_label` from
  `slab.fetch` (now live) to `slab.storage.get` (still Slice-8 placeholder).
- 3 new E2E tests (all green):
  - `slab_fetch_round_trip_resolves_with_body_via_actor` — one-shot
    `127.0.0.1` HTTP server in-process, plugin does `await slab.fetch(...)`,
    we observe `r.text()` via `slab.ui.notify`. End-to-end through
    reqwest + Promise + microtask drain.
  - `slab_fetch_throws_sync_when_net_not_granted` — capability gate.
  - `slab_fetch_throws_sync_when_host_not_in_allowlist` — allowlist enforcement.

**Quality gates on `feature/v2.0.0-workshop` HEAD `3929d48`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **871 passed / 0 failed** (848 prior + 19 fetch
  unit tests in commit 1 + 4 actor/binding tests in commit 2)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 23:01 PT — MODE C v2.0.0 Slice 6.7 + 6.8 (Slice 6 COMPLETE)

True end-to-end vertical slice: the actor system from 6.1–6.6 is now
wired into Slab's real plugin enable flow AND the PDF viewer's real
load/teardown path. `slab.document.{onOpen,onClose,getActive}` is no
longer "wired but no callers" — it's live every time a user enables a
runtime plugin and opens a PDF.

**Commits this tick:**
- `6bd171d` feat(plugins): Tauri document-event commands + actor lifecycle on enable (v2.0.0 Slice 6.7)
- `65588a5` feat(viewer): wire ReaderPanel into plugin document lifecycle (v2.0.0 Slice 6.8)

**Slice 6.7 (`6bd171d`) — backend + enable wiring (~420 insertions, 2 files):**
- New `slab_plugins_document_opened(path, registry)` and `_closed` Tauri
  commands. Each builds `DocumentEvent::from_path(path)` and calls
  `registry.broadcast(RuntimeCmd::Document{Opened,Closed}(ev))`. Both
  registered in `tauri::generate_handler!`.
- `slab_plugins_set_enabled` now takes `runtime_reg: State<PluginRuntimeRegistry>`.
  On enable for `[runtime]` plugins: spawn `PluginActor` with grants from
  `~/.slab/plugin-grants.toml` (deny-all default), insert into registry.
  On disable: `registry.remove(id)` → `LiveEntry::Drop` → worker
  Shutdown + join. Declarative-only plugins untouched.
- 5 new registry-level integration tests verify broadcast reaches real
  JS handlers across 2+ plugins, removed plugins are skipped,
  re-insertion shuts down the old actor cleanly, and open→close fires
  in order.

**Slice 6.8 (`65588a5`) — frontend hook (~54 LOC, 1 file):**
- `ReaderPanel.svelte` got `notifyPluginsDocumentOpened/Closed` helpers
  — fire-and-forget invokes guarded by `isInTauri()`; failures log to
  `console.debug`. `lastPluginPath` tracks which path has an
  outstanding "opened" so every close pairs with a real prior open.
- `loadBytes()` fires `_opened` after existing audit kickoffs (covers
  open, drag-drop, recents, post-OCR/Polyglot/decrypt — every load
  path funnels here).
- `tearDownDoc()` fires `_closed` BEFORE clearing pdfjs state (mirrors
  the actor's "clear active_doc before dispatch" ordering so plugin
  `onClose` handlers observe `getActive() === null`).
- onDestroy already calls tearDownDoc → tab-close & app-exit fire close.

**Quality gates on `feature/v2.0.0-workshop` HEAD `65588a5`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **848 passed / 0 failed** (843 prior + 5 new
  registry/broadcast integration tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 22:23 PT — MODE C v2.0.0 Slice 6.5 (real actor runtime) + 6.6 finalize

Two commits, BIG vertical slice: `PluginRuntimeRegistry` finalized + Tauri-managed, AND the real Slice 6.5 actor body that turns `slab.document.{onOpen,onClose,getActive}` from "throws when called outside enable context" into a fully live, event-driven surface. `Persistent<Function>` callbacks now actually fire on `DocumentOpened`/`DocumentClosed` commands.

**Commits this tick:**
- `db3897b` feat(plugins): PluginRuntimeRegistry — process-global live actor handles (v2.0.0 Slice 6.6)
- `7b73329` feat(plugins/runtime): long-lived actor evaluates plugin + dispatches doc events (v2.0.0 Slice 6.5)

**Slice 6.5 (`7b73329`) — the meaty one (732 insertions, 99 deletions in 2 files):**
- Replaced placeholder `run_actor` with full Runtime+Context worker. `slab.document.*` is *live*.
- Init handshake via `sync_channel(1)`: spawn blocks until eval completes; syntax/throw/time/memory errors propagate to the host the same way `Runtime::enable_plugin` already does.
- Event loop dispatches each `Persistent<Function>` callback inside fresh `ctx.with` with a fresh interrupt deadline per batch.
- `active_doc` set BEFORE OnOpen dispatch, cleared BEFORE OnClose dispatch — handlers observe the doc that just opened / "no doc" intuitively.
- **Drop order strictly enforced** on both happy and error paths: `lifecycle.clear()` → `drop(ctx)` → `drop(rt)`. No rquickjs aborts.
- `ActorSharedState` only carries Send-safe state (registrations + logs). `SharedLifecycle`/`SharedActiveDoc` are worker-thread-local because `Persistent` wraps `*mut JSRuntime` (`!Send`).
- Per-callback try/catch in `dispatch_lifecycle` — one buggy handler doesn't poison the batch. Logged via `eprintln!`.
- Snapshot `Vec<Persistent>` under the lock then call — avoids reentrancy deadlock when a callback re-registers via `slab.document.on*`.
- `WorkerHandle::shared_state()` exposes `Arc<ActorSharedState>` for Slice 6.7 commands.
- 9 new contract tests cover: onOpen/onClose dispatch + payload, registration order, error isolation, getActive() inside handlers (both axes), Persistent shutdown safety, syntax/throw propagation, top-level log capture.

**Slice 6.6 (`db3897b`) — registry finalize:**
- `PluginRuntimeRegistry` `Mutex<HashMap<String, LiveEntry>>` with `insert` (replaces and Drop-shuts-down old handle), `remove`, `broadcast` (best-effort fan-out), `live_plugin_ids`, `len`/`is_empty`. 8 tests.
- `.manage(plugins::PluginRuntimeRegistry::default())` wired in `lib.rs::run()`.

**Quality gates on `feature/v2.0.0-workshop` HEAD `7b73329`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean (added one targeted `#[allow]` on `lifecycle::new_shared` with doc explaining intra-thread refcounting)
- `cargo test --lib` — **843 passed / 0 failed** (834 before 6.5; +9 actor contract tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 21:46 PT — MODE C v2.0.0 Slice 6 plan + Slice 6.1 scaffolding

Slice 6 (`slab.document.{onOpen,onClose,getActive}` event dispatch) is the trickiest piece in the Workshop arc because rquickjs runtimes aren't `Send` across `Context::with`, so dispatch needs a per-plugin actor thread. Spent this tick writing the ship-ready implementation plan (10 sub-tasks, ~1030 LOC est, full drop-order safety notes), and landing the first sub-task as scaffolding so future ticks have a stable starting point.

**Commits this tick:**
- `17d2cc1` docs(plans): v2.0.0 Slice 6 implementation plan — document lifecycle events
- `5941add` feat(plugins/runtime): RuntimeCmd + DocumentEvent actor types (v2.0.0 Slice 6.1)

**Slice 6 plan (`docs/plans/2026-05-18-v2.0.0-workshop-slice-6.md`, 1059 lines):**
- Architecture: per-plugin actor thread owns a long-lived `rquickjs::Runtime` + `Context`. Host sends `RuntimeCmd` over `crossbeam-channel`. `Persistent<Function>` is `Send + 'static` but the runtime is pinned to its spawn thread — that's why actors, not a global runtime.
- 10 sub-tasks: 6.1 types ✅, 6.2 PluginActor skeleton, 6.3 JS-side `onOpen/onClose` (stores `Persistent`), 6.4 `getActive()` reads shared `ActiveDoc`, 6.5 real `run_actor` body (init handshake + event loop + drop-order), 6.6 `PluginRuntimeRegistry` Tauri state, 6.7 Tauri commands, 6.8 frontend integration in `+page.svelte`, 6.9 10+ contract tests, 6.10 quality gates.
- Critical invariant called out explicitly: **`Persistent` handles MUST be cleared before the owning `Runtime` drops** or rquickjs aborts. Actor exit path: `lifecycle.clear()` → `drop(ctx)` → `drop(rt)`.

**Slice 6.1 (`5941add`) — scaffolding:**
- New `src-tauri/src/plugins/runtime/actor.rs` (141 LOC): `RuntimeCmd { DocumentOpened, DocumentClosed, Shutdown }`, `DocumentEvent { path, name }` with `from_path()` deriving name from `file_stem()`.
- 7 unit tests cover the stem-derivation edge cases (no extension, multi-dot like `archive.tar.gz` → `archive.tar`, dotfile `.bashrc` → `.bashrc`, root path → empty), `matches!(Shutdown)`, and a compile-time `Send + Clone + 'static` assertion. All pass.

**Quality gates on HEAD `5941add`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **812 passed / 0 failed** (805 prior + 7 new actor tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Why a plan tick instead of full Slice 6:** Slice 6 touches lifetimes (`Persistent<Function<'static>>`), threading (one actor thread per plugin), and unsafe-adjacent drop ordering (`Persistent` outliving the runtime aborts the process). Writing 600+ LOC of that without a written design first would mean revising it in-flight. Plan-first means future ticks can dispatch focused sub-task subagents with zero ambiguity.

**Push:** in progress (see below).

---

Big vertical slice: typed manifest surface + standalone modal component + parent panel wiring + permission management actions — 3 commits, ~850 LOC net.

**Commits this tick:**
- `407e2c3` feat(plugins/ts): Manifest.runtime + ManifestCapabilities + consent i18n (v2.0.0 Slice 5a)
- `7b09f0f` feat(plugins/ui): PluginConsentModal component (v2.0.0 Slice 5b)
- `ad1d0d0` feat(plugins/ui): wire consent modal into enable flow + permission actions (v2.0.0 Slice 5c)

**Slice 5a (`407e2c3`) — typing + i18n prep:**
- `src/lib/plugins.ts`: added `Manifest.runtime: RuntimeManifest | null` mirror, plus new `RuntimeManifest` (entry/sha256/capabilities) and `ManifestCapabilities` (fs/net/ui/beacon + two allow-lists) interfaces. `ManifestCapabilities` is kept distinct from `PluginGrants` so the modal can enforce "user can dial down, not up" by comparing the two.
- `src/lib/i18n/en.json`: 36 new strings under `plugins.consent.*` + `plugins.permissions.*`. Other locales fall back to en.

**Slice 5b (`7b09f0f`) — modal component (510 lines):**
- New `src/lib/components/PluginConsentModal.svelte`. Pure presentational; no Tauri knowledge.
- Header: 🔐 icon + "Permissions for <name>" + default-deny subtitle.
- Per-cap rows (fs/net/ui/beacon) — render only when declared bound is non-`none`. Segmented radio with values from `none` up to declared max. 11px declared-hint shows what the plugin asked for.
- Allow-lists (fs paths, net hosts) surfaced read-only when the axis is non-`none`. Collapsed when set to `none`. Editing them lands in a follow-up slice.
- Helper `allowedValues<T>(order, max)` computes per-axis lattice prefix. `as const` tuples so the lattice is type-checked.
- On approve: scrubs allow-lists when axis is `none` (keeps grants file tidy).
- Esc + backdrop = Deny. Approve focused on mount.
- `noRuntime` short-circuit branch for declarative-only plugins (defensive — parent skips entirely for those).

**Slice 5c (`ad1d0d0`) — panel wiring (264 LOC added):**
- `toggleEnabled(p, true)` gates on `getPluginGrants(id).has_decision === false` for `[runtime]` plugins. First-enable → consent modal pre-fills with manifest's declared bounds (max-useful default).
- `ConsentModalState` carries plugin + optional `initial` grants + optional `onResolve` callback. Two flows: first-enable (initial=null, onResolve resumes enable) vs re-review (initial=current grants, onResolve=null).
- Approve: `setPluginGrants(id, grants)` + success toast + resume enable if pending.
- Deny: first-enable path persists `emptyPluginGrants()` (so we don't re-prompt) + info toast. Re-review path just closes without writing.
- Contrib drilldown gets a new "Permissions" row (only when `manifest.runtime !== null`) with "Review permissions" + "Reset permissions" link-buttons. Reset → `resetPluginGrants(id)` + toast explaining next-enable will re-prompt.
- `.permissions-row` CSS with dashed top-border for visual separation.

**Quality gates on `feature/v2.0.0-workshop` HEAD `ad1d0d0`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **805 passed / 0 failed** (unchanged from Slice 4 — Slice 5 is frontend-only)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged in count + identity)

**Push:** in progress (see below).

---

## NEXT TICK PLAYBOOK

### Step 1 — MODE C continue v2.0.0 "Workshop" Slice 9 (SDK npm package)

Slice 8 DONE. Slice 9 ships a typed npm SDK so plugin authors get
IntelliSense on the `slab` global without copying types around. The
workshop master plan is at `docs/plans/2026-05-18-v2.0.0-workshop.md`.

- Pick directory shape: `sdk/slab-plugin-sdk/` in repo root (matches
  the marketplace-seed pattern).
- Build the `.d.ts` from the live TS types in `src/lib/plugins.ts` —
  no manual maintenance.
- Author + ship to npm as `@slab/plugin-sdk` (Sanjay needs to own the
  npm org first; flag this if blocked).

### Step 2 — Or skip ahead to Slice 10 (sample plugin) for faster demo

A real sample plugin exercising `slab.storage` + `slab.fetch` + the
document lifecycle is the highest-signal demo for v2.0.0. Use this if
the npm namespace is blocked.

### Step 3 — Watch for sibling subagent activity

Sibling subagents can touch `/tmp/msg.txt`. Always overwrite right
before commit. Sibling subagents also can `git checkout main`
unexpectedly — verify branch at start of tick.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **RELEASED 2026-05-18** (6 assets on GH)
### v1.9.3 "Voice Mode: Windows-native" — Windows WASAPI recorder via cpal (T6 from v1.9.2 plan, plus real impl)
### v2.0.0 "Workshop" — TypeScript Plugins (rquickjs). **In flight on `feature/v2.0.0-workshop`. Slices 1-8 done (8/12). Plan at `docs/plans/2026-05-18-v2.0.0-workshop.md`; Slice 8 sub-plan at `docs/plans/2026-05-19-v2.0.0-workshop-slice-8.md`.**

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
- ✅ Slice 5 (Cabinet consent modal + enable integration) — shipped 2026-05-18
- ✅ Slice 6 (document lifecycle events) — **COMPLETE 2026-05-18** (6.1-6.6 actor system, 6.7 Tauri commands + enable-flow spawn/teardown, 6.8 ReaderPanel hook)
- ✅ Slice 7 (`slab.fetch` shim — host-mediated HTTP) — **COMPLETE 2026-05-18** (7.1-7.4: process-shared reqwest client + Promise-bridging actor + JS binding + E2E)
- ✅ Slice 8 (`slab.storage.*` — per-plugin KV) — **COMPLETE 2026-05-19** (8.1 module+schema, 8.2-8.3 kv CRUD + quotas + 16 unit tests, 8.4 HostBindings plumbing + spawn_inner test seam, 8.5 6 JS bindings, 8.6 7 E2E tests + actor microtask pump fix, 8.7 sentinel flip)
- ⏭ Slices 9-12 — see plan doc

Slices in target order: 1→rquickjs+console ✅, 2→manifest schema ✅, 3→capability backend ✅, 4→`slab` global + lifecycle ✅, 5→Cabinet consent modal ✅, 6→event dispatch ✅, 7→fetch shim ✅, 8→storage ✅, 9→SDK npm pkg, 10→sample plugin+docs, 11→AI provider registration, 12→release.

**v2.1.0 candidates (post-Workshop):**
- **Forge** — author-signed plugins. Wants 10+ plugins in curated index before considering (Sanjay's flag).
- **Slab CLI** — `slab plugin install <url>`.
- **Plugin author cookbook** — recipes for common plugin patterns.

**Parked items (pre-existing):**
- `docs/screenshots-v1.3.1/` working copy in repo root — harmless, can `rm -rf` someday.
- CommandPalette DETACHABLE_PANELS drift — missing citations/study/glossary entries pre-existed v1.9.0; voice was added but the other three remain. Quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins` GH repo, drop seed files from `docs/marketplace-seed/`, sign the hello-slab plugin, post first real `index.json`.
