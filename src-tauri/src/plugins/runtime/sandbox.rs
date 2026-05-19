//! Console wiring for the Workshop runtime sandbox.
//!
//! This module installs a minimal `console` global with `log`, `warn`,
//! and `error` methods. Each call coerces its variadic args to strings
//! (via `String(...)` ‐ semantics — JS's built-in coercion), joins them
//! with spaces, and appends a [`LogEntry`] to a shared `Vec` provided
//! by the caller.
//!
//! No other `console` methods are exposed in v2.0.0 — `dir`, `table`,
//! `time`, etc. are deliberately omitted because they imply behaviour
//! we don't want to commit to (formatter quirks, timers, etc). If a
//! plugin author calls `console.dir(x)` it will be a `TypeError`, and
//! that's fine for now: plugins should use `console.log(JSON.stringify(x))`
//! during development.

use std::sync::{Arc, Mutex};

use rquickjs::{convert::Coerced, function::Rest, Ctx, Function, Object, Result};

/// Severity of a `console.*` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    /// `console.log(...)` — informational.
    Log,
    /// `console.warn(...)` — non-fatal concern.
    Warn,
    /// `console.error(...)` — error / failure path.
    Error,
}

impl LogLevel {
    /// Short uppercase tag for `tracing` / log-prefix usage.
    pub fn tag(self) -> &'static str {
        match self {
            LogLevel::Log => "LOG",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// A single captured `console.*` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Plugin ID that emitted the call. Captured so the host can route
    /// to per-plugin sinks without parsing the message.
    pub plugin_id: String,
    /// Severity.
    pub level: LogLevel,
    /// Joined, space-separated, string-coerced message.
    pub message: String,
}

/// Install `console.{log,warn,error}` on the context's globals.
///
/// Each function pushes a [`LogEntry`] onto `buffer`. The buffer is
/// shared by reference so the caller can read it after the script
/// finishes (the script never sees the Rust-side buffer).
///
/// All three functions accept variadic args and stringify them with
/// JS's default `String(x)` semantics (rquickjs's
/// [`Coerced<String>`] does exactly that).
pub(super) fn install_console(
    ctx: &Ctx<'_>,
    plugin_id: String,
    buffer: Arc<Mutex<Vec<LogEntry>>>,
) -> Result<()> {
    let console = Object::new(ctx.clone())?;

    console.set(
        "log",
        make_console_fn(ctx, plugin_id.clone(), buffer.clone(), LogLevel::Log)?,
    )?;
    console.set(
        "warn",
        make_console_fn(ctx, plugin_id.clone(), buffer.clone(), LogLevel::Warn)?,
    )?;
    console.set(
        "error",
        make_console_fn(ctx, plugin_id, buffer, LogLevel::Error)?,
    )?;

    ctx.globals().set("console", console)?;
    Ok(())
}

/// Build one `console.<level>` closure.
fn make_console_fn<'js>(
    ctx: &Ctx<'js>,
    plugin_id: String,
    buffer: Arc<Mutex<Vec<LogEntry>>>,
    level: LogLevel,
) -> Result<Function<'js>> {
    Function::new(ctx.clone(), move |args: Rest<Coerced<String>>| {
        let joined = args
            .into_inner()
            .into_iter()
            .map(|c| c.0)
            .collect::<Vec<_>>()
            .join(" ");
        // `lock().unwrap()` is fine here: the mutex is local to a
        // single execute_script() call and never poisoned in the
        // normal control flow. If a panic does land we'd surface
        // a poisoned lock during the post-eval read, which the
        // caller will turn into a generic Init error.
        if let Ok(mut g) = buffer.lock() {
            g.push(LogEntry {
                plugin_id: plugin_id.clone(),
                level,
                message: joined,
            });
        }
    })
}
