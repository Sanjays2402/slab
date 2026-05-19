//! Workshop (v2.0.0 Slice 6.6) — process-global registry of live
//! plugin actor handles.
//!
//! Tauri owns one [`PluginRuntimeRegistry`] via `.manage(...)`. The
//! Tauri commands wired in Slice 6.7 look entries up by plugin_id to
//! broadcast `slab_plugins_document_{opened,closed}` events to every
//! enabled plugin.
//!
//! ## Why a registry
//!
//! Each enabled plugin owns a [`WorkerHandle`] (`runtime/actor.rs`).
//! Two patterns are well-served by holding those handles in a single
//! `HashMap<String, LiveEntry>`:
//!
//! 1. **Re-enable.** The cabinet allows users to toggle a plugin off
//!    and back on without restarting Slab. `insert` replaces an
//!    existing entry, which causes the previous `WorkerHandle`'s
//!    `Drop` to fire — issuing a clean Shutdown to the old worker
//!    thread before the new one becomes addressable.
//!
//! 2. **Broadcast.** When a PDF is loaded the frontend sends one
//!    Tauri call; the host fan-outs `RuntimeCmd::DocumentOpened` to
//!    every live actor. The registry's `broadcast` method does this
//!    in O(plugins) with no per-call lookups.
//!
//! ## Slice 6.6 status
//!
//! The registry itself is fully functional — `insert`/`remove`/
//! `broadcast`/`live_plugin_ids` are all wired and tested. What's
//! still placeholder: [`crate::plugins::runtime::actor::PluginActor::spawn`]
//! returns a handle whose worker body is a no-op drain (Slice 6.5).
//! Once 6.5 lands, the same registry surface starts dispatching into
//! real `Persistent<Function>` callbacks.

use std::collections::HashMap;
use std::sync::Mutex;

use super::runtime::actor::{RuntimeCmd, WorkerHandle};

/// One live plugin actor entry in the registry. Owns the
/// [`WorkerHandle`]; dropping the entry triggers
/// `WorkerHandle::Drop` which Shutdowns + joins the worker thread.
///
/// In Slice 6.5 this will grow an `ActorSharedState` field carrying
/// the shared lifecycle + active-doc snapshots so the registry can
/// expose them for Tauri commands that need to introspect a single
/// plugin (e.g. "list active onOpen handlers for diagnostics").
pub struct LiveEntry {
    pub handle: WorkerHandle,
}

impl LiveEntry {
    /// Convenience constructor — keeps call sites tidy at the
    /// expense of a tiny allocation.
    pub fn new(handle: WorkerHandle) -> Self {
        Self { handle }
    }
}

/// Process-global map of `plugin_id -> LiveEntry`.
///
/// Tauri manages one of these via `app.manage(PluginRuntimeRegistry::default())`
/// and commands access it via `tauri::State<'_, PluginRuntimeRegistry>`.
///
/// The mutex is held for the duration of `insert` / `remove` /
/// `broadcast` calls. None of those touch JS execution (they only
/// send messages on crossbeam channels), so contention is negligible.
pub struct PluginRuntimeRegistry {
    inner: Mutex<HashMap<String, LiveEntry>>,
}

impl Default for PluginRuntimeRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl PluginRuntimeRegistry {
    /// Insert (or replace) the entry for `plugin_id`. If an existing
    /// entry is replaced, its `WorkerHandle` drops at the end of this
    /// call — which sends `Shutdown` to the old worker and joins it.
    /// That gives "re-enable" the right semantics without callers
    /// having to remember to `remove` first.
    pub fn insert(&self, plugin_id: String, entry: LiveEntry) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(plugin_id, entry);
        }
    }

    /// Remove the entry for `plugin_id`, returning it for explicit
    /// drop. Returns `None` if no such plugin is live (no-op).
    ///
    /// Callers typically `drop()` the result immediately; we return
    /// `Option<LiveEntry>` rather than `bool` so future tests /
    /// diagnostics can inspect the handle before letting it drop.
    pub fn remove(&self, plugin_id: &str) -> Option<LiveEntry> {
        self.inner.lock().ok().and_then(|mut g| g.remove(plugin_id))
    }

    /// Broadcast a command to every live actor. Send failures
    /// (channel disconnected → worker already exited) are swallowed
    /// silently — the next `insert`/`remove` cycle will clean up the
    /// dead entry. We never block waiting for callbacks: lifecycle
    /// dispatch is best-effort and asynchronous from the host's POV.
    pub fn broadcast(&self, cmd: RuntimeCmd) {
        if let Ok(guard) = self.inner.lock() {
            for entry in guard.values() {
                let _ = entry.handle.send(cmd.clone());
            }
        }
    }

    /// List the plugin IDs that currently have a live actor.
    /// Useful for diagnostics and the cabinet panel.
    pub fn live_plugin_ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .ok()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Number of live actors. O(1).
    pub fn len(&self) -> usize {
        self.inner.lock().ok().map(|g| g.len()).unwrap_or(0)
    }

    /// `true` when no actors are live.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::grants::PluginGrants;
    use crate::plugins::manifest::Capabilities;
    use crate::plugins::runtime::actor::{DocumentEvent, PluginActor, RuntimeCmd};

    /// Spawn a fresh actor with the placeholder body — enough for
    /// the registry contract tests.
    fn spawn_for(id: &str) -> WorkerHandle {
        PluginActor::spawn(
            id.into(),
            Capabilities::default(),
            PluginGrants::default(),
            String::new(),
        )
        .expect("spawn")
    }

    #[test]
    fn default_registry_is_empty() {
        let reg = PluginRuntimeRegistry::default();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.live_plugin_ids().is_empty());
    }

    #[test]
    fn insert_then_lookup_via_live_ids() {
        let reg = PluginRuntimeRegistry::default();
        reg.insert("p.a".into(), LiveEntry::new(spawn_for("p.a")));
        reg.insert("p.b".into(), LiveEntry::new(spawn_for("p.b")));
        assert_eq!(reg.len(), 2);
        let mut ids = reg.live_plugin_ids();
        ids.sort();
        assert_eq!(ids, vec!["p.a".to_string(), "p.b".to_string()]);
    }

    #[test]
    fn insert_replaces_existing_entry() {
        // Re-enable contract: a second insert for the same plugin_id
        // drops the previous entry, which Shutdowns the old worker.
        let reg = PluginRuntimeRegistry::default();
        reg.insert("p.re".into(), LiveEntry::new(spawn_for("p.re")));
        reg.insert("p.re".into(), LiveEntry::new(spawn_for("p.re")));
        // Still exactly one live entry for that ID.
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.live_plugin_ids(), vec!["p.re".to_string()]);
    }

    #[test]
    fn remove_returns_entry_and_evicts() {
        let reg = PluginRuntimeRegistry::default();
        reg.insert("p.rm".into(), LiveEntry::new(spawn_for("p.rm")));
        let entry = reg.remove("p.rm");
        assert!(entry.is_some());
        assert!(reg.is_empty());
        // Removing again is a no-op (returns None).
        assert!(reg.remove("p.rm").is_none());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let reg = PluginRuntimeRegistry::default();
        assert!(reg.remove("nope").is_none());
    }

    #[test]
    fn broadcast_reaches_every_live_actor() {
        // We can't observe the JS-side effect yet (Slice 6.5), but we
        // can verify broadcast doesn't panic and the registry stays
        // consistent across many sends. Burst-stress 3 plugins x 16
        // events each.
        let reg = PluginRuntimeRegistry::default();
        for i in 0..3 {
            reg.insert(
                format!("p.bcast.{i}"),
                LiveEntry::new(spawn_for(&format!("p.bcast.{i}"))),
            );
        }
        for i in 0..16 {
            reg.broadcast(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
                format!("/tmp/B{i}.pdf"),
            )));
            reg.broadcast(RuntimeCmd::DocumentClosed(DocumentEvent::from_path(
                format!("/tmp/B{i}.pdf"),
            )));
        }
        assert_eq!(reg.len(), 3);
    }

    #[test]
    fn broadcast_empty_registry_is_noop() {
        let reg = PluginRuntimeRegistry::default();
        // No live actors → no targets → must not panic / leak.
        reg.broadcast(RuntimeCmd::DocumentOpened(DocumentEvent::from_path(
            "/tmp/x.pdf",
        )));
        reg.broadcast(RuntimeCmd::Shutdown);
        assert!(reg.is_empty());
    }

    #[test]
    fn drop_registry_drops_all_entries_and_joins_workers() {
        // Dropping the registry must drop all LiveEntry instances,
        // which in turn drops the WorkerHandles, which Shutdown +
        // join the worker threads. We exercise the path; the join
        // itself is verified by WorkerHandle's own contract tests.
        let reg = PluginRuntimeRegistry::default();
        for i in 0..8 {
            reg.insert(
                format!("p.drop.{i}"),
                LiveEntry::new(spawn_for(&format!("p.drop.{i}"))),
            );
        }
        assert_eq!(reg.len(), 8);
        drop(reg);
    }
}
