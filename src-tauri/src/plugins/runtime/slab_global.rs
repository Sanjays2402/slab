//! Workshop (v2.0.0 Slice 4) — the `slab` global injected into every
//! plugin script.
//!
//! Plugins author against a known surface:
//!
//! ```js
//! slab.beacon.registerTool({ id: 'translate', ... });
//! slab.beacon.registerAiProvider({ id: 'my-llm', ... });
//! slab.ui.registerPanel({ id: 'stats', ... });
//! slab.ui.registerTool({ id: 'quick-redact', ... });
//! slab.ui.notify('Plugin loaded', 'info');
//! ```
//!
//! This module wires those calls to Rust-side handlers that:
//!
//! 1. Consult the plugin's *declared* `Capabilities` and the user's
//!    actual `PluginGrants` via [`crate::plugins::grants::enforce`].
//! 2. On `Ok`, push the descriptor into the shared [`Registrations`]
//!    buffer captured by closure.
//! 3. On `Err(DenyReason)`, throw a JS `Error` with a message that
//!    matches what the Cabinet UI will surface to the user. Plugin
//!    scripts can `try { } catch (e)` to handle this gracefully.
//!
//! ## What this slice does NOT cover
//!
//! - `slab.storage.{get,set,remove}` — Slice 8.
//! - `slab.fetch` — Slice 7.
//!
//! Each unimplemented section throws a `Slice <N>` error message so
//! plugin authors get a clear signal that the surface is reserved
//! but not yet live, rather than the surface silently returning
//! `undefined`.
//!
//! ## `slab.document.*` (live since Slice 6)
//!
//! `onOpen`, `onClose` register `Persistent<Function>` handlers into
//! a per-plugin `LifecycleRegistry` (see `runtime/lifecycle.rs`).
//! `getActive()` reads from a shared `Arc<Mutex<Option<DocumentEvent>>>`
//! the actor (Slice 6.5) updates whenever the frontend opens/closes
//! a PDF. When called from ephemeral `execute_script` / `enable_plugin`
//! paths (where there's no long-lived runtime to host the callback)
//! `onOpen`/`onClose` throw a clear "not available outside enable
//! context" error and `getActive()` returns `null`.

use std::sync::{Arc, Mutex};

use rquickjs::{
    convert::Coerced, function::Opt, function::Rest, Ctx, Exception, Function, Object, Persistent,
    Promise, Result, Value,
};

use super::actor::{FetchRequest, RuntimeCmd};
use super::fetch::SharedPendingFetches;
use super::host_api::{
    BeaconAiProviderReg, BeaconToolReg, NotifyCall, NotifyLevel, Registrations, UiPanelReg,
    UiToolReg,
};
use super::lifecycle::{SharedActiveDoc, SharedLifecycle};
use crate::plugins::grants::{enforce, CapabilityRequest, DenyReason, PluginGrants};
use crate::plugins::manifest::Capabilities;
use crate::plugins::storage::SharedPluginStorage;

/// Bundle of everything a plugin script needs from the host side.
/// Cloned into each closure so the JS-callable functions can
/// enforce capabilities + push registrations without long-lived
/// references.
#[derive(Clone)]
pub(super) struct HostBindings {
    pub plugin_id: String,
    pub declared: Arc<Capabilities>,
    pub granted: Arc<PluginGrants>,
    pub registrations: Arc<Mutex<Registrations>>,
    /// Persistent callbacks registered via `slab.document.on*`.
    /// `None` during ephemeral [`super::Runtime::execute_script`] /
    /// [`super::Runtime::enable_plugin`] paths (no long-lived runtime
    /// → no place to stash callbacks). `Some` only inside the
    /// long-lived per-plugin actor (Slice 6.5).
    pub lifecycle: Option<SharedLifecycle>,
    /// Snapshot of the currently-active document, updated by the
    /// actor whenever it processes a `DocumentOpened`/`DocumentClosed`.
    /// `None` during ephemeral paths or when no PDF is loaded.
    pub active_doc: Option<SharedActiveDoc>,
    /// Channel back to the actor's recv loop, used by `slab.fetch`
    /// to enqueue outbound HTTP. `None` during ephemeral paths
    /// (no actor → fetch unavailable; the binding rejects the
    /// Promise synchronously).
    pub cmd_tx: Option<crossbeam_channel::Sender<RuntimeCmd>>,
    /// Worker-local pending-fetch registry. `None` during
    /// ephemeral paths. Same `Arc<Mutex<_>>` instance the actor
    /// stores; both ends are on the same thread.
    pub pending_fetches: Option<SharedPendingFetches>,
    /// Process-wide per-plugin KV store. `Some` whenever the on-disk
    /// DB at `~/.slab/plugin-storage.sqlite` opened successfully
    /// (which is essentially always — only a filesystem-level
    /// permission denied would force `None` here). `None` during
    /// ephemeral `enable_plugin` / `execute_script` paths or when the
    /// global open errored; the JS binding (Slice 8.5) returns an
    /// already-rejected Promise in that case so plugins can `.catch`
    /// gracefully without losing the actor.
    ///
    /// `dead_code` allow here is a one-commit bridge: Slice 8.4 ships
    /// the plumbing, Slice 8.5 (same tick) ships the readers in
    /// `make_storage_*`. Will be dropped in the very next commit.
    #[allow(dead_code)]
    pub storage: Option<SharedPluginStorage>,
}

/// Install the `slab` global on the given context.
///
/// The shape of the installed object mirrors what plugin TypeScript
/// authors will see in `@slab/plugin-sdk`'s `.d.ts` shims (Slice 9).
/// Sub-objects are organised by domain — `slab.beacon.*`, `slab.ui.*`,
/// etc.
pub(super) fn install_slab(ctx: &Ctx<'_>, bindings: HostBindings) -> Result<()> {
    let slab = Object::new(ctx.clone())?;

    // --- slab.beacon.* -----------------------------------------------------
    let beacon = Object::new(ctx.clone())?;
    beacon.set(
        "registerTool",
        make_register_beacon_tool(ctx, bindings.clone())?,
    )?;
    beacon.set(
        "registerAiProvider",
        make_register_beacon_ai_provider(ctx, bindings.clone())?,
    )?;
    slab.set("beacon", beacon)?;

    // --- slab.ui.* ---------------------------------------------------------
    let ui = Object::new(ctx.clone())?;
    ui.set(
        "registerPanel",
        make_register_ui_panel(ctx, bindings.clone())?,
    )?;
    ui.set(
        "registerTool",
        make_register_ui_tool(ctx, bindings.clone())?,
    )?;
    ui.set("notify", make_ui_notify(ctx, bindings.clone())?)?;
    slab.set("ui", ui)?;

    // --- slab.document.* (Slice 6 — live!) ---------------------------------
    // `onOpen` / `onClose` stash a `Persistent<Function>` into the shared
    // lifecycle registry when called from inside the actor (Slice 6.5);
    // they throw cleanly when called from ephemeral execute_script paths
    // where there's no long-lived runtime to host the callback.
    let document = Object::new(ctx.clone())?;
    document.set("getActive", make_get_active(ctx, bindings.clone())?)?;
    document.set(
        "onOpen",
        make_on_event(ctx, bindings.clone(), LifecycleAxis::OnOpen)?,
    )?;
    document.set(
        "onClose",
        make_on_event(ctx, bindings.clone(), LifecycleAxis::OnClose)?,
    )?;
    slab.set("document", document)?;

    // --- slab.storage.* (Slice 8 — reserved-but-not-live) ------------------
    let storage = Object::new(ctx.clone())?;
    storage.set("get", make_unavailable(ctx, "slab.storage.get", "Slice 8")?)?;
    storage.set("set", make_unavailable(ctx, "slab.storage.set", "Slice 8")?)?;
    storage.set(
        "remove",
        make_unavailable(ctx, "slab.storage.remove", "Slice 8")?,
    )?;
    slab.set("storage", storage)?;

    // --- slab.fetch (Slice 7) ----------------------------------------------
    slab.set("fetch", make_fetch(ctx, bindings.clone())?)?;

    // --- slab.plugin metadata (read-only) ----------------------------------
    // Plugin scripts often want to know their own ID for log prefixes
    // and to namespace storage keys. Stamp it as a plain string prop.
    slab.set("pluginId", bindings.plugin_id.clone())?;

    ctx.globals().set("slab", slab)?;
    Ok(())
}

/// Build a placeholder function that throws an `Error("<name> is not
/// available in this Slab version (lands in <slice>)")` when called.
/// Used for surfaces that are part of the v2.0.0 plan but ship in a
/// later slice.
fn make_unavailable<'js>(
    ctx: &Ctx<'js>,
    surface_name: &'static str,
    slice_label: &'static str,
) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> Result<()> {
        Err(Exception::throw_message(
            &ctx,
            &format!("{surface_name} is not available yet (ships in {slice_label})"),
        ))
    })
}

fn make_register_beacon_tool<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |ctx: Ctx<'js>, descriptor: Value<'js>| {
        gate(&ctx, &b, &CapabilityRequest::BeaconRegisterTool)?;
        let blob = json_from_value(&ctx, descriptor)?;
        let mut regs = b
            .registrations
            .lock()
            .map_err(|_| Exception::throw_internal(&ctx, "registrations mutex poisoned"))?;
        regs.beacon_tools.push(BeaconToolReg {
            plugin_id: b.plugin_id.clone(),
            descriptor: blob,
        });
        Ok::<(), rquickjs::Error>(())
    })
}

fn make_register_beacon_ai_provider<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |ctx: Ctx<'js>, descriptor: Value<'js>| {
        gate(&ctx, &b, &CapabilityRequest::BeaconRegisterAiProvider)?;
        let blob = json_from_value(&ctx, descriptor)?;
        let mut regs = b
            .registrations
            .lock()
            .map_err(|_| Exception::throw_internal(&ctx, "registrations mutex poisoned"))?;
        regs.beacon_ai_providers.push(BeaconAiProviderReg {
            plugin_id: b.plugin_id.clone(),
            descriptor: blob,
        });
        Ok::<(), rquickjs::Error>(())
    })
}

fn make_register_ui_panel<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |ctx: Ctx<'js>, descriptor: Value<'js>| {
        gate(&ctx, &b, &CapabilityRequest::UiRegisterPanel)?;
        let blob = json_from_value(&ctx, descriptor)?;
        let mut regs = b
            .registrations
            .lock()
            .map_err(|_| Exception::throw_internal(&ctx, "registrations mutex poisoned"))?;
        regs.ui_panels.push(UiPanelReg {
            plugin_id: b.plugin_id.clone(),
            descriptor: blob,
        });
        Ok::<(), rquickjs::Error>(())
    })
}

fn make_register_ui_tool<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |ctx: Ctx<'js>, descriptor: Value<'js>| {
        gate(&ctx, &b, &CapabilityRequest::UiRegisterTool)?;
        let blob = json_from_value(&ctx, descriptor)?;
        let mut regs = b
            .registrations
            .lock()
            .map_err(|_| Exception::throw_internal(&ctx, "registrations mutex poisoned"))?;
        regs.ui_tools.push(UiToolReg {
            plugin_id: b.plugin_id.clone(),
            descriptor: blob,
        });
        Ok::<(), rquickjs::Error>(())
    })
}

/// `slab.ui.notify(message, level?)`. Not capability-gated — every
/// plugin can post a toast even with `ui = "none"`, because notify is
/// strictly write-once and host-rendered (no XSS surface; the toast
/// system text-escapes its content). If plugin authors hate this
/// later, gate it on `ui != "none"`.
fn make_ui_notify<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, args: Rest<Value<'js>>| -> Result<()> {
            let argv = args.into_inner();
            // First arg = message, coerced to string per JS rules.
            let raw_msg = argv
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let msg: Coerced<String> = rquickjs::FromJs::from_js(&ctx, raw_msg).map_err(|_| {
                Exception::throw_type(&ctx, "slab.ui.notify: message not coercible to string")
            })?;
            // Second arg (optional) = level. Coerce to string and
            // parse loosely so unknown values degrade rather than
            // throw.
            let level = if let Some(v) = argv.get(1).cloned() {
                let s: Coerced<String> = rquickjs::FromJs::from_js(&ctx, v).map_err(|_| {
                    Exception::throw_type(&ctx, "slab.ui.notify: level not coercible to string")
                })?;
                NotifyLevel::from_str_loose(&s.0)
            } else {
                NotifyLevel::Info
            };
            let mut regs = b
                .registrations
                .lock()
                .map_err(|_| Exception::throw_internal(&ctx, "registrations mutex poisoned"))?;
            regs.notifications.push(NotifyCall {
                plugin_id: b.plugin_id.clone(),
                message: msg.0,
                level,
            });
            Ok(())
        },
    )
}

/// Run [`enforce`] and convert a `DenyReason` into a `throw new
/// Error("…")` inside the JS context. The message text is the
/// same one the Cabinet modal will show users, so plugin authors can
/// pattern-match on it during development.
fn gate(ctx: &Ctx<'_>, b: &HostBindings, req: &CapabilityRequest<'_>) -> Result<()> {
    match enforce(b.declared.as_ref(), b.granted.as_ref(), req) {
        Ok(()) => Ok(()),
        Err(reason) => Err(Exception::throw_message(
            ctx,
            &format_deny_message(&b.plugin_id, req, reason),
        )),
    }
}

/// Human-readable rejection message. Mirrors the strings the Cabinet
/// uses when explaining a denied capability so plugin authors aren't
/// reading two different vocabularies.
fn format_deny_message(plugin_id: &str, req: &CapabilityRequest<'_>, reason: DenyReason) -> String {
    let surface = match req {
        CapabilityRequest::FsRead => "fs read",
        CapabilityRequest::FsWrite => "fs write",
        CapabilityRequest::NetFetch { .. } => "net fetch",
        CapabilityRequest::UiRegisterPanel => "ui.registerPanel",
        CapabilityRequest::UiRegisterTool => "ui.registerTool",
        CapabilityRequest::BeaconRegisterTool => "beacon.registerTool",
        CapabilityRequest::BeaconRegisterAiProvider => "beacon.registerAiProvider",
    };
    let why = match reason {
        DenyReason::NotDeclared => "plugin manifest does not declare this capability",
        DenyReason::NotGranted => "user has not granted this capability",
        DenyReason::GrantTooNarrow => "user grant is narrower than what the plugin needs",
        DenyReason::HostNotAllowed => "host is not in the user-granted allow-list",
    };
    format!("[{plugin_id}] {surface} denied: {why}")
}

/// Convert a JS [`Value`] into a `serde_json::Value`. Used for
/// stashing descriptors in [`Registrations`]. Failure becomes a
/// thrown TypeError, which plugin authors will see as a clear
/// "descriptor was not JSON-serializable" message.
fn json_from_value<'js>(ctx: &Ctx<'js>, v: Value<'js>) -> Result<serde_json::Value> {
    // Round-trip via JSON.stringify so we honour `.toJSON()` hooks
    // and circular-ref errors that QuickJS already implements
    // correctly. Direct ser via rquickjs::serde would also work but
    // wouldn't get us the `.toJSON()` semantics plugin authors
    // expect from idiomatic JS.
    let globals = ctx.globals();
    let json: Object = globals
        .get("JSON")
        .map_err(|_| Exception::throw_internal(ctx, "JSON global missing"))?;
    let stringify: Function = json
        .get("stringify")
        .map_err(|_| Exception::throw_internal(ctx, "JSON.stringify missing"))?;
    let s: Coerced<String> = stringify.call((v,))?;
    serde_json::from_str(&s.0)
        .map_err(|e| Exception::throw_type(ctx, &format!("descriptor not JSON-serializable: {e}")))
}

/// Which lifecycle slot a `slab.document.on*` call writes to.
/// Plain enum so the closure passed to `Function::new` can be a
/// trivial `Copy`.
#[derive(Clone, Copy)]
enum LifecycleAxis {
    OnOpen,
    OnClose,
}

/// Build `slab.document.onOpen` / `slab.document.onClose`.
///
/// Behaviour:
/// - When the host bindings carry a [`SharedLifecycle`] (i.e. we're
///   inside the actor's long-lived context, Slice 6.5), the supplied
///   `callback` is saved as `Persistent::save(&ctx, callback)` and
///   appended to the relevant slot.
/// - When `lifecycle` is `None` (ephemeral `execute_script` /
///   `enable_plugin` paths), the call throws a clear "not available"
///   error so plugin authors using these surfaces from one-shot
///   scripts get an immediate signal rather than a silent no-op.
///
/// Capability gating is intentionally absent: the `ui` axis already
/// gates *visible* surfaces (panels, tools), and document lifecycle
/// events are purely observational — every plugin that's been enabled
/// at all is implicitly granted the ability to observe document
/// open/close. Restricting lifecycle visibility lands as a future
/// "background" capability if Sanjay decides it's worth it.
fn make_on_event<'js>(
    ctx: &Ctx<'js>,
    b: HostBindings,
    axis: LifecycleAxis,
) -> Result<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, callback: Function<'js>| -> Result<()> {
            let Some(reg) = b.lifecycle.as_ref() else {
                let surface = match axis {
                    LifecycleAxis::OnOpen => "slab.document.onOpen",
                    LifecycleAxis::OnClose => "slab.document.onClose",
                };
                return Err(Exception::throw_message(
                    &ctx,
                    &format!("{surface} is not available outside of plugin enable context"),
                ));
            };
            // `Persistent::save` extends the function's lifetime to
            // `'static` by bumping the QuickJS refcount. The matching
            // refcount decrement happens via `Persistent::drop`,
            // which the actor's shutdown path triggers BEFORE the
            // runtime drops (see `LifecycleRegistry::clear`).
            let persistent = Persistent::save(&ctx, callback);
            let mut guard = reg
                .lock()
                .map_err(|_| Exception::throw_internal(&ctx, "lifecycle mutex poisoned"))?;
            match axis {
                LifecycleAxis::OnOpen => guard.push_on_open(persistent),
                LifecycleAxis::OnClose => guard.push_on_close(persistent),
            }
            Ok(())
        },
    )
}

/// Build `slab.document.getActive()`.
///
/// Returns:
/// - `{ path: string, name: string }` when a document is currently
///   open in the viewer (actor has stamped its shared `active_doc`).
/// - `null` when no document is open OR when called from a context
///   without an `active_doc` snapshot (ephemeral runtimes).
///
/// We deliberately return `null` (not `undefined`) for the "no doc"
/// case so plugin authors can use `=== null` checks unambiguously.
fn make_get_active<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> Result<Value<'js>> {
        let Some(snap_arc) = b.active_doc.as_ref() else {
            return Ok(Value::new_null(ctx));
        };
        let snap = snap_arc
            .lock()
            .map_err(|_| Exception::throw_internal(&ctx, "active_doc mutex poisoned"))?;
        match snap.as_ref() {
            None => Ok(Value::new_null(ctx)),
            Some(ev) => {
                let obj = Object::new(ctx.clone())?;
                obj.set("path", ev.path.to_string_lossy().to_string())?;
                obj.set("name", ev.name.clone())?;
                Ok(obj.into_value())
            }
        }
    })
}

/// `slab.fetch(url, init?)` — host-mediated HTTP fetch.
///
/// Returns a JS `Promise<Response>` where `Response` is a plain
/// object shaped like the web Fetch `Response` interface:
///
/// ```js
/// {
///   ok: boolean,        // status in 200..=299
///   status: number,     // HTTP status code
///   statusText: string, // reason phrase (may be empty)
///   url: string,        // final URL after redirects
///   headers: Record<string, string>,
///   body: string,       // utf-8 body; binary bodies arrive as
///                       //   replacement-char-tolerated strings
/// }
/// ```
///
/// On network / capability / parse errors the Promise rejects with
/// a JS `Error` whose message is the host's failure reason. HTTP
/// 4xx and 5xx responses **resolve** with `ok=false` (per the web
/// Fetch spec — only network-layer failures reject).
///
/// `init` accepts a subset of the web Fetch init bag:
/// - `method`: string, default `"GET"`
/// - `headers`: plain object `{ key: value }` (string values)
/// - `body`: string OR null/undefined
/// - `timeoutMs`: number, default 30_000, clamped to `[1, 120_000]`
///
/// Streaming bodies, AbortSignals, credentials, cache hints, and
/// referrer policy are all deliberately unsupported. Plugins that
/// need those should ship their own platform-specific bindings.
fn make_fetch<'js>(ctx: &Ctx<'js>, b: HostBindings) -> Result<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, url: String, init: Opt<Object<'js>>| -> Result<Value<'js>> {
            // 1. Extract host. Pre-flight parse — runs *before*
            //    the capability gate so we can refuse junk URLs
            //    without leaking grant info, and so the gate sees
            //    a well-formed host string.
            let host = match super::fetch::extract_host(&url) {
                Ok(h) => h,
                Err(e) => return reject_now(&ctx, &e),
            };

            // 2. Capability gate. Throws (not rejects) on deny so
            //    plugin authors get a sync throw at the call site,
            //    matching how every other gate in this module
            //    fails. `try { await slab.fetch(...) }` still
            //    catches it because async functions wrap throws.
            gate(&ctx, &b, &CapabilityRequest::NetFetch { host: &host })?;

            // 3. Build the request from `init`. Errors here
            //    reject the Promise (input was syntactically a
            //    valid call, semantically wrong → async reject).
            let request = match build_fetch_request(&ctx, url, init.0) {
                Ok(r) => r,
                Err(e) => return reject_now(&ctx, &e),
            };

            // 4. Create a JS Promise, persist the resolve/reject
            //    handles for cross-tick settlement.
            let (promise, resolve, reject) = Promise::new(&ctx)?;
            let resolve_p = Persistent::save(&ctx, resolve);
            let reject_p = Persistent::save(&ctx, reject);

            // 5. Enqueue into the worker-local pending map and
            //    fire a `Fetch` command at our own recv loop. If
            //    either side is `None` we're on the ephemeral
            //    path (no actor) — reject synchronously.
            let (tx, pending) = match (&b.cmd_tx, &b.pending_fetches) {
                (Some(t), Some(p)) => (t, p),
                _ => {
                    return reject_now(
                        &ctx,
                        "slab.fetch unavailable outside the per-plugin runtime (enable the plugin first)",
                    );
                }
            };

            let request_id = match super::fetch::enqueue_pending(pending, (resolve_p, reject_p)) {
                Some(id) => id,
                None => {
                    return reject_now(&ctx, "slab.fetch: internal pending-fetch table poisoned");
                }
            };

            if let Err(e) = tx.send(RuntimeCmd::Fetch {
                request_id,
                request,
            }) {
                // Worker exited between binding install and send.
                // Drop the now-unreferenced pending entry to keep
                // the table from growing — and reject so the
                // plugin doesn't await forever.
                let _ = super::fetch::take_pending(pending, request_id);
                return reject_now(&ctx, &format!("slab.fetch: actor send failed ({e})"));
            }

            Ok(promise.into_value())
        },
    )
}

/// Build a JS `Promise` that's already-rejected with a new `Error`
/// whose message is `msg`. Returned as a `Value` so callers can
/// `return reject_now(...)` directly.
///
/// We use `eval` rather than reaching for `Promise.reject` via the
/// globals because (a) it's two fewer property lookups and (b) it
/// makes the Error stack trace originate from a single, stable
/// host-side source string instead of plugin-supplied input.
fn reject_now<'js>(ctx: &Ctx<'js>, msg: &str) -> Result<Value<'js>> {
    let escaped = serde_json::to_string(msg).unwrap_or_else(|_| "\"<unprintable>\"".to_string());
    let src = format!("Promise.reject(new Error({escaped}))");
    ctx.eval::<Value<'js>, _>(src.as_bytes())
}

/// Translate a JS `init` object into a [`FetchRequest`].
///
/// Returns a host-side `String` on error (not an rquickjs `Result`)
/// so the caller can fold both "missing init" and "malformed init"
/// branches into a single `reject_now` call.
fn build_fetch_request<'js>(
    ctx: &Ctx<'js>,
    url: String,
    init: Option<Object<'js>>,
) -> std::result::Result<FetchRequest, String> {
    let Some(init) = init else {
        return Ok(FetchRequest {
            method: "GET".to_string(),
            url,
            headers: vec![],
            body: None,
            timeout_ms: 30_000,
        });
    };

    let method = init
        .get::<_, Option<String>>("method")
        .map_err(|e| format!("init.method: {e}"))?
        .unwrap_or_else(|| "GET".to_string())
        .to_uppercase();

    let headers = parse_headers(ctx, &init).map_err(|e| format!("init.headers: {e}"))?;
    let body = parse_body(&init).map_err(|e| format!("init.body: {e}"))?;

    let timeout_ms = init
        .get::<_, Option<u64>>("timeoutMs")
        .map_err(|e| format!("init.timeoutMs: {e}"))?
        .unwrap_or(30_000)
        .clamp(1, 120_000);

    Ok(FetchRequest {
        method,
        url,
        headers,
        body,
        timeout_ms,
    })
}

/// Pull a `Record<string, string>` from `init.headers`.
///
/// Tuple-array form (`[["k", "v"]]`) is accepted by the web Fetch
/// spec but unused in practice by Slab plugin authors; we skip it
/// for Slice 7 and fall through to empty headers. A future cleanup
/// slice can add it if anyone asks.
fn parse_headers<'js>(_ctx: &Ctx<'js>, init: &Object<'js>) -> Result<Vec<(String, String)>> {
    let h: Option<Value<'js>> = init.get("headers")?;
    let Some(h) = h else {
        return Ok(vec![]);
    };
    let Some(obj) = h.as_object() else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for k in obj.keys::<String>() {
        let k = k?;
        let v: String = obj.get(&k)?;
        // Lowercase header names so the host can match them
        // case-insensitively (HTTP/1.1 names are case-insensitive
        // and reqwest preserves whatever case the caller used —
        // normalising here gives plugin authors predictable echo
        // behaviour).
        out.push((k.to_lowercase(), v));
    }
    Ok(out)
}

/// Pull a string body from `init.body`. ArrayBuffer / Uint8Array
/// bodies are deferred to a future slice — plugin authors who need
/// to send binary should stringify it (base64 etc.) themselves for
/// now.
fn parse_body<'js>(init: &Object<'js>) -> Result<Option<Vec<u8>>> {
    let b: Option<Value<'js>> = init.get("body")?;
    let Some(b) = b else {
        return Ok(None);
    };
    if b.is_null() || b.is_undefined() {
        return Ok(None);
    }
    if let Some(s) = b.as_string() {
        let s = s.to_string()?;
        return Ok(Some(s.into_bytes()));
    }
    // Silently ignore typed-array / arraybuffer bodies for Slice 7.
    // Plugin authors get an empty body, which the host will surface
    // back as `Content-Length: 0` — surprising but not dangerous.
    // TODO(slice-7.x): wire `rquickjs::ArrayBuffer` and `TypedArray`.
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::{BeaconCap, FsCap, NetCap, UiCap};

    #[test]
    fn format_deny_message_includes_plugin_and_surface() {
        let msg = format_deny_message(
            "com.x.y",
            &CapabilityRequest::BeaconRegisterTool,
            DenyReason::NotGranted,
        );
        assert!(msg.contains("com.x.y"));
        assert!(msg.contains("beacon.registerTool"));
        assert!(msg.contains("not granted"));
    }

    #[test]
    fn format_deny_message_covers_all_reasons() {
        // sanity: every reason yields a non-empty descriptive string.
        for r in [
            DenyReason::NotDeclared,
            DenyReason::NotGranted,
            DenyReason::GrantTooNarrow,
            DenyReason::HostNotAllowed,
        ] {
            let msg = format_deny_message("p", &CapabilityRequest::FsWrite, r.clone());
            assert!(!msg.is_empty());
            assert!(msg.contains("fs write"));
        }
    }

    // Smoke test for the plain-Rust types — full integration of
    // install_slab(...) lives in runtime/mod.rs::tests because it
    // needs a live Context.

    #[test]
    fn host_bindings_clones_share_state() {
        let caps = Capabilities {
            fs: FsCap::None,
            net: NetCap::None,
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            net_allow_hosts: vec![],
            fs_allow_paths: vec![],
        };
        let grants = PluginGrants {
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            ..PluginGrants::default()
        };
        let b1 = HostBindings {
            plugin_id: "p".into(),
            declared: Arc::new(caps),
            granted: Arc::new(grants),
            registrations: Arc::new(Mutex::new(Registrations::default())),
            lifecycle: None,
            active_doc: None,
            cmd_tx: None,
            pending_fetches: None,
            storage: None,
        };
        let b2 = b1.clone();
        b1.registrations.lock().unwrap().ui_panels.push(UiPanelReg {
            plugin_id: "p".into(),
            descriptor: serde_json::Value::Null,
        });
        assert_eq!(b2.registrations.lock().unwrap().ui_panels.len(), 1);
    }
}
