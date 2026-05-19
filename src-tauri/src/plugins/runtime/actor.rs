//! Workshop (v2.0.0 Slice 6) — per-plugin actor message protocol and worker.
//!
//! This module is the long-lived JS execution surface for a single enabled
//! plugin. Each [`PluginActor::spawn`] call creates a dedicated OS thread
//! that owns a `rquickjs::Runtime` + `Context` for the lifetime of the
//! plugin and processes [`RuntimeCmd`]s off a `crossbeam-channel`.
//!
//! ## Threading model
//!
//! `rquickjs` requires that *all* JS execution for a given `Runtime` happen
//! on a single OS thread — the `Context::with` callback borrows the
//! runtime non-`Send`. We therefore park the runtime on a dedicated
//! worker thread and use channels to ferry commands in.
//!
//! `Persistent<Function>` is `Send + 'static` (a `Persistent` is just an
//! owned QuickJS refcount), so callbacks registered during top-level
//! `slab.document.onOpen(...)` survive across command dispatches without
//! the host having to know which thread they live on.
//!
//! ## Drop order (CRITICAL — see also `lifecycle.rs`)
//!
//! `rquickjs::Runtime::drop` calls `abort()` if any `Persistent` is still
//! live. The worker's shutdown path enforces:
//!
//! ```text
//! shared.lifecycle.lock().clear();   // decrement all refcounts
//! drop(ctx);                         // drop context
//! drop(rt);                          // safe — registry empty
//! ```
//!
//! This applies to *both* the happy path (Shutdown received) and the
//! init-error path (eval threw before the actor entered its loop).
//!
//! ## Init handshake
//!
//! [`PluginActor::spawn`] is synchronous from the caller's POV: it
//! blocks until top-level evaluation has either succeeded (registrations
//! recorded into `shared.registrations`) or failed (error propagated as
//! `RuntimeError`). This lets cabinet enable flow show errors inline
//! the same way the legacy `Runtime::enable_plugin` did.
//!
//! See `docs/plans/2026-05-18-v2.0.0-workshop-slice-6.md` for the full
//! implementation arc.

use std::path::PathBuf;
use std::sync::{mpsc::sync_channel, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crossbeam_channel::{unbounded, Sender};
use rquickjs::{CatchResultExt, Context, Object, Value};

use super::lifecycle::{new_shared, new_shared_active_doc, SharedActiveDoc, SharedLifecycle};
use super::slab_global::{install_slab, HostBindings};
use super::{
    classify_error, host_api::Registrations, sandbox, LogEntry, RuntimeError, MEMORY_LIMIT_BYTES,
    WALL_CLOCK_LIMIT,
};
use crate::plugins::grants::PluginGrants;
use crate::plugins::manifest::Capabilities;

/// Commands the host sends to a plugin worker thread.
///
/// The variants intentionally carry small owned payloads (no
/// borrowed refs, no `&Ctx`), so they're `Send + 'static` and can
/// cross a `crossbeam-channel` boundary cleanly.
///
/// In particular, `Fetch` does NOT carry the `Persistent` resolve/
/// reject callbacks directly — `Persistent` is `!Send` because it
/// holds a raw `*mut JSRuntime`. Instead, the JS-side binding (which
/// runs ON the worker thread inside `Context::with`) stashes the
/// callbacks into a thread-local pending-fetch map keyed by
/// `request_id`, then sends `RuntimeCmd::Fetch { request_id,
/// request }`. The recv-loop pops the callbacks back out when it
/// processes the command, all on the same thread, no cross-thread
/// movement of `!Send` types.
#[derive(Debug, Clone)]
pub enum RuntimeCmd {
    /// A PDF was loaded into the viewer. The worker will invoke every
    /// `Persistent<Function>` stored in its lifecycle registry under
    /// the `onOpen` axis, passing a JS `{ path, name }` object.
    DocumentOpened(DocumentEvent),
    /// The active PDF was closed (or replaced — the frontend emits a
    /// close for the previous doc followed by an open for the new one).
    DocumentClosed(DocumentEvent),
    /// Host-mediated HTTP request from a plugin's `slab.fetch` call.
    /// The recv loop runs the request synchronously (`block_on` on
    /// the Tauri tokio handle), then looks up the stashed
    /// `Persistent` resolve/reject pair in the worker's pending-fetch
    /// map (see `runtime::fetch::PendingFetch`) and invokes whichever
    /// matches the result.
    Fetch {
        request_id: u64,
        request: FetchRequest,
    },
    /// Drain any pending events, clear all Persistents from the
    /// lifecycle registry, then exit the worker thread cleanly.
    ///
    /// This is the only safe way to terminate a worker: dropping the
    /// runtime with live Persistents triggers a rquickjs abort.
    Shutdown,
}

/// Payload for both `DocumentOpened` and `DocumentClosed`. Mirrors the
/// shape that surfaces to plugin authors via the `event` argument of
/// `slab.document.onOpen((event) => ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentEvent {
    /// Absolute path of the PDF on disk. We keep this as a
    /// [`PathBuf`] rather than a `String` so future slices can pass
    /// it to filesystem APIs without re-parsing.
    pub path: PathBuf,
    /// Display name — `file_stem()` of `path`, lossy-stringified.
    /// Plugin authors prefer the human-friendly form over the full
    /// path; the path is still available for plugins that need it.
    pub name: String,
}

impl DocumentEvent {
    /// Build a `DocumentEvent` from any path-like input. Computes
    /// `name` as the file stem (filename minus extension); empty
    /// string when the path has no file component (e.g. `/`).
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Self { path, name }
    }
}

/// Outbound HTTP request built by the JS-side `slab.fetch` host
/// binding and consumed by the actor's recv loop.
///
/// All fields are owned `Send + 'static` values so the payload
/// crosses the crossbeam-channel cleanly without borrows. Headers
/// are kept lowercase by convention (the JS binding lowercases them
/// at parse time); body bytes are buffered (Slice 7 does not stream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    /// Uppercase HTTP method (e.g. `"GET"`, `"POST"`). The binding
    /// normalises whatever the plugin passed via `init.method`.
    pub method: String,
    /// Absolute URL string as supplied by the plugin. The host MUST
    /// re-parse this before sending (the JS binding has already
    /// validated it, but trust-but-verify keeps the recv loop honest).
    pub url: String,
    /// `(name, value)` pairs. Names are lowercase ASCII; values are
    /// passed through verbatim. The recv loop converts these into a
    /// `reqwest::header::HeaderMap` before dispatch.
    pub headers: Vec<(String, String)>,
    /// Body bytes. `None` for the default GET/HEAD case; `Some(vec![])`
    /// for an explicit empty body on POST/PUT etc. The 16 MiB cap on
    /// outbound bodies is enforced at JS-binding parse time (Slice 7.4).
    pub body: Option<Vec<u8>>,
    /// Hard timeout in milliseconds. Defaults to 30_000 when the JS
    /// caller omits `init.timeoutMs`; clamped to `[1, 120_000]` by
    /// the binding so a hostile plugin can't park the actor forever.
    pub timeout_ms: u64,
}

/// Response payload returned from the host's `reqwest` call back to
/// the JS Promise resolver.
///
/// Shape mirrors the minimum surface of the web Fetch `Response`
/// that plugin authors expect: status code + canonical reason +
/// final URL (after redirects) + headers + body bytes + `ok` flag.
/// Conversion to JS lives in the actor recv loop (Slice 7.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    /// HTTP status code (e.g. 200, 404, 500). Caller surfaces this
    /// as `resp.status` in JS land.
    pub status: u16,
    /// Canonical reason phrase (e.g. `"OK"`, `"Not Found"`). May be
    /// empty for unknown statuses.
    pub status_text: String,
    /// Final URL after redirect following — `reqwest::Response::url()`.
    /// JS plugins use this to detect whether they were redirected.
    pub url: String,
    /// Lowercased response header pairs. Multi-value headers are
    /// joined with `, ` per RFC 7230.
    pub headers: Vec<(String, String)>,
    /// Body bytes. Capped at [`crate::plugins::runtime::fetch::MAX_BODY_BYTES`]
    /// (16 MiB) — oversized responses surface as a [`String`] error
    /// from `do_fetch`, never reach this struct.
    pub body: Vec<u8>,
    /// True when `status` is in `200..=299`. Matches the web Fetch
    /// `Response.ok` semantics — `slab.fetch` does NOT reject on
    /// 4xx/5xx, it resolves with `ok = false`.
    pub ok: bool,
}

/// Shared state owned by the actor's worker thread and visible to the
/// host via [`WorkerHandle::shared_state`].
///
/// We intentionally only expose host-side-readable state here:
///
/// - `registrations` — every `slab.{beacon,ui}.*` registration the
///   plugin made during top-level eval. Host code (Slice 7's
///   actor-driven enable flow) reads this once after `spawn` returns
///   to wire UI panels, beacon tools, etc.
/// - `logs` — captured `console.*` output from the *top-level eval*
///   only. Subsequent event-dispatch logs are intentionally NOT
///   captured here (plugins do their own logging); we keep startup
///   logs around because they're what the cabinet shows in the
///   "plugin enabled" toast.
///
/// The `SharedLifecycle` and `SharedActiveDoc` handles needed by
/// `slab.document.{onOpen,onClose,getActive}` live worker-thread-local
/// inside [`run_actor`]: `rquickjs::Persistent` carries a raw
/// `*mut JSRuntime` which is `!Send`, so the `SharedLifecycle` itself
/// cannot cross thread boundaries. The host has no need to read either
/// directly anyway — both are exposed to plugin code via the `slab`
/// global within the same worker thread.
pub struct ActorSharedState {
    pub registrations: Arc<Mutex<Registrations>>,
    pub logs: Arc<Mutex<Vec<LogEntry>>>,
}

impl Default for ActorSharedState {
    fn default() -> Self {
        Self {
            registrations: Arc::new(Mutex::new(Registrations::default())),
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// Handle to a running plugin actor.
///
/// Owning this means the worker thread is alive (or just exited and
/// awaiting join). Drop performs a best-effort `Shutdown` send + join
/// so leaking a handle never leaks an OS thread.
///
/// Slice 6.5 wires this to the real `Runtime` + `Context` loop:
/// callbacks registered via `slab.document.on*` are now invoked on
/// `DocumentOpened`/`DocumentClosed`.
pub struct WorkerHandle {
    plugin_id: String,
    tx: Sender<RuntimeCmd>,
    join: Option<JoinHandle<()>>,
    shared: Arc<ActorSharedState>,
}

impl std::fmt::Debug for WorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerHandle")
            .field("plugin_id", &self.plugin_id)
            .field("joined", &self.join.is_none())
            .finish()
    }
}

impl WorkerHandle {
    /// Plugin identifier this worker was spawned for. Used by the
    /// registry (Slice 6.6) for keying and by diagnostics.
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Send a command to the worker. Returns the underlying
    /// `crossbeam_channel::SendError` if the worker has already exited
    /// (channel disconnected). Callers should treat that as
    /// "actor is gone — drop the handle" rather than retrying.
    pub fn send(&self, cmd: RuntimeCmd) -> Result<(), crossbeam_channel::SendError<RuntimeCmd>> {
        self.tx.send(cmd)
    }

    /// Clone of the shared state owned by this actor. Cheap (one
    /// `Arc` bump). Callers typically hold this across `join()` so
    /// they can inspect `registrations` / `logs` after the worker
    /// has fully exited.
    pub fn shared_state(&self) -> Arc<ActorSharedState> {
        Arc::clone(&self.shared)
    }

    /// Cleanly shut the worker down and wait for the thread to exit.
    /// Idempotent: a `Shutdown` from a previous `send` is harmless
    /// because the worker's `recv` will already have returned `Err`.
    pub fn join(mut self) -> thread::Result<()> {
        // Best-effort: if the worker already exited the channel is
        // disconnected and `send` errors — that's fine, we just want
        // the thread joined.
        let _ = self.tx.send(RuntimeCmd::Shutdown);
        if let Some(j) = self.join.take() {
            return j.join();
        }
        Ok(())
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        // Best-effort shutdown so a forgotten handle never leaks a
        // thread. Ignore both send + join errors: the worker may have
        // exited on its own (e.g. a panic inside `run_actor`), in which
        // case `tx.send` errors and the join already happened.
        let _ = self.tx.send(RuntimeCmd::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Per-plugin actor: spawns a dedicated OS thread that owns the
/// plugin's QuickJS runtime and processes [`RuntimeCmd`]s off a
/// channel.
///
/// In Slice 6.5 (this) the worker:
/// 1. Creates a fresh `rquickjs::Runtime` with Slab's standard limits.
/// 2. Creates one long-lived `Context::full(&rt)`.
/// 3. Installs the sandbox (`console.*`) and the `slab.*` global
///    (with `lifecycle`/`active_doc` wired to live shared state).
/// 4. Evaluates the plugin's top-level source exactly once.
/// 5. Reports back to the spawner via a sync init channel.
/// 6. Enters a `recv` loop dispatching `DocumentOpened`/`DocumentClosed`
///    into the registered `Persistent<Function>` callbacks.
/// 7. On `Shutdown`, clears the lifecycle registry **before** dropping
///    the runtime and exits cleanly.
pub struct PluginActor;

impl PluginActor {
    /// Spawn the actor's worker thread and wait for top-level eval to
    /// complete. The returned [`WorkerHandle`] is live and ready to
    /// receive [`RuntimeCmd`]s; `shared_state().registrations` carries
    /// every `slab.{beacon,ui}.*` registration the plugin made during
    /// eval.
    ///
    /// # Errors
    /// - [`RuntimeError::Init`] if the OS refuses to spawn the thread,
    ///   the QuickJS runtime / context can't be created, or the
    ///   internal init channel closes unexpectedly.
    /// - [`RuntimeError::Syntax`] / [`RuntimeError::Thrown`] /
    ///   [`RuntimeError::TimeLimit`] / [`RuntimeError::MemoryLimit`] if
    ///   the plugin source fails to evaluate at top level.
    ///
    /// On any error, the worker thread tears itself down cleanly
    /// (clears Persistents, drops the runtime, exits) before this
    /// call returns the `Err`.
    pub fn spawn(
        plugin_id: String,
        declared: Capabilities,
        granted: PluginGrants,
        source: String,
    ) -> Result<WorkerHandle, RuntimeError> {
        let (tx, rx) = unbounded::<RuntimeCmd>();
        let (init_tx, init_rx) = sync_channel::<Result<(), RuntimeError>>(1);

        let shared = Arc::new(ActorSharedState::default());
        let shared_for_worker = Arc::clone(&shared);
        let pid_for_handle = plugin_id.clone();

        let join = thread::Builder::new()
            // Visible in `ps -T` / Activity Monitor — makes debugging
            // a stuck plugin trivial: `slab-plugin:com.x.y` is the
            // thread name, plugin_id is the ID in the manifest.
            .name(format!("slab-plugin:{plugin_id}"))
            .spawn(move || {
                run_actor(
                    plugin_id,
                    declared,
                    granted,
                    source,
                    rx,
                    init_tx,
                    shared_for_worker,
                );
            })
            .map_err(|e| RuntimeError::Init(format!("actor thread spawn: {e}")))?;

        // Block until the worker reports eval result. The worker only
        // sends once; recv-closed means it panicked before sending,
        // which we surface as a generic Init error.
        let init = init_rx
            .recv()
            .map_err(|_| RuntimeError::Init("actor init channel closed".into()))?;

        match init {
            Ok(()) => Ok(WorkerHandle {
                plugin_id: pid_for_handle,
                tx,
                join: Some(join),
                shared,
            }),
            Err(e) => {
                // Worker has already torn itself down on the error
                // path; we still join the thread so its OS handle is
                // released before we return.
                let _ = join.join();
                Err(e)
            }
        }
    }
}

/// Worker body: owns the runtime, evaluates the plugin, then dispatches
/// events until [`RuntimeCmd::Shutdown`].
///
/// The function is single-entry / single-exit on purpose so the
/// "clear lifecycle before runtime drops" invariant (see module docs)
/// is mechanically enforced in both the happy and error paths.
#[allow(clippy::too_many_arguments)]
fn run_actor(
    plugin_id: String,
    declared: Capabilities,
    granted: PluginGrants,
    source: String,
    rx: crossbeam_channel::Receiver<RuntimeCmd>,
    init_tx: std::sync::mpsc::SyncSender<Result<(), RuntimeError>>,
    shared: Arc<ActorSharedState>,
) {
    // -- Boot: build runtime + context ---------------------------------------
    let rt = match rquickjs::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            let _ = init_tx.send(Err(RuntimeError::Init(format!("{e}"))));
            return;
        }
    };
    rt.set_memory_limit(MEMORY_LIMIT_BYTES);

    let ctx = match Context::full(&rt) {
        Ok(c) => c,
        Err(e) => {
            let _ = init_tx.send(Err(RuntimeError::Init(format!("context: {e}"))));
            return;
        }
    };

    // Lifecycle + active-doc handles are constructed *inside* the
    // worker thread because `rquickjs::Persistent` carries a raw
    // `*mut JSRuntime` (`!Send`), so `SharedLifecycle` cannot cross
    // threads. Plugin code interacts with both via `slab.document.*`
    // bindings, which run on this same worker.
    let lifecycle: SharedLifecycle = new_shared();
    let active_doc: SharedActiveDoc = new_shared_active_doc();
    // Worker-local pending-fetch table. `slab.fetch` enqueues
    // (resolve, reject) pairs here keyed by a monotonic request id,
    // then sends `RuntimeCmd::Fetch { request_id, request }` over
    // the channel. The recv loop pops the pair out by id and
    // settles the Promise.
    //
    // Same `!Send` story as `lifecycle`: this `Arc` never crosses
    // a thread boundary; the worker thread is the only one touching
    // it. The `Arc<Mutex<_>>` shape is purely so the JS-side
    // `Function::new` closure can capture a clone.
    let pending_fetches: super::fetch::SharedPendingFetches = super::fetch::new_shared_pending();

    // -- Top-level eval ------------------------------------------------------
    // Wall-clock guard for the eval phase. The interrupt closure
    // captures `deadline` by value; we clear the handler when eval
    // returns so subsequent dispatches don't inherit a stale deadline.
    let eval_deadline = Instant::now() + WALL_CLOCK_LIMIT;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= eval_deadline)));

    let init_result = ctx.with(|ctx| -> Result<(), RuntimeError> {
        sandbox::install_console(&ctx, plugin_id.clone(), Arc::clone(&shared.logs))
            .map_err(|e| RuntimeError::Init(format!("console install: {e}")))?;

        let bindings = HostBindings {
            plugin_id: plugin_id.clone(),
            declared: Arc::new(declared.clone()),
            granted: Arc::new(granted.clone()),
            registrations: Arc::clone(&shared.registrations),
            // Live wiring — Persistents from slab.document.on* will
            // survive across event dispatches via these handles.
            lifecycle: Some(Arc::clone(&lifecycle)),
            active_doc: Some(Arc::clone(&active_doc)),
        };
        install_slab(&ctx, bindings)
            .map_err(|e| RuntimeError::Init(format!("slab global install: {e}")))?;

        match ctx.eval::<Value, _>(source.as_bytes()).catch(&ctx) {
            Ok(_) => Ok(()),
            Err(caught) => Err(classify_error(caught)),
        }
    });

    rt.set_interrupt_handler(None);

    if let Err(e) = init_result {
        // Eval threw (or install failed). Some `slab.document.on*`
        // calls may have stashed Persistents before the throw, so we
        // MUST clear them before runtime drops.
        if let Ok(mut g) = lifecycle.lock() {
            g.clear();
        }
        drop(ctx);
        drop(rt);
        let _ = init_tx.send(Err(e));
        return;
    }

    // Eval succeeded; unblock spawn() so the host can start sending
    // events. The order matters: signal *after* registrations are
    // fully populated so callers reading shared_state() see them.
    let _ = init_tx.send(Ok(()));

    // -- Event loop ----------------------------------------------------------
    while let Ok(cmd) = rx.recv() {
        match cmd {
            RuntimeCmd::DocumentOpened(ev) => {
                if let Ok(mut g) = active_doc.lock() {
                    *g = Some(ev.clone());
                }
                dispatch_lifecycle(&rt, &ctx, &lifecycle, LifecycleAxis::OnOpen, &ev);
            }
            RuntimeCmd::DocumentClosed(ev) => {
                // Clear the active-doc snapshot BEFORE invoking the
                // onClose callbacks so `slab.document.getActive()`
                // inside a handler observes `null`. This mirrors what
                // a plugin author intuitively expects ("the doc is
                // gone — getActive() should reflect that").
                if let Ok(mut g) = active_doc.lock() {
                    *g = None;
                }
                dispatch_lifecycle(&rt, &ctx, &lifecycle, LifecycleAxis::OnClose, &ev);
            }
            RuntimeCmd::Fetch {
                request_id,
                request,
            } => {
                let callbacks = super::fetch::take_pending(&pending_fetches, request_id);
                dispatch_fetch(&rt, &ctx, request, callbacks);
            }
            RuntimeCmd::Shutdown => break,
        }
    }

    // -- Clean shutdown ------------------------------------------------------
    // Must clear all Persistents BEFORE the runtime drops; otherwise
    // rquickjs aborts the process. See module docs.
    if let Ok(mut g) = lifecycle.lock() {
        g.clear();
    }
    drop(ctx);
    drop(rt);
}

/// Internal axis enum mirroring the one in `slab_global.rs` — kept
/// private so `actor.rs` doesn't leak this distinction to outside
/// callers. (The plan's "single shared axis enum" refactor lives in
/// post-1.0; for now duplication is cheaper than coupling.)
#[derive(Clone, Copy)]
enum LifecycleAxis {
    OnOpen,
    OnClose,
}

/// Invoke every `Persistent<Function>` in `lifecycle`'s slot for `axis`
/// with a freshly-built `{ path, name }` event object.
///
/// Errors from individual callbacks are logged via `eprintln!` and
/// otherwise suppressed — one buggy handler must not poison the rest
/// of the registry or take down the actor thread. Same for restore
/// failures (which would indicate cross-runtime contamination — a
/// host bug, not a plugin bug).
///
/// The lifecycle mutex is held only long enough to snapshot the slot
/// into a local `Vec<Persistent>`. That avoids reentrancy deadlocks
/// when a callback calls `slab.document.onOpen(...)` while running
/// (which would try to re-acquire the same lock).
fn dispatch_lifecycle(
    rt: &rquickjs::Runtime,
    ctx: &Context,
    lifecycle: &SharedLifecycle,
    axis: LifecycleAxis,
    ev: &DocumentEvent,
) {
    // Snapshot the callback list under the lock, then drop the lock
    // before we enter `ctx.with` — callbacks are free to call back
    // into `slab.document.onOpen` without deadlocking.
    let snapshot = {
        let Ok(guard) = lifecycle.lock() else {
            // Mutex poisoned. Skip dispatch — host is in an
            // inconsistent state anyway and there's nothing useful
            // we can do from inside the worker.
            return;
        };
        match axis {
            LifecycleAxis::OnOpen => guard.on_open().to_vec(),
            LifecycleAxis::OnClose => guard.on_close().to_vec(),
        }
    };
    if snapshot.is_empty() {
        // Fast path: no handlers registered. Avoids paying for
        // ctx.with + event-object construction.
        return;
    }

    // Wall-clock guard for the dispatch batch. If the cumulative
    // runtime of all handlers (plus any reentrant slab.* calls they
    // make) exceeds WALL_CLOCK_LIMIT, an `interrupted` exception
    // propagates up into the catch arm below and we stop dispatching
    // further handlers for *this* event. Subsequent events get a
    // fresh deadline.
    let deadline = Instant::now() + WALL_CLOCK_LIMIT;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

    ctx.with(|ctx| {
        // Build the `{ path, name }` event object once per dispatch
        // batch and clone it into each handler call. Cheaper than
        // re-creating per handler for non-trivial registries.
        let event_obj = match Object::new(ctx.clone()) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[plugin lifecycle] cannot build event obj: {e}");
                return;
            }
        };
        if let Err(e) = event_obj.set("path", ev.path.to_string_lossy().to_string()) {
            eprintln!("[plugin lifecycle] event obj.path set failed: {e}");
            return;
        }
        if let Err(e) = event_obj.set("name", ev.name.clone()) {
            eprintln!("[plugin lifecycle] event obj.name set failed: {e}");
            return;
        }

        for persistent in snapshot {
            match persistent.restore(&ctx) {
                Ok(f) => {
                    // Per-callback catch — one buggy handler must not
                    // poison the rest of the dispatch batch.
                    if let Err(e) = f.call::<_, ()>((event_obj.clone(),)).catch(&ctx) {
                        // `CaughtError`'s Display gives us a useful
                        // single-line summary including the JS message
                        // and (sometimes) the stack frame.
                        eprintln!("[plugin lifecycle] callback threw: {e}");
                    }
                }
                Err(e) => {
                    // The only way restore fails is `UnrelatedRuntime`
                    // — a Persistent got smuggled in from a different
                    // runtime. That's a host bug; log loudly.
                    eprintln!("[plugin lifecycle] restore failed: {e}");
                }
            }
        }
    });

    rt.set_interrupt_handler(None);
}

/// Run a `RuntimeCmd::Fetch` against the shared `reqwest::Client`
/// (Slice 7.2's `do_fetch`), then settle the JS Promise by invoking
/// the supplied `Persistent<Function>` resolve/reject callbacks.
///
/// Blocks the actor thread for the duration of the request — that's
/// intentional for Slice 7. QuickJS is single-threaded inside the
/// actor anyway; while a fetch is in flight the plugin's JS is
/// already idle (it's `await`ing the Promise). Concurrent fetches
/// from the same plugin serialise naturally. If profiling later
/// shows this is too restrictive, Slice 7b can promote individual
/// fetches to spawned tokio tasks tracked in a per-actor join-set.
///
/// Error handling philosophy: never panic; one bad fetch must not
/// take down the worker. Network errors, body-too-large, malformed
/// URLs etc. all surface as Promise rejections in JS land. If the
/// Promise plumbing itself fails (resolve/reject restore returns
/// `UnrelatedRuntime`, or the JS-side resolve throws) we log via
/// `eprintln!` and drop the callback — the Promise stays pending
/// forever, which is acceptable for what's almost always a host bug
/// (not a plugin bug).
fn dispatch_fetch(
    rt: &rquickjs::Runtime,
    ctx: &Context,
    request: super::actor::FetchRequest,
    callbacks: Option<super::fetch::PendingCallbacks>,
) {
    // If the pending-fetch map has no entry for this request id
    // (e.g. the entry was evicted, or there's a bug in the
    // JS-side enqueue), drop silently — there's nobody to settle.
    let Some((resolve, reject)) = callbacks else {
        eprintln!("[plugin fetch] no pending callbacks for request — dropping");
        return;
    };

    // Fresh wall-clock guard for this dispatch batch. The
    // interrupt only fires while JS is executing (resolve/reject
    // body etc.), so it doesn't interrupt the network `block_on` —
    // that's bounded separately by `request.timeout_ms` inside
    // `do_fetch`.
    let deadline = Instant::now() + WALL_CLOCK_LIMIT;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

    // Run the HTTP call. If we're inside a Tauri tokio runtime,
    // borrow its handle; otherwise spin up a single-threaded
    // current-thread runtime for this one call. The latter path
    // exists so the actor remains usable from unit tests that
    // don't bring up Tauri.
    let result: Result<super::actor::FetchResponse, String> =
        match tokio::runtime::Handle::try_current() {
            Ok(h) => h.block_on(super::fetch::do_fetch(request)),
            Err(_) => match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt.block_on(super::fetch::do_fetch(request)),
                Err(e) => Err(format!("tokio runtime build: {e}")),
            },
        };

    // Settle the Promise inside the rquickjs context. Both
    // resolve and reject are `Persistent`s owned by us; restoring
    // requires a `Ctx<'_>` so we do it inside a `with` block.
    ctx.with(|ctx| {
        match result {
            Ok(resp) => {
                let resp_val = match super::fetch::response_to_js(&ctx, &resp) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("[plugin fetch] response_to_js failed: {e}");
                        return;
                    }
                };
                match resolve.restore(&ctx) {
                    Ok(f) => {
                        if let Err(e) = f.call::<_, Value>((resp_val,)).catch(&ctx) {
                            eprintln!("[plugin fetch] resolve threw: {e}");
                        }
                    }
                    Err(e) => eprintln!("[plugin fetch] resolve restore failed: {e}"),
                }
            }
            Err(msg) => {
                // Build an Error object so plugin authors get a
                // typed rejection (e.g. `(await fetch(...)).catch(e
                // => e.message)`). `eval` is the simplest way to
                // get an `Error`-instance value without juggling
                // rquickjs's Exception types.
                let err_src = format!("new Error({})", json_string(&msg));
                let err_val = match ctx.eval::<Value, _>(err_src.as_bytes()).catch(&ctx) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("[plugin fetch] error obj build failed: {e}");
                        return;
                    }
                };
                match reject.restore(&ctx) {
                    Ok(f) => {
                        if let Err(e) = f.call::<_, Value>((err_val,)).catch(&ctx) {
                            eprintln!("[plugin fetch] reject threw: {e}");
                        }
                    }
                    Err(e) => eprintln!("[plugin fetch] reject restore failed: {e}"),
                }
            }
        }
    });

    rt.set_interrupt_handler(None);
}

/// Quote `s` as a JS string literal. Used to embed an error message
/// into a `new Error(...)` source string without worrying about
/// embedded quotes/newlines/backslashes. `serde_json::to_string` of
/// a `&str` produces a valid JS string literal too (JSON strings are
/// a subset of JS string literals).
fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"<unprintable>\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_event_from_path_derives_name_from_stem() {
        let e = DocumentEvent::from_path("/tmp/Whitepaper.pdf");
        assert_eq!(e.name, "Whitepaper");
        assert_eq!(e.path, PathBuf::from("/tmp/Whitepaper.pdf"));
    }

    #[test]
    fn document_event_handles_path_without_extension() {
        let e = DocumentEvent::from_path("/tmp/Notes");
        assert_eq!(e.name, "Notes");
    }

    #[test]
    fn document_event_empty_name_for_root_path() {
        let e = DocumentEvent::from_path("/");
        assert_eq!(e.name, "");
    }

    #[test]
    fn document_event_handles_multiple_dots() {
        // file_stem strips only the final extension. `archive.tar.gz`
        // -> "archive.tar". Plugin authors who want the basename
        // sans-anything should split on `.` themselves.
        let e = DocumentEvent::from_path("/tmp/archive.tar.gz");
        assert_eq!(e.name, "archive.tar");
    }

    #[test]
    fn document_event_handles_hidden_dotfile_without_extension() {
        // file_stem on ".bashrc" returns ".bashrc" itself (no
        // extension to strip). This is consistent with std::path
        // semantics; documented here so future maintainers know it's
        // deliberate, not a bug.
        let e = DocumentEvent::from_path("/home/sanjay/.bashrc");
        assert_eq!(e.name, ".bashrc");
    }

    #[test]
    fn runtime_cmd_shutdown_is_match_distinguishable() {
        // Tiny sanity check that the actor loop's `matches!` pattern
        // works as expected; protects against an accidental rename.
        let c = RuntimeCmd::Shutdown;
        assert!(matches!(c, RuntimeCmd::Shutdown));
        let c2 = RuntimeCmd::DocumentOpened(DocumentEvent::from_path("/x.pdf"));
        assert!(!matches!(c2, RuntimeCmd::Shutdown));
    }

    #[test]
    fn runtime_cmd_is_clone_and_send() {
        // Compile-time check via trait bounds: if RuntimeCmd ever
        // accidentally grows a `Rc<...>` field, this will fail to
        // compile.
        fn assert_send_clone<T: Send + Clone + 'static>() {}
        assert_send_clone::<RuntimeCmd>();
        assert_send_clone::<DocumentEvent>();
    }

    // ---- PluginActor contract tests --------------------------------------
    //
    // These exercise the real Slice 6.5 runtime body: spawn evaluates
    // the source, registrations are visible via shared_state, lifecycle
    // callbacks fire on Document{Opened,Closed}, errors are isolated.

    use crate::plugins::grants::PluginGrants;
    use crate::plugins::manifest::{BeaconCap, Capabilities, FsCap, NetCap, UiCap};
    use std::time::{Duration, Instant};

    fn caps_ui_full() -> Capabilities {
        Capabilities {
            fs: FsCap::None,
            net: NetCap::None,
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            net_allow_hosts: vec![],
            fs_allow_paths: vec![],
        }
    }

    fn grants_ui_full() -> PluginGrants {
        PluginGrants {
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            ..PluginGrants::default()
        }
    }

    #[test]
    fn plugin_actor_spawns_and_shuts_down_cleanly() {
        let handle = PluginActor::spawn(
            "p.actor.spawn".into(),
            Capabilities::default(),
            PluginGrants::default(),
            String::new(),
        )
        .expect("actor must spawn");
        assert_eq!(handle.plugin_id(), "p.actor.spawn");
        handle.send(RuntimeCmd::Shutdown).expect("send shutdown");
        handle.join().expect("worker thread joins cleanly");
    }

    #[test]
    fn plugin_actor_drops_into_clean_shutdown_when_handle_forgotten() {
        // Dropping a handle without explicit `Shutdown` still tears
        // the worker down — that's the invariant Drop enforces. We
        // verify by spawning N actors in a tight loop; if Drop were
        // broken, this would explode FD/thread limits.
        for i in 0..8 {
            let h = PluginActor::spawn(
                format!("p.drop.{i}"),
                Capabilities::default(),
                PluginGrants::default(),
                String::new(),
            )
            .expect("spawn");
            // Send one no-op command to prove the channel works,
            // then drop without calling `join`.
            h.send(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
                "/tmp/x.pdf",
            )))
            .expect("send");
            drop(h);
        }
    }

    #[test]
    fn plugin_actor_drains_pending_events_before_shutdown_exits() {
        // Send a burst of events followed by Shutdown; the worker
        // must process all events + Shutdown promptly. With Slice 6.5
        // wired, each event invokes dispatch_lifecycle (no handlers
        // registered → fast-path).
        let handle = PluginActor::spawn(
            "p.actor.drain".into(),
            Capabilities::default(),
            PluginGrants::default(),
            String::new(),
        )
        .expect("spawn");
        for i in 0..16 {
            let ev = DocumentEvent::from_path(format!("/tmp/doc-{i}.pdf"));
            handle.send(RuntimeCmd::DocumentOpened(ev)).expect("send");
        }
        let start = Instant::now();
        handle.send(RuntimeCmd::Shutdown).expect("send shutdown");
        handle.join().expect("join clean");
        // Drain + join should finish well under 1s even with a real
        // Runtime per actor (each dispatch is a fast-path no-op when
        // no handlers are registered).
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "drain+join took {:?}, expected < 2s",
            start.elapsed()
        );
    }

    #[test]
    fn plugin_actor_invokes_onopen_when_document_opened() {
        // Plugin registers an onOpen handler that records the event
        // via slab.ui.notify (which lands in shared.registrations).
        let script = r#"
            slab.document.onOpen(function (ev) {
                slab.ui.notify("opened:" + ev.name, "info");
            });
        "#;
        let h = PluginActor::spawn(
            "p.onopen.basic".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        h.send(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
            "/tmp/Alpha.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");

        let regs = shared.registrations.lock().unwrap();
        let messages: Vec<&str> = regs
            .notifications
            .iter()
            .map(|n| n.message.as_str())
            .collect();
        assert!(
            messages.contains(&"opened:Alpha"),
            "expected onOpen to fire with name 'Alpha', got {messages:?}"
        );
    }

    #[test]
    fn plugin_actor_invokes_onclose_when_document_closed() {
        let script = r#"
            slab.document.onClose(function (ev) {
                slab.ui.notify("closed:" + ev.name, "info");
            });
        "#;
        let h = PluginActor::spawn(
            "p.onclose.basic".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        h.send(RuntimeCmd::DocumentClosed(DocumentEvent::from_path(
            "/tmp/Beta.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");

        let regs = shared.registrations.lock().unwrap();
        let messages: Vec<&str> = regs
            .notifications
            .iter()
            .map(|n| n.message.as_str())
            .collect();
        assert!(
            messages.contains(&"closed:Beta"),
            "expected onClose to fire with name 'Beta', got {messages:?}"
        );
    }

    #[test]
    fn plugin_actor_invokes_multiple_onopen_handlers_in_registration_order() {
        // Plugins can register multiple onOpen handlers — they fire
        // in registration order (addEventListener semantics).
        let script = r#"
            slab.document.onOpen(function () { slab.ui.notify("first", "info"); });
            slab.document.onOpen(function () { slab.ui.notify("second", "info"); });
            slab.document.onOpen(function () { slab.ui.notify("third", "info"); });
        "#;
        let h = PluginActor::spawn(
            "p.onopen.order".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        h.send(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
            "/tmp/x.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");

        let regs = shared.registrations.lock().unwrap();
        let messages: Vec<&str> = regs
            .notifications
            .iter()
            .map(|n| n.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec!["first", "second", "third"],
            "handlers must fire in registration order"
        );
    }

    #[test]
    fn plugin_actor_isolates_callback_errors() {
        // One callback throws; subsequent callbacks must still run,
        // and the worker must not die. The thrown error gets logged
        // via eprintln! (no assertion on stderr here — we just verify
        // the worker stays healthy).
        let script = r#"
            slab.document.onOpen(function () { throw new Error("boom"); });
            slab.document.onOpen(function () { slab.ui.notify("survived", "info"); });
        "#;
        let h = PluginActor::spawn(
            "p.onopen.isolate".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        h.send(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
            "/tmp/x.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");

        let regs = shared.registrations.lock().unwrap();
        let messages: Vec<&str> = regs
            .notifications
            .iter()
            .map(|n| n.message.as_str())
            .collect();
        assert!(
            messages.contains(&"survived"),
            "second handler must run after first threw; got {messages:?}"
        );
    }

    #[test]
    fn plugin_actor_active_doc_visible_inside_onopen_handler() {
        // While an onOpen handler runs, slab.document.getActive()
        // should return the event being dispatched. Verify by having
        // the handler stash getActive().name into a notify.
        let script = r#"
            slab.document.onOpen(function () {
                var d = slab.document.getActive();
                slab.ui.notify("active:" + (d ? d.name : "<null>"), "info");
            });
        "#;
        let h = PluginActor::spawn(
            "p.onopen.active".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        h.send(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
            "/tmp/Gamma.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");

        let regs = shared.registrations.lock().unwrap();
        let messages: Vec<&str> = regs
            .notifications
            .iter()
            .map(|n| n.message.as_str())
            .collect();
        assert!(
            messages.contains(&"active:Gamma"),
            "getActive() inside onOpen must see the active doc; got {messages:?}"
        );
    }

    #[test]
    fn plugin_actor_active_doc_cleared_before_onclose_handler_runs() {
        // Symmetric invariant: when onClose fires, getActive() should
        // already report null. (We clear active_doc BEFORE dispatch.)
        let script = r#"
            slab.document.onClose(function () {
                var d = slab.document.getActive();
                slab.ui.notify("after-close:" + (d === null ? "null" : "still-set"), "info");
            });
        "#;
        let h = PluginActor::spawn(
            "p.onclose.cleared".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        // Open then close so active_doc has been Some at some point.
        h.send(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
            "/tmp/Delta.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::DocumentClosed(DocumentEvent::from_path(
            "/tmp/Delta.pdf",
        )))
        .expect("send");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");

        let regs = shared.registrations.lock().unwrap();
        let messages: Vec<&str> = regs
            .notifications
            .iter()
            .map(|n| n.message.as_str())
            .collect();
        assert!(
            messages.contains(&"after-close:null"),
            "getActive() inside onClose must be null; got {messages:?}"
        );
    }

    #[test]
    fn plugin_actor_shuts_down_cleanly_with_persistents_registered() {
        // The "must clear Persistents before runtime drop" invariant.
        // Register handlers, send Shutdown without ever dispatching,
        // join, assert no panic / abort.
        let script = r#"
            slab.document.onOpen(function () {});
            slab.document.onOpen(function () {});
            slab.document.onClose(function () {});
        "#;
        let h = PluginActor::spawn(
            "p.persistents.shutdown".into(),
            caps_ui_full(),
            grants_ui_full(),
            script.into(),
        )
        .expect("spawn");
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join — must not abort");
    }

    #[test]
    fn plugin_actor_propagates_syntax_error_from_top_level() {
        let err = PluginActor::spawn(
            "p.syntax".into(),
            Capabilities::default(),
            PluginGrants::default(),
            "function ( {".into(),
        )
        .expect_err("syntax error must surface");
        assert!(
            matches!(err, RuntimeError::Syntax(_)),
            "expected Syntax, got {err:?}"
        );
    }

    #[test]
    fn plugin_actor_propagates_thrown_error_from_top_level() {
        let err = PluginActor::spawn(
            "p.throw".into(),
            Capabilities::default(),
            PluginGrants::default(),
            "throw new Error('boom-at-init');".into(),
        )
        .expect_err("thrown error must surface");
        match err {
            RuntimeError::Thrown(msg) => {
                assert!(msg.contains("boom-at-init"), "got {msg:?}")
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn plugin_actor_logs_captured_during_top_level_eval() {
        // console.log during eval lands in shared.logs.
        let h = PluginActor::spawn(
            "p.logs".into(),
            Capabilities::default(),
            PluginGrants::default(),
            "console.log('hello from init');".into(),
        )
        .expect("spawn");
        let shared = h.shared_state();
        h.send(RuntimeCmd::Shutdown).expect("shutdown");
        h.join().expect("join");
        let logs = shared.logs.lock().unwrap();
        let messages: Vec<&str> = logs.iter().map(|e| e.message.as_str()).collect();
        assert!(
            messages.contains(&"hello from init"),
            "expected init log captured; got {messages:?}"
        );
    }
}
