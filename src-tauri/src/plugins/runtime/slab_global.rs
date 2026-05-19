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
    convert::Coerced, function::Rest, Ctx, Exception, Function, Object, Persistent, Result, Value,
};

use super::host_api::{
    BeaconAiProviderReg, BeaconToolReg, NotifyCall, NotifyLevel, Registrations, UiPanelReg,
    UiToolReg,
};
use super::lifecycle::{SharedActiveDoc, SharedLifecycle};
use crate::plugins::grants::{enforce, CapabilityRequest, DenyReason, PluginGrants};
use crate::plugins::manifest::Capabilities;

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

    // --- slab.fetch (Slice 7 — reserved-but-not-live) ----------------------
    slab.set("fetch", make_unavailable(ctx, "slab.fetch", "Slice 7")?)?;

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

/// Build the `slab.beacon.registerTool` JS function. Capability gate
/// is `BeaconRegisterTool`; on grant, the descriptor is recorded.
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
        };
        let b2 = b1.clone();
        b1.registrations.lock().unwrap().ui_panels.push(UiPanelReg {
            plugin_id: "p".into(),
            descriptor: serde_json::Value::Null,
        });
        assert_eq!(b2.registrations.lock().unwrap().ui_panels.len(), 1);
    }
}
