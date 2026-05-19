//! Workshop (v2.0.0 Slice 6) — per-plugin actor message protocol.
//!
//! This module defines the *types* exchanged between the host side
//! (Tauri commands, the PDF viewer integration) and a plugin's
//! long-lived worker thread (Slice 6.5). The worker thread itself —
//! the one that owns the rquickjs `Runtime` + `Context` and dispatches
//! these commands into JS callbacks — lands in Slice 6.5.
//!
//! Slice 6.1 (this file) is intentionally tiny: just the message
//! enum, the event payload, and a handful of helper constructors,
//! all unit-tested. That lets later slices pull this module in via a
//! simple `use super::actor::{...}` without rewriting any contracts.
//!
//! ## Threading model preview
//!
//! Each enabled plugin gets one dedicated OS thread because rquickjs
//! requires that all JS execution for a given `Runtime` happen on a
//! single thread (the `Context::with` callback borrows the runtime).
//! `Persistent<Function>` is `Send` so callbacks survive across
//! command dispatches, but the runtime itself stays put.
//!
//! See `docs/plans/2026-05-18-v2.0.0-workshop-slice-6.md` for the
//! full implementation arc.

use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{unbounded, Sender};

use super::RuntimeError;
use crate::plugins::grants::PluginGrants;
use crate::plugins::manifest::Capabilities;

/// Commands the host sends to a plugin worker thread.
///
/// The variants intentionally carry small owned payloads (no
/// borrowed refs, no `&Ctx`), so they're `Send + 'static` and can
/// cross a `crossbeam-channel` boundary cleanly.
#[derive(Debug, Clone)]
pub enum RuntimeCmd {
    /// A PDF was loaded into the viewer. The worker will invoke every
    /// `Persistent<Function>` stored in its lifecycle registry under
    /// the `onOpen` axis, passing a JS `{ path, name }` object.
    DocumentOpened(DocumentEvent),
    /// The active PDF was closed (or replaced — the frontend emits a
    /// close for the previous doc followed by an open for the new one).
    DocumentClosed(DocumentEvent),
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

/// Handle to a running plugin actor.
///
/// Owning this means the worker thread is alive (or just exited and
/// awaiting join). Drop performs a best-effort `Shutdown` send + join
/// so leaking a handle never leaks an OS thread.
///
/// In Slice 6.2 the worker body is a placeholder that just drains
/// commands and exits on [`RuntimeCmd::Shutdown`]; the real loop with
/// `Runtime` + `Context` + `Persistent` dispatch lands in Slice 6.5.
pub struct WorkerHandle {
    plugin_id: String,
    tx: Sender<RuntimeCmd>,
    join: Option<JoinHandle<()>>,
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
        // exited on its own (placeholder body in 6.2 does that when
        // the channel closes), in which case `tx.send` errors and the
        // join already happened.
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
/// Slice 6.2 (this) is the skeleton: thread spawn + channel + clean
/// shutdown. Slice 6.5 replaces the worker body with the real runtime
/// loop. Keeping the public `spawn` signature stable across slices
/// means callers (registry, Tauri commands) won't churn.
pub struct PluginActor;

impl PluginActor {
    /// Spawn the actor's worker thread. Returns a [`WorkerHandle`]
    /// once the thread has been created; the worker performs the
    /// real `enable_plugin` evaluation inside its loop in Slice 6.5.
    ///
    /// For 6.2 the worker body just drains commands and exits on
    /// `Shutdown`. The `declared`/`granted`/`source` arguments are
    /// accepted now (so the call sites in Slices 6.6/6.7 stay
    /// unchanged) but ignored until 6.5.
    ///
    /// # Errors
    /// Returns [`RuntimeError::Init`] if the OS refuses to spawn the
    /// thread (rare — typically ulimit/EAGAIN). All other failure
    /// modes (bad source, runtime OOM at boot) surface asynchronously
    /// in Slice 6.5 via a side channel.
    pub fn spawn(
        plugin_id: String,
        declared: Capabilities,
        granted: PluginGrants,
        source: String,
    ) -> Result<WorkerHandle, RuntimeError> {
        let (tx, rx) = unbounded::<RuntimeCmd>();
        let pid_for_handle = plugin_id.clone();
        let join = thread::Builder::new()
            // Visible in `ps -T` / Activity Monitor — makes debugging
            // a stuck plugin trivial: `slab-plugin:com.x.y` is the
            // thread name, plugin_id is the ID in the manifest.
            .name(format!("slab-plugin:{plugin_id}"))
            .spawn(move || {
                run_actor(plugin_id, declared, granted, source, rx);
            })
            .map_err(|e| RuntimeError::Init(format!("actor thread spawn: {e}")))?;

        Ok(WorkerHandle {
            plugin_id: pid_for_handle,
            tx,
            join: Some(join),
        })
    }
}

/// Placeholder actor body for Slice 6.2.
///
/// Slice 6.5 will replace this with a function that creates a
/// `rquickjs::Runtime` + `Context`, evaluates `source` once with the
/// `slab` global installed (and `lifecycle: Some(...)`, so
/// `slab.document.onOpen` actually stashes a `Persistent`), then
/// enters a recv loop that dispatches into the stored callbacks.
///
/// For now we just drain commands and exit on `Shutdown` — that's
/// enough for the registry/broadcast tests in Slices 6.6+.
fn run_actor(
    _plugin_id: String,
    _declared: Capabilities,
    _granted: PluginGrants,
    _source: String,
    rx: crossbeam_channel::Receiver<RuntimeCmd>,
) {
    while let Ok(cmd) = rx.recv() {
        if matches!(cmd, RuntimeCmd::Shutdown) {
            break;
        }
        // Slice 6.5: dispatch DocumentOpened/DocumentClosed into the
        // plugin's `Persistent<Function>` registry inside Context::with.
    }
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

    // ---- Slice 6.2 contract tests: PluginActor + WorkerHandle ----
    //
    // The body of `run_actor` is still a placeholder (Slice 6.5 lands
    // the real Runtime + Context). These tests verify the channel /
    // thread machinery only: spawn succeeds, Shutdown drains the
    // worker, drop joins, and a forgotten handle never leaks a thread.

    use crate::plugins::grants::PluginGrants;
    use crate::plugins::manifest::Capabilities;
    use std::time::{Duration, Instant};

    #[test]
    fn plugin_actor_spawns_and_shuts_down_cleanly() {
        let handle = PluginActor::spawn(
            "p.actor.spawn".into(),
            Capabilities::default(),
            PluginGrants::default(),
            // 6.2 ignores `source`; 6.5 will eval it.
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
        for i in 0..16 {
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
    fn plugin_actor_send_after_join_is_err() {
        // After `join()` consumes the handle the channel is dropped
        // entirely — there's nothing left to send to. We mostly want
        // to assert this path doesn't panic.
        let handle = PluginActor::spawn(
            "p.actor.join".into(),
            Capabilities::default(),
            PluginGrants::default(),
            String::new(),
        )
        .expect("actor must spawn");
        handle.join().expect("join clean");
        // `handle` is consumed; we can't call `send` on it anymore.
        // The post-join "send" semantics are exercised indirectly via
        // the registry (Slice 6.6) which holds the handle across
        // calls — kept here as documentation only.
    }

    #[test]
    fn plugin_actor_drains_pending_events_before_shutdown_exits() {
        // Send a burst of events followed by Shutdown; the worker
        // must process Shutdown promptly without panicking on the
        // queued events. (Placeholder body in 6.2 just `matches!`es
        // on Shutdown; Slice 6.5 will dispatch the events first.)
        let handle = PluginActor::spawn(
            "p.actor.drain".into(),
            Capabilities::default(),
            PluginGrants::default(),
            String::new(),
        )
        .expect("spawn");
        for i in 0..32 {
            let ev = DocumentEvent::from_path(format!("/tmp/doc-{i}.pdf"));
            handle.send(RuntimeCmd::DocumentOpened(ev)).expect("send");
        }
        let start = Instant::now();
        handle.send(RuntimeCmd::Shutdown).expect("send shutdown");
        handle.join().expect("join clean");
        // Drain + join should finish near-instantly — definitely
        // under 250ms on any non-broken machine.
        assert!(
            start.elapsed() < Duration::from_millis(250),
            "drain+join took {:?}, expected < 250ms",
            start.elapsed()
        );
    }
}
