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
}
