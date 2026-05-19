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

pub mod sandbox;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::{CatchResultExt, CaughtError, Context};

pub use sandbox::{LogEntry, LogLevel};

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
}
