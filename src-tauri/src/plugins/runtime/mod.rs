//! Workshop (v2.0.0) — sandboxed QuickJS runtime for plugin scripts.
//!
//! This module wraps [`rquickjs`] in a thin newtype that gives every
//! plugin script a *fresh* interpreter context with strict resource
//! limits. The goal is twofold:
//!
//! 1. **Isolation.** No script can see another script's globals,
//!    prototypes, or closures. Each call to [`Runtime::execute_script`]
//!    builds a brand-new `Context`, runs the source, and drops it.
//!    Future slices may relax this for long-lived plugins, but Slice 1
//!    explicitly verifies the "fresh world per execution" guarantee.
//!
//! 2. **Resource bounds.** Sanjay's machine is one OOM away from a
//!    swap-death; an untrusted plugin author shouldn't be able to
//!    `while(true) {}` or `new Array(1e10)`. Each runtime is capped at
//!    [`MEMORY_LIMIT_BYTES`] (16 MB) and an interrupt handler trips
//!    after [`WALL_CLOCK_LIMIT`] (1 s) of execution time.
//!
//! ## Console wiring
//!
//! The sandbox exposes only `console.log`, `console.warn`, and
//! `console.error` — no `console.dir`, no DOM, no `globalThis.fetch`.
//! Each call appends a [`LogEntry`] to the per-execution buffer that
//! callers retrieve via [`ScriptOutput::logs`]. The wiring lives in
//! `runtime::sandbox`.
//!
//! ## What this module is NOT
//!
//! - It does **not** expose Slab APIs yet (that's Slice 4's `slab`
//!   global).
//! - It does **not** persist state across invocations (Slice 8).
//! - It does **not** load ES6 modules or `import` (Slice 1.5).
//! - It does **not** invoke async hooks; everything runs to completion
//!   synchronously inside [`Context::with`].

pub mod actor;
pub mod fetch;
pub mod host_api;
pub mod lifecycle;
pub mod sandbox;
pub mod slab_global;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::{CatchResultExt, CaughtError, Context};

pub use host_api::{
    BeaconAiProviderReg, BeaconToolReg, NotifyCall, NotifyLevel, Registrations, UiPanelReg,
    UiToolReg,
};
pub use sandbox::{LogEntry, LogLevel};

use crate::plugins::grants::PluginGrants;
use crate::plugins::manifest::Capabilities;

/// Hard cap on heap memory used by a single plugin script.
/// 16 MiB is generous for the kinds of glue/transform plugins Workshop
/// targets, and small enough that 30+ enabled plugins can't blow the
/// host's RAM.
pub const MEMORY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// Hard cap on wall-clock execution time for a single script.
/// 1 second is "instantaneous" to the user but plenty for any
/// reasonable startup/event-handler workload. Slow plugin authors will
/// notice immediately.
pub const WALL_CLOCK_LIMIT: Duration = Duration::from_secs(1);

/// What [`Runtime::execute_script`] gives back on success.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScriptOutput {
    /// Captured `console.*` calls, in order of invocation.
    pub logs: Vec<LogEntry>,
}

impl ScriptOutput {
    /// Convenience: just the message strings of `console.log` calls.
    /// Useful for tests; production code should use `logs` directly.
    pub fn log_messages(&self) -> Vec<String> {
        self.logs
            .iter()
            .filter(|e| e.level == LogLevel::Log)
            .map(|e| e.message.clone())
            .collect()
    }
}

/// Result of [`Runtime::enable_plugin`]. Carries everything the host
/// needs to wire a freshly-enabled plugin into the rest of Slab:
/// console output (for diagnostics + cabinet UI), and the plugin's
/// declared registrations (tools, panels, AI providers, ...).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct EnableOutput {
    /// Captured `console.*` calls during top-level evaluation.
    pub logs: Vec<LogEntry>,
    /// Tools / panels / providers / notifications the plugin
    /// registered during top-level eval.
    pub registrations: Registrations,
}

impl EnableOutput {
    /// Convenience mirroring [`ScriptOutput::log_messages`]: just the
    /// message strings of `console.log` calls. Used in tests.
    pub fn log_messages(&self) -> Vec<String> {
        self.logs
            .iter()
            .filter(|e| e.level == LogLevel::Log)
            .map(|e| e.message.clone())
            .collect()
    }
}

/// Anything that can go wrong during script execution. Each variant
/// carries enough context to surface a useful message in the Cabinet
/// UI without leaking internal QuickJS state.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Couldn't create the underlying QuickJS runtime (OOM at boot).
    #[error("failed to create QuickJS runtime: {0}")]
    Init(String),

    /// The script could not be parsed. Carries the JS-level error
    /// message (e.g. `"unexpected token"`).
    #[error("syntax error: {0}")]
    Syntax(String),

    /// The script ran but threw an uncaught exception. The message
    /// matches what `try { ... } catch (e) { e.message }` would see.
    #[error("script threw: {0}")]
    Thrown(String),

    /// Script tripped the interrupt handler (exceeded
    /// [`WALL_CLOCK_LIMIT`]).
    #[error("script exceeded {limit_ms} ms wall-clock limit")]
    TimeLimit { limit_ms: u128 },

    /// Script tried to allocate beyond [`MEMORY_LIMIT_BYTES`].
    /// QuickJS surfaces this as a generic exception, but we recognise
    /// the `out of memory` marker and re-tag it.
    #[error("script exceeded memory limit ({limit_bytes} bytes)")]
    MemoryLimit { limit_bytes: usize },
}

/// Owning handle to the sandboxed QuickJS runtime for a single plugin
/// script invocation.
///
/// Cheap to construct — one heap allocation for the runtime plus a
/// closure for the interrupt handler. Drop the [`Runtime`] when the
/// script is done to release all QuickJS memory.
pub struct Runtime {
    inner: rquickjs::Runtime,
}

impl Runtime {
    /// Build a new runtime with Slab's default limits applied.
    pub fn new() -> Result<Self, RuntimeError> {
        let inner = rquickjs::Runtime::new().map_err(|e| RuntimeError::Init(format!("{e}")))?;
        inner.set_memory_limit(MEMORY_LIMIT_BYTES);
        Ok(Self { inner })
    }

    /// Run `source` to completion in a fresh `Context`. Returns the
    /// captured `console.*` output on success.
    ///
    /// The runtime is interrupted at [`WALL_CLOCK_LIMIT`]. The
    /// interrupt handler is wired *before* eval starts so even a
    /// `while(true)` loop is killed.
    ///
    /// `plugin_id` is captured into the log entries so callers can
    /// route messages to per-plugin sinks (e.g. the Cabinet panel,
    /// stderr, tracing target).
    pub fn execute_script(
        &self,
        plugin_id: &str,
        source: &str,
    ) -> Result<ScriptOutput, RuntimeError> {
        let deadline = Instant::now() + WALL_CLOCK_LIMIT;
        // Install the interrupt handler. Closure captures the deadline
        // and returns `true` to abort once exceeded. QuickJS polls
        // this between bytecode operations.
        self.inner
            .set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

        let context =
            Context::full(&self.inner).map_err(|e| RuntimeError::Init(format!("context: {e}")))?;

        // Shared log buffer the sandbox functions write into.
        let log_buffer: Arc<Mutex<Vec<LogEntry>>> = Arc::new(Mutex::new(Vec::new()));

        let plugin_id = plugin_id.to_string();
        let result: Result<(), RuntimeError> = context.with(|ctx| {
            sandbox::install_console(&ctx, plugin_id.clone(), Arc::clone(&log_buffer))
                .map_err(|e| RuntimeError::Init(format!("console install: {e}")))?;

            match ctx.eval::<rquickjs::Value, _>(source).catch(&ctx) {
                Ok(_) => Ok(()),
                Err(caught) => Err(classify_error(caught)),
            }
        });

        // Always clear the interrupt handler so a subsequent reuse of
        // this Runtime doesn't inherit a stale deadline. (Currently we
        // create one Runtime per script, but make the contract clean
        // for future Slice 4 work that may reuse runtimes.)
        self.inner.set_interrupt_handler(None);

        result?;

        let logs = Arc::try_unwrap(log_buffer)
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());
        Ok(ScriptOutput { logs })
    }

    /// Evaluate a plugin's `script.js` at enable time, with the
    /// `slab` global installed and capability enforcement wired
    /// through.
    ///
    /// This is the v2.0.0 lifecycle entrypoint: the host calls this
    /// exactly once when a user enables a plugin (or at boot for
    /// already-enabled plugins). The plugin's top-level code is
    /// expected to declare its contributions via `slab.beacon.*`,
    /// `slab.ui.*`, etc. These calls accumulate into [`Registrations`]
    /// inside the returned [`EnableOutput`].
    ///
    /// The fresh-context contract from [`Self::execute_script`] still
    /// holds — every call gets its own `Context`. State that needs to
    /// persist between events (e.g. `slab.document.onOpen` callbacks)
    /// lives in the per-plugin actor (Slice 6.5) built on top of this
    /// primitive; the ephemeral `enable_plugin` here is only used for
    /// one-shot smoke evaluation in tests.
    pub fn enable_plugin(
        &self,
        plugin_id: &str,
        declared: &Capabilities,
        granted: &PluginGrants,
        source: &str,
    ) -> Result<EnableOutput, RuntimeError> {
        let deadline = Instant::now() + WALL_CLOCK_LIMIT;
        self.inner
            .set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

        let context =
            Context::full(&self.inner).map_err(|e| RuntimeError::Init(format!("context: {e}")))?;

        let log_buffer: Arc<Mutex<Vec<LogEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let registrations: Arc<Mutex<Registrations>> =
            Arc::new(Mutex::new(Registrations::default()));

        let plugin_id_string = plugin_id.to_string();
        let declared_arc = Arc::new(declared.clone());
        let granted_arc = Arc::new(granted.clone());
        let regs_for_closure = Arc::clone(&registrations);

        let result: Result<(), RuntimeError> = context.with(|ctx| {
            sandbox::install_console(&ctx, plugin_id_string.clone(), Arc::clone(&log_buffer))
                .map_err(|e| RuntimeError::Init(format!("console install: {e}")))?;

            let bindings = slab_global::HostBindings {
                plugin_id: plugin_id_string.clone(),
                declared: declared_arc,
                granted: granted_arc,
                registrations: regs_for_closure,
                // Ephemeral enable: no long-lived runtime, so no
                // lifecycle / active_doc snapshots / fetch channel /
                // storage handle. The actor path (Slice 6.5+7+8)
                // constructs HostBindings with `Some(..)` for all.
                lifecycle: None,
                active_doc: None,
                cmd_tx: None,
                pending_fetches: None,
                storage: None,
            };
            slab_global::install_slab(&ctx, bindings)
                .map_err(|e| RuntimeError::Init(format!("slab global install: {e}")))?;

            match ctx.eval::<rquickjs::Value, _>(source).catch(&ctx) {
                Ok(_) => Ok(()),
                Err(caught) => Err(classify_error(caught)),
            }
        });

        self.inner.set_interrupt_handler(None);
        result?;

        let logs = Arc::try_unwrap(log_buffer)
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());
        let registrations = Arc::try_unwrap(registrations)
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());
        Ok(EnableOutput {
            logs,
            registrations,
        })
    }
}

/// Map a `CaughtError` to the appropriate `RuntimeError` variant.
/// Splits "syntax error" from "thrown" from "interrupted / OOM" by
/// pattern-matching the rendered exception string.
fn classify_error(caught: CaughtError) -> RuntimeError {
    let rendered = format!("{caught}");
    let lower = rendered.to_ascii_lowercase();

    // QuickJS uses InternalError("interrupted") when an interrupt
    // handler returns true. We map that to TimeLimit because that's
    // the only interrupt we install in this slice.
    if lower.contains("interrupted") {
        return RuntimeError::TimeLimit {
            limit_ms: WALL_CLOCK_LIMIT.as_millis(),
        };
    }
    if lower.contains("out of memory") {
        return RuntimeError::MemoryLimit {
            limit_bytes: MEMORY_LIMIT_BYTES,
        };
    }

    match caught {
        CaughtError::Error(e) => {
            // rquickjs::Error::Exception is the catch-all for thrown
            // JS values; anything else (e.g. ConversionFailed,
            // FromJsConversion) we also surface as Thrown for now.
            RuntimeError::Thrown(format!("{e}"))
        }
        CaughtError::Exception(ex) => {
            // Pull `Error.name` via the underlying object (Exception
            // is a transparent wrapper around `Object`). We use it to
            // discriminate SyntaxError from a generic thrown error.
            let name = ex
                .as_object()
                .get::<_, Option<rquickjs::convert::Coerced<String>>>(
                    rquickjs::atom::PredefinedAtom::Name,
                )
                .ok()
                .flatten()
                .map(|c| c.0)
                .unwrap_or_default();
            let message = ex.message().unwrap_or_default();
            if name == "SyntaxError" {
                RuntimeError::Syntax(message)
            } else {
                let body = if message.is_empty() { name } else { message };
                RuntimeError::Thrown(body)
            }
        }
        CaughtError::Value(_) => RuntimeError::Thrown(rendered),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Slice 1 contract tests ----

    #[test]
    fn console_log_pipes_to_buffer() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .execute_script("test.plugin", "console.log('hello', 'world');")
            .expect("script ran");
        assert_eq!(out.log_messages(), vec!["hello world"]);
        assert_eq!(out.logs.len(), 1);
        assert_eq!(out.logs[0].level, LogLevel::Log);
        assert_eq!(out.logs[0].plugin_id, "test.plugin");
    }

    #[test]
    fn console_warn_and_error_are_tagged_correctly() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .execute_script(
                "lvl.test",
                "console.log('a'); console.warn('b'); console.error('c');",
            )
            .expect("script ran");
        assert_eq!(out.logs.len(), 3);
        assert_eq!(out.logs[0].level, LogLevel::Log);
        assert_eq!(out.logs[1].level, LogLevel::Warn);
        assert_eq!(out.logs[2].level, LogLevel::Error);
        assert_eq!(out.logs[1].message, "b");
        assert_eq!(out.logs[2].message, "c");
    }

    #[test]
    fn console_coerces_non_string_arguments() {
        // `console.log(1, true, {a:1})` should produce a space-joined
        // string. We don't promise JSON-style formatting — just that
        // QuickJS's String() coercion runs.
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .execute_script("coerce.test", "console.log(1, true, null);")
            .expect("script ran");
        // Per JS String() rules: 1->"1", true->"true", null->"null".
        assert_eq!(out.log_messages(), vec!["1 true null"]);
    }

    #[test]
    fn script_syntax_error_returned_as_syntax_variant() {
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .execute_script("syn.test", "function (")
            .expect_err("must fail");
        assert!(
            matches!(err, RuntimeError::Syntax(_)),
            "expected RuntimeError::Syntax, got {err:?}"
        );
    }

    #[test]
    fn script_throws_propagates_as_thrown() {
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .execute_script("throw.test", "throw new Error('boom');")
            .expect_err("must fail");
        match err {
            RuntimeError::Thrown(msg) => assert!(msg.contains("boom"), "got {msg:?}"),
            other => panic!("expected Thrown(boom), got {other:?}"),
        }
    }

    #[test]
    fn each_script_gets_fresh_context() {
        // Define a global in the first script; second script must not
        // see it. We verify by relying on `typeof` not throwing
        // (undeclared globals would normally throw ReferenceError in
        // strict eval mode, but `typeof` is a special form).
        let rt1 = Runtime::new().expect("rt1");
        rt1.execute_script("p1", "globalThis.poison = 42;")
            .expect("first script");

        let rt2 = Runtime::new().expect("rt2");
        let out = rt2
            .execute_script("p2", "console.log(typeof globalThis.poison);")
            .expect("second script");
        assert_eq!(out.log_messages(), vec!["undefined"]);

        // Also verify within a single Runtime: separate execute_script
        // calls get separate Contexts (each call builds a fresh one).
        let rt3 = Runtime::new().expect("rt3");
        rt3.execute_script("p3a", "globalThis.x = 7;").expect("3a");
        let out2 = rt3
            .execute_script("p3b", "console.log(typeof globalThis.x);")
            .expect("3b");
        assert_eq!(out2.log_messages(), vec!["undefined"]);
    }

    #[test]
    fn time_limit_interrupts_infinite_loop() {
        let rt = Runtime::new().expect("runtime");
        let start = Instant::now();
        let err = rt
            .execute_script("loop.test", "while (true) {}")
            .expect_err("infinite loop must be interrupted");
        let elapsed = start.elapsed();

        // Allow a 2-second ceiling: interrupt poll is bytecode-level
        // so there can be a few ms of latency, but we should never
        // burn the whole test budget.
        assert!(
            elapsed < Duration::from_secs(3),
            "infinite loop ran for {elapsed:?}, should have been killed near {WALL_CLOCK_LIMIT:?}"
        );
        assert!(
            matches!(err, RuntimeError::TimeLimit { .. }),
            "expected TimeLimit, got {err:?}"
        );
    }

    #[test]
    fn memory_limit_kills_runaway_allocation() {
        let rt = Runtime::new().expect("runtime");
        // Repeatedly concat a 1MB-ish string until allocator says no.
        // 16MB cap means this should fail within ~30 iterations.
        let script = r#"
            let s = "x".repeat(1024 * 1024);
            let acc = "";
            for (let i = 0; i < 200; i++) {
                acc = acc + s;
            }
        "#;
        let err = rt
            .execute_script("mem.test", script)
            .expect_err("must hit memory limit");
        // We accept either MemoryLimit (clean classification) or
        // Thrown with an OOM-shaped message — different rquickjs
        // versions render this differently. The key invariant is:
        // we returned an error rather than letting the process OOM.
        let ok = matches!(err, RuntimeError::MemoryLimit { .. })
            || matches!(&err, RuntimeError::Thrown(m) if m.to_ascii_lowercase().contains("memory") || m.to_ascii_lowercase().contains("oom") || m.to_ascii_lowercase().contains("internal"));
        assert!(ok, "expected MemoryLimit or Thrown(OOM-ish), got {err:?}");
    }

    #[test]
    fn empty_script_succeeds_with_no_logs() {
        let rt = Runtime::new().expect("runtime");
        let out = rt.execute_script("empty.test", "").expect("script ran");
        assert!(out.logs.is_empty());
    }

    #[test]
    fn plugin_id_propagated_to_every_log_entry() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .execute_script("com.example.tagme", "console.log('a'); console.warn('b');")
            .expect("script ran");
        assert!(out.logs.iter().all(|e| e.plugin_id == "com.example.tagme"));
    }

    // ---- Slice 4 contract tests: enable_plugin + slab global ----

    use crate::plugins::grants::PluginGrants;
    use crate::plugins::manifest::{BeaconCap, Capabilities, FsCap, NetCap, UiCap};

    fn caps_full() -> Capabilities {
        Capabilities {
            fs: FsCap::ReadWrite,
            net: NetCap::Any,
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            net_allow_hosts: vec![],
            fs_allow_paths: vec![],
        }
    }

    fn grants_full() -> PluginGrants {
        PluginGrants {
            fs: FsCap::ReadWrite,
            net: NetCap::Any,
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            net_allow_hosts: vec![],
            fs_allow_paths: vec![],
        }
    }

    #[test]
    fn enable_plugin_installs_slab_global() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p1",
                &caps_full(),
                &grants_full(),
                "console.log(typeof slab); console.log(slab.pluginId);",
            )
            .expect("enable ran");
        // Two log messages: "object" + "p1".
        let msgs = out.log_messages();
        assert_eq!(msgs, vec!["object", "p1"]);
    }

    #[test]
    fn enable_plugin_captures_beacon_tool_registration() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.tool",
                &caps_full(),
                &grants_full(),
                "slab.beacon.registerTool({ id: 'translate', name: 'Translate' });",
            )
            .expect("enable ran");
        assert_eq!(out.registrations.beacon_tools.len(), 1);
        let t = &out.registrations.beacon_tools[0];
        assert_eq!(t.plugin_id, "p.tool");
        assert_eq!(t.descriptor["id"], "translate");
        assert_eq!(t.descriptor["name"], "Translate");
    }

    #[test]
    fn enable_plugin_captures_ai_provider_registration() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.ai",
                &caps_full(),
                &grants_full(),
                "slab.beacon.registerAiProvider({ id: 'my-llm', kind: 'chat' });",
            )
            .expect("enable ran");
        assert_eq!(out.registrations.beacon_ai_providers.len(), 1);
        assert_eq!(
            out.registrations.beacon_ai_providers[0].descriptor["id"],
            "my-llm"
        );
    }

    #[test]
    fn enable_plugin_captures_panel_and_ui_tool() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.panel",
                &caps_full(),
                &grants_full(),
                "slab.ui.registerPanel({ id: 'stats' });\
                 slab.ui.registerTool({ id: 'quick-redact' });",
            )
            .expect("enable ran");
        assert_eq!(out.registrations.ui_panels.len(), 1);
        assert_eq!(out.registrations.ui_tools.len(), 1);
        assert_eq!(out.registrations.ui_panels[0].descriptor["id"], "stats");
        assert_eq!(
            out.registrations.ui_tools[0].descriptor["id"],
            "quick-redact"
        );
    }

    #[test]
    fn enable_plugin_captures_notify_calls_with_level() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.notify",
                &caps_full(),
                &grants_full(),
                "slab.ui.notify('hello');\
                 slab.ui.notify('be careful', 'warn');\
                 slab.ui.notify('boom', 'error');\
                 slab.ui.notify('huh', 'wat');",
            )
            .expect("enable ran");
        let n = &out.registrations.notifications;
        assert_eq!(n.len(), 4);
        assert_eq!(n[0].message, "hello");
        assert_eq!(n[0].level, NotifyLevel::Info);
        assert_eq!(n[1].level, NotifyLevel::Warn);
        assert_eq!(n[2].level, NotifyLevel::Error);
        // unknown level degrades to Info, no throw.
        assert_eq!(n[3].level, NotifyLevel::Info);
    }

    #[test]
    fn enable_plugin_throws_when_capability_not_declared() {
        // Manifest declares beacon=none — plugin tries to register a
        // tool anyway. Should throw with a NotDeclared-shaped error.
        let mut declared = caps_full();
        declared.beacon = BeaconCap::None;
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .enable_plugin(
                "p.bad",
                &declared,
                &grants_full(),
                "slab.beacon.registerTool({ id: 'x' });",
            )
            .expect_err("must reject");
        match err {
            RuntimeError::Thrown(m) => {
                assert!(m.contains("beacon.registerTool"), "got {m:?}");
                assert!(m.contains("does not declare"), "got {m:?}");
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn enable_plugin_throws_when_capability_not_granted() {
        // Manifest declares it; user grant is empty.
        let declared = caps_full();
        let granted = PluginGrants::default(); // deny-all
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .enable_plugin(
                "p.ungranted",
                &declared,
                &granted,
                "slab.ui.registerPanel({ id: 'x' });",
            )
            .expect_err("must reject");
        match err {
            RuntimeError::Thrown(m) => {
                assert!(m.contains("ui.registerPanel"), "got {m:?}");
                assert!(m.contains("not granted"), "got {m:?}");
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn enable_plugin_reserved_surfaces_throw_with_slice_label() {
        // `slab.storage.*` is reserved for Slice 8 — calling it
        // should throw a recognizable message so plugin authors can
        // probe. (`slab.fetch` was the placeholder used here pre-
        // Slice 7; it's now live so we use `storage.get` instead.)
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .enable_plugin(
                "p.future",
                &caps_full(),
                &grants_full(),
                "slab.storage.get('k');",
            )
            .expect_err("must throw");
        match err {
            RuntimeError::Thrown(m) => {
                assert!(m.contains("slab.storage.get"), "got {m:?}");
                assert!(m.contains("Slice 8"), "got {m:?}");
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    /// Slice 7: in the ephemeral `enable_plugin` path there's no
    /// actor, so `slab.fetch` exists but returns an immediately-
    /// rejected Promise rather than throwing. The eval still
    /// succeeds (a Promise return is not a throw) — the rejection
    /// is asynchronous and only observable to plugin code that
    /// `await`s or `.catch()`es the result. The actor-based tests
    /// exercise the live behaviour; this one just pins the
    /// no-throw contract for the ephemeral path.
    #[test]
    fn enable_plugin_fetch_does_not_throw_outside_actor_runtime() {
        let rt = Runtime::new().expect("runtime");
        rt.enable_plugin(
            "p.fetch.ephemeral",
            &caps_full(),
            &grants_full(),
            "slab.fetch('https://example.com');",
        )
        .expect("must succeed (Promise return is not a throw)");
    }

    #[test]
    fn enable_plugin_fresh_context_per_call() {
        let rt = Runtime::new().expect("runtime");
        rt.enable_plugin(
            "p.a",
            &caps_full(),
            &grants_full(),
            "globalThis.poison = 42;",
        )
        .expect("first plugin");
        let out = rt
            .enable_plugin(
                "p.b",
                &caps_full(),
                &grants_full(),
                "console.log(typeof globalThis.poison);",
            )
            .expect("second plugin");
        assert_eq!(out.log_messages(), vec!["undefined"]);
    }

    #[test]
    fn enable_plugin_empty_script_returns_empty_registrations() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin("p.empty", &caps_full(), &grants_full(), "")
            .expect("enable ran");
        assert!(out.registrations.is_empty());
        assert_eq!(out.registrations.total(), 0);
        assert!(out.logs.is_empty());
    }

    #[test]
    fn enable_plugin_console_logs_propagated() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.log",
                &caps_full(),
                &grants_full(),
                "console.log('top-level eval ran');",
            )
            .expect("enable ran");
        assert_eq!(out.log_messages(), vec!["top-level eval ran"]);
    }

    #[test]
    fn enable_plugin_descriptor_can_be_complex_nested_object() {
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.complex",
                &caps_full(),
                &grants_full(),
                "slab.beacon.registerTool({\
                    id: 'foo',\
                    parameters: { type: 'object', properties: { x: { type: 'string' } } },\
                    tags: ['math', 'utility'],\
                    enabled: true,\
                });",
            )
            .expect("enable ran");
        let d = &out.registrations.beacon_tools[0].descriptor;
        assert_eq!(d["id"], "foo");
        assert_eq!(d["parameters"]["properties"]["x"]["type"], "string");
        assert_eq!(d["tags"][1], "utility");
        assert_eq!(d["enabled"], true);
    }

    // ---- Slice 6.3/6.4 contract tests: slab.document.* surface ----

    #[test]
    fn enable_plugin_document_onopen_throws_outside_actor() {
        // Ephemeral `enable_plugin` has `lifecycle: None` — calling
        // `slab.document.onOpen` from a one-shot script must throw a
        // clear "not available outside" error so plugin authors get
        // an immediate signal. (Real callback storage lands in 6.5
        // via PluginActor.)
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .enable_plugin(
                "p.docopen",
                &caps_full(),
                &grants_full(),
                "slab.document.onOpen(() => {});",
            )
            .expect_err("must throw outside enable context");
        match err {
            RuntimeError::Thrown(m) => {
                assert!(m.contains("slab.document.onOpen"), "got {m:?}");
                assert!(m.contains("not available outside"), "got {m:?}");
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn enable_plugin_document_onclose_throws_outside_actor() {
        let rt = Runtime::new().expect("runtime");
        let err = rt
            .enable_plugin(
                "p.docclose",
                &caps_full(),
                &grants_full(),
                "slab.document.onClose(() => {});",
            )
            .expect_err("must throw outside enable context");
        match err {
            RuntimeError::Thrown(m) => {
                assert!(m.contains("slab.document.onClose"), "got {m:?}");
                assert!(m.contains("not available outside"), "got {m:?}");
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn enable_plugin_document_get_active_returns_null_without_actor() {
        // With no `active_doc` snapshot (`None`), `getActive()` must
        // return JS null — not undefined, not throw. Plugin authors
        // can then write `if (slab.document.getActive() === null)`.
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.docga",
                &caps_full(),
                &grants_full(),
                "console.log(slab.document.getActive() === null);",
            )
            .expect("script ran");
        assert_eq!(out.log_messages(), vec!["true"]);
    }

    #[test]
    fn enable_plugin_document_get_active_callable_without_throwing() {
        // Belt-and-braces: `getActive()` is never supposed to throw,
        // it just returns null when no doc is active. Make sure the
        // wrapper doesn't accidentally surface an Exception variant.
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.docga2",
                &caps_full(),
                &grants_full(),
                "let v = slab.document.getActive(); console.log(typeof v);",
            )
            .expect("script ran");
        // `null` coerces to `"object"` in JS — that's the canonical
        // typeof-null quirk; we verify the call shape, not the value.
        assert_eq!(out.log_messages(), vec!["object"]);
    }

    #[test]
    fn enable_plugin_document_surface_keys_exist() {
        // Defensive check: regardless of the actor backing, the
        // `slab.document` object must expose the three documented
        // properties so plugins can `typeof` them at load.
        let rt = Runtime::new().expect("runtime");
        let out = rt
            .enable_plugin(
                "p.docshape",
                &caps_full(),
                &grants_full(),
                "console.log(typeof slab.document.getActive);\
                 console.log(typeof slab.document.onOpen);\
                 console.log(typeof slab.document.onClose);",
            )
            .expect("script ran");
        assert_eq!(out.log_messages(), vec!["function", "function", "function"]);
    }
}
