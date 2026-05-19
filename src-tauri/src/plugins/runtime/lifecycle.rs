//! Workshop (v2.0.0 Slice 6.3) — per-plugin store of `Persistent<Function>`
//! lifecycle callbacks.
//!
//! Plugins register handlers via:
//!
//! ```js
//! slab.document.onOpen((event) => { /* ... */ });
//! slab.document.onClose((event) => { /* ... */ });
//! ```
//!
//! Each call stashes a [`rquickjs::Persistent`] of the supplied JS
//! [`rquickjs::Function`] into one of two slot vectors. When the host
//! later sends a [`super::actor::RuntimeCmd::DocumentOpened`] /
//! `DocumentClosed`, the actor (Slice 6.5) iterates the relevant
//! vector and `restore`s each handle inside `Context::with` to invoke
//! it.
//!
//! ## Slot semantics
//!
//! Per axis (`onOpen`, `onClose`) we keep an ordered `Vec<Persistent>`
//! — registrations are **additive** and invoked in registration order.
//! Plugins can register multiple handlers per axis (matches the web
//! `addEventListener` mental model) and compose nicely:
//!
//! ```js
//! slab.document.onOpen(initIndex);
//! slab.document.onOpen(logViewerOpen);
//! // both fire on every DocumentOpened
//! ```
//!
//! ## Drop-order invariant (CRITICAL)
//!
//! `rquickjs::Persistent<Function>` is `Send + 'static` *but* its
//! `Drop` impl decrements the underlying QuickJS refcount. If the
//! owning `rquickjs::Runtime` has already been dropped, this becomes
//! a use-after-free and rquickjs calls `abort()` to surface the bug.
//!
//! Therefore: [`LifecycleRegistry::clear`] **must** be called from
//! inside the actor's worker thread *before* the runtime is dropped.
//! The actor's shutdown path enforces this:
//!
//! ```text
//! lifecycle.clear();   // drops all Persistents
//! drop(ctx);
//! drop(rt);            // safe — registry is empty
//! ```
//!
//! ## Why a separate module
//!
//! Slice 6.4 also needs a shared "active document" snapshot
//! (`Arc<Mutex<Option<DocumentEvent>>>`) so `slab.document.getActive()`
//! can read the current viewer state. We park both types here so
//! `slab_global.rs` keeps a tight focus on per-API wiring.

use std::sync::{Arc, Mutex};

use rquickjs::{Function, Persistent};

use super::actor::DocumentEvent;

/// Persistent function handles, keyed by lifecycle axis.
///
/// Construction goes through [`new_shared`] — callers always work
/// with the `Arc<Mutex<_>>` newtype alias [`SharedLifecycle`] so a
/// registry can be cheaply cloned across the worker thread, the
/// `slab.document.on*` Function closures, and Slice 6.5's dispatch
/// site.
#[derive(Default)]
pub struct LifecycleRegistry {
    on_open: Vec<Persistent<Function<'static>>>,
    on_close: Vec<Persistent<Function<'static>>>,
}

impl LifecycleRegistry {
    /// Append an `onOpen` handler. Called from
    /// `slab.document.onOpen` after `Persistent::save`.
    pub fn push_on_open(&mut self, f: Persistent<Function<'static>>) {
        self.on_open.push(f);
    }

    /// Append an `onClose` handler.
    pub fn push_on_close(&mut self, f: Persistent<Function<'static>>) {
        self.on_close.push(f);
    }

    /// Borrow the `onOpen` slot. Slice 6.5 clones each `Persistent`
    /// into a local snapshot before iterating so callback execution
    /// can drop the mutex.
    pub fn on_open(&self) -> &[Persistent<Function<'static>>] {
        &self.on_open
    }

    /// Borrow the `onClose` slot.
    pub fn on_close(&self) -> &[Persistent<Function<'static>>] {
        &self.on_close
    }

    /// Convenience counters (used in tests + diagnostics).
    pub fn on_open_len(&self) -> usize {
        self.on_open.len()
    }
    pub fn on_close_len(&self) -> usize {
        self.on_close.len()
    }

    /// Drop every stored `Persistent`.
    ///
    /// **Must be called from inside the runtime-owning thread BEFORE
    /// the runtime is dropped** — see the module docs. Idempotent;
    /// safe to call when already empty.
    pub fn clear(&mut self) {
        self.on_open.clear();
        self.on_close.clear();
    }
}

/// Shared handle to a [`LifecycleRegistry`].
///
/// The actor thread holds one clone; each `slab.document.on*` closure
/// holds another. `Mutex` is fine because lifecycle registrations
/// happen at enable-time (single thread, no contention) and dispatch
/// happens later from the same actor thread.
pub type SharedLifecycle = Arc<Mutex<LifecycleRegistry>>;

/// Construct a fresh, empty [`SharedLifecycle`]. Slice 6.5 calls
/// this once per actor; ephemeral `execute_script` paths pass `None`
/// for `lifecycle` and the `slab.document.on*` calls throw cleanly.
pub fn new_shared() -> SharedLifecycle {
    Arc::new(Mutex::new(LifecycleRegistry::default()))
}

/// Snapshot of what the actor currently considers the "active
/// document". `None` means no PDF is open.
///
/// Updated by the actor whenever it processes a
/// [`super::actor::RuntimeCmd::DocumentOpened`] /
/// `DocumentClosed`. Read by `slab.document.getActive()` (Slice 6.4)
/// when the plugin queries the current state.
pub type SharedActiveDoc = Arc<Mutex<Option<DocumentEvent>>>;

/// Construct a fresh `SharedActiveDoc` initialised to `None`.
pub fn new_shared_active_doc() -> SharedActiveDoc {
    Arc::new(Mutex::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_reports_zero_lengths() {
        let reg = LifecycleRegistry::default();
        assert_eq!(reg.on_open_len(), 0);
        assert_eq!(reg.on_close_len(), 0);
        assert!(reg.on_open().is_empty());
        assert!(reg.on_close().is_empty());
    }

    #[test]
    fn clear_on_empty_registry_is_noop() {
        // Idempotent + safe-to-call-when-empty is part of the
        // contract Slice 6.5's shutdown path relies on.
        let mut reg = LifecycleRegistry::default();
        reg.clear();
        reg.clear();
        assert_eq!(reg.on_open_len(), 0);
    }

    #[test]
    fn shared_lifecycle_clones_share_state() {
        // Two clones of the same Arc must observe each other's
        // updates. Real Persistent<Function> values come from a
        // live runtime in Slice 6.5 contract tests; here we just
        // verify the Arc<Mutex<>> plumbing.
        let a = new_shared();
        let b = a.clone();
        // Pretend the registry has some content by setting flag-ish
        // state through Arc::strong_count growth.
        assert_eq!(Arc::strong_count(&a), 2);
        drop(b);
        assert_eq!(Arc::strong_count(&a), 1);
    }

    #[test]
    fn shared_active_doc_starts_none_and_round_trips() {
        let snap = new_shared_active_doc();
        assert!(snap.lock().unwrap().is_none());
        *snap.lock().unwrap() = Some(DocumentEvent::from_path("/tmp/A.pdf"));
        let cur = snap.lock().unwrap().clone();
        assert_eq!(cur.unwrap().name, "A");
    }

    #[test]
    fn shared_active_doc_clones_share_state() {
        let a = new_shared_active_doc();
        let b = a.clone();
        *a.lock().unwrap() = Some(DocumentEvent::from_path("/tmp/X.pdf"));
        assert_eq!(b.lock().unwrap().as_ref().unwrap().name, "X");
    }
}
