//! Theater session manager.
//!
//! Tauri stores exactly one active [`TheaterState`] at a time behind a
//! `Mutex<Option<TheaterState>>`. The audience and presenter windows both
//! subscribe to `slab:theater-state` events emitted on every mutation.
//!
//! The session manager is intentionally tiny — it owns lifecycle (start /
//! end), serves snapshots to either window, and routes mutations from the
//! presenter window through into the shared state. Any rendering, ink
//! capture, or keyboard handling lives in the frontend.

use crate::theater::state::{InkStroke, TheaterState};
use std::path::PathBuf;
use std::sync::Mutex;

/// Single-active-session manager. Wrap with [`Mutex`] when sharing across
/// Tauri command handlers.
#[derive(Default, Debug)]
pub struct TheaterManager {
    inner: Mutex<Option<TheaterState>>,
}

/// Result of a mutation — either an updated snapshot to broadcast, or an
/// `Err` describing why the mutation was rejected (e.g. no active session).
pub type SessionResult<T> = Result<T, &'static str>;

impl TheaterManager {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Begin a new session, replacing any previous one. Returns the fresh
    /// snapshot for immediate broadcast to both windows.
    pub fn start(&self, path: PathBuf, total_pages: u32) -> TheaterState {
        let state = TheaterState::new(path, total_pages);
        *self.inner.lock().expect("theater mutex poisoned") = Some(state.clone());
        state
    }

    /// End the active session and discard its state. Returns the final
    /// snapshot (if any) so the caller can persist ink strokes as
    /// annotations before discard.
    pub fn end(&self) -> Option<TheaterState> {
        self.inner.lock().expect("theater mutex poisoned").take()
    }

    /// Read-only snapshot of the active session, if any.
    pub fn snapshot(&self) -> Option<TheaterState> {
        self.inner.lock().expect("theater mutex poisoned").clone()
    }

    /// Apply `mutator` to the active session in-place and return the new
    /// snapshot. Returns `Err("no active session")` when nothing is running.
    pub fn mutate<F: FnOnce(&mut TheaterState)>(&self, mutator: F) -> SessionResult<TheaterState> {
        let mut guard = self.inner.lock().expect("theater mutex poisoned");
        match guard.as_mut() {
            Some(state) => {
                mutator(state);
                Ok(state.clone())
            }
            None => Err("no active session"),
        }
    }

    // ---- High-level convenience wrappers (used by Tauri commands) ----

    pub fn next_page(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::next)
    }

    pub fn prev_page(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::prev)
    }

    pub fn jump(&self, page: u32) -> SessionResult<TheaterState> {
        self.mutate(|s| s.jump(page))
    }

    pub fn toggle_blackout(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::toggle_blackout)
    }

    pub fn toggle_whiteout(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::toggle_whiteout)
    }

    pub fn toggle_laser(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::toggle_laser)
    }

    pub fn toggle_ink(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::toggle_ink)
    }

    pub fn toggle_spotlight(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::toggle_spotlight)
    }

    pub fn push_stroke(&self, stroke: InkStroke) -> SessionResult<TheaterState> {
        self.mutate(|s| s.push_stroke(stroke))
    }

    pub fn undo_stroke(&self) -> SessionResult<TheaterState> {
        self.mutate(|s| {
            s.undo_stroke();
        })
    }

    pub fn clear_strokes(&self) -> SessionResult<TheaterState> {
        self.mutate(TheaterState::clear_strokes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> TheaterManager {
        TheaterManager::new()
    }

    #[test]
    fn empty_manager_has_no_snapshot() {
        assert!(mgr().snapshot().is_none());
    }

    #[test]
    fn start_records_state() {
        let m = mgr();
        let snap = m.start(PathBuf::from("/tmp/a.pdf"), 10);
        assert_eq!(snap.current_page, 1);
        assert_eq!(snap.total_pages, 10);
        assert!(m.snapshot().is_some());
    }

    #[test]
    fn start_replaces_previous_session() {
        let m = mgr();
        m.start(PathBuf::from("/tmp/a.pdf"), 10);
        let snap = m.start(PathBuf::from("/tmp/b.pdf"), 3);
        assert_eq!(snap.total_pages, 3);
        assert_eq!(m.snapshot().unwrap().path, PathBuf::from("/tmp/b.pdf"));
    }

    #[test]
    fn end_returns_final_snapshot_and_clears() {
        let m = mgr();
        m.start(PathBuf::from("/tmp/a.pdf"), 10);
        m.next_page().unwrap();
        let final_snap = m.end().expect("had session");
        assert_eq!(final_snap.current_page, 2);
        assert!(m.snapshot().is_none());
        assert!(m.end().is_none());
    }

    #[test]
    fn mutations_without_session_error_out() {
        let m = mgr();
        assert!(m.next_page().is_err());
        assert!(m.prev_page().is_err());
        assert!(m.jump(2).is_err());
        assert!(m.toggle_blackout().is_err());
        assert!(m.toggle_whiteout().is_err());
        assert!(m.toggle_laser().is_err());
        assert!(m.toggle_ink().is_err());
        assert!(m.toggle_spotlight().is_err());
        assert!(m.undo_stroke().is_err());
        assert!(m.clear_strokes().is_err());
    }

    #[test]
    fn navigation_returns_updated_snapshots() {
        let m = mgr();
        m.start(PathBuf::from("/tmp/a.pdf"), 5);
        let s = m.next_page().unwrap();
        assert_eq!(s.current_page, 2);
        let s = m.next_page().unwrap();
        assert_eq!(s.current_page, 3);
        let s = m.prev_page().unwrap();
        assert_eq!(s.current_page, 2);
        let s = m.jump(99).unwrap();
        assert_eq!(s.current_page, 5);
    }

    #[test]
    fn blackout_whiteout_mutual_exclusion_via_manager() {
        let m = mgr();
        m.start(PathBuf::from("/tmp/a.pdf"), 5);
        let s = m.toggle_blackout().unwrap();
        assert!(s.blackout);
        let s = m.toggle_whiteout().unwrap();
        assert!(s.whiteout);
        assert!(!s.blackout);
    }

    #[test]
    fn ink_strokes_persist_across_mutations() {
        let m = mgr();
        m.start(PathBuf::from("/tmp/a.pdf"), 5);
        let mut stroke = InkStroke::new(1, "#ff3b30", 2.0);
        stroke.push(0.1, 0.1);
        stroke.push(0.2, 0.2);
        let s = m.push_stroke(stroke).unwrap();
        assert_eq!(s.ink_strokes.len(), 1);
        // Page jump should NOT discard strokes.
        let s = m.jump(3).unwrap();
        assert_eq!(s.ink_strokes.len(), 1);
        let s = m.undo_stroke().unwrap();
        assert!(s.ink_strokes.is_empty());
    }

    #[test]
    fn clear_strokes_via_manager() {
        let m = mgr();
        m.start(PathBuf::from("/tmp/a.pdf"), 5);
        for _ in 0..3 {
            let mut k = InkStroke::new(1, "#000", 2.0);
            k.push(0.0, 0.0);
            k.push(0.5, 0.5);
            m.push_stroke(k).unwrap();
        }
        let s = m.clear_strokes().unwrap();
        assert!(s.ink_strokes.is_empty());
    }
}
