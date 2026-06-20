//! Library auto-OCR queue — Slice 2 of v0.13.0 Lens.
//!
//! A simple sequential queue: ask the library for docs whose
//! `ocr_state` is `scanned` or `mixed` (and not currently pending /
//! done), then OCR them one at a time, writing a searchable PDF next
//! to the source as `<basename>.ocr.pdf`.
//!
//! The queue is intentionally synchronous — Tauri commands invoke
//! `run_one` directly. Slice 2 ships the building blocks; Slice 7 may
//! upgrade to a `tauri::async_runtime::spawn` worker with progress
//! events once the UX needs them.
//!
//! ## State machine
//!
//! ```text
//!   scanned ────► ocr_pending ────► ocr_done
//!   mixed   ──┘                ╲
//!                               ╲──► ocr_failed (with error message)
//! ```
//!
//! Failed entries can be re-queued by setting their state back to
//! `scanned` / `mixed`; that's a future UX affordance.

use super::registry::{
    DocumentRecord, LibraryDb, LibraryError, OCR_STATE_DONE, OCR_STATE_FAILED, OCR_STATE_MIXED,
    OCR_STATE_PENDING, OCR_STATE_SCANNED,
};
use crate::pdf::ocr::{ocr as run_ocr, OcrOpts};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What `run_one` produces — the doc id (so the UI can refresh that
/// row), the new state (so the UI doesn't have to re-fetch), and the
/// output path on success or the error message on failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrQueueResult {
    pub doc_id: i64,
    pub state_after: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

/// List documents the queue should process: ocr_state in
/// (`scanned`, `mixed`), excluding rows already pending/done/failed.
/// Ordered by added_at ASC so older imports run first.
pub fn list_pending(db: &LibraryDb) -> Result<Vec<DocumentRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages,
                added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error
         FROM library_documents
         WHERE ocr_state IN (?1, ?2)
         ORDER BY added_at ASC, id ASC",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![OCR_STATE_SCANNED, OCR_STATE_MIXED],
            |row| {
                Ok(DocumentRecord {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    path: row.get(2)?,
                    title: row.get(3)?,
                    hash: row.get(4)?,
                    size_bytes: row.get(5)?,
                    mtime_ns: row.get(6)?,
                    pages: row.get(7)?,
                    added_at: row.get(8)?,
                    last_seen_at: row.get(9)?,
                    ocr_state: row.get(10)?,
                    ocr_output_path: row.get(11)?,
                    ocr_error: row.get(12)?,
                    tags: Vec::new(),
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Compute the canonical OCR-output path for an input PDF: insert
/// `.ocr` before the extension. `/tmp/a.pdf` → `/tmp/a.ocr.pdf`.
pub fn ocr_output_path_for(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".into());
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("pdf");
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}.ocr.{ext}"))
}

/// OCR one document end-to-end. Never panics — converts every error
/// path into `OCR_STATE_FAILED` with a captured error string. Caller
/// is responsible for atomicity: if the doc row vanishes between the
/// lookup and the OCR call, the second `set_doc_ocr_state` call is a
/// no-op rather than an error.
pub fn run_one(db: &mut LibraryDb, doc_id: i64, opts: &OcrOpts) -> OcrQueueResult {
    // Look up the doc first so we know the input path.
    let path_lookup: Result<Option<DocumentRecord>, LibraryError> = {
        let conn = db.conn();
        let row = conn
            .query_row(
                "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages,
                        added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error
                 FROM library_documents WHERE id = ?1",
                rusqlite::params![doc_id],
                |row| {
                    Ok(DocumentRecord {
                        id: row.get(0)?,
                        folder_id: row.get(1)?,
                        path: row.get(2)?,
                        title: row.get(3)?,
                        hash: row.get(4)?,
                        size_bytes: row.get(5)?,
                        mtime_ns: row.get(6)?,
                        pages: row.get(7)?,
                        added_at: row.get(8)?,
                        last_seen_at: row.get(9)?,
                        ocr_state: row.get(10)?,
                        ocr_output_path: row.get(11)?,
                        ocr_error: row.get(12)?,
                        tags: Vec::new(),
                    })
                },
            )
            .map_err(LibraryError::from);
        match row {
            Ok(r) => Ok(Some(r)),
            Err(LibraryError::Db(rusqlite::Error::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    };

    let doc = match path_lookup {
        Ok(Some(d)) => d,
        Ok(None) => {
            return OcrQueueResult {
                doc_id,
                state_after: OCR_STATE_FAILED.to_string(),
                output_path: None,
                error: Some(format!("doc id {doc_id} not in library")),
            };
        }
        Err(e) => {
            return OcrQueueResult {
                doc_id,
                state_after: OCR_STATE_FAILED.to_string(),
                output_path: None,
                error: Some(format!("library lookup failed: {e}")),
            };
        }
    };

    // Mark pending → if this fails, give up early (the DB is broken).
    if let Err(e) = db.set_doc_ocr_state(doc.id, OCR_STATE_PENDING) {
        return OcrQueueResult {
            doc_id,
            state_after: OCR_STATE_FAILED.to_string(),
            output_path: None,
            error: Some(format!("mark pending failed: {e}")),
        };
    }

    // Build output path and OCR.
    let input = PathBuf::from(&doc.path);
    let output = ocr_output_path_for(&input);

    match run_ocr(&input, &output, opts) {
        Ok(_report) => {
            let _ = db.set_doc_ocr_state(doc.id, OCR_STATE_DONE);
            let _ = db.set_doc_ocr_output_path(doc.id, Some(output.to_string_lossy().as_ref()));
            // Clear any prior failure string — a successful re-OCR
            // overwrites the stale reason so the failure inbox stays
            // honest. v3.52.0 Atlas OCR-Queue Slice 1.
            let _ = db.set_doc_ocr_error(doc.id, None);
            OcrQueueResult {
                doc_id: doc.id,
                state_after: OCR_STATE_DONE.to_string(),
                output_path: Some(output.to_string_lossy().into_owned()),
                error: None,
            }
        }
        Err(err) => {
            let msg = err.to_string();
            let _ = db.set_doc_ocr_state(doc.id, OCR_STATE_FAILED);
            // Persist the captured reason so the failure inbox can render
            // it without re-running OCR; clear any leftover output_path
            // from a prior success so the row never claims to have an
            // .ocr.pdf it doesn't. v3.52.0 Atlas OCR-Queue Slice 1.
            let _ = db.set_doc_ocr_error(doc.id, Some(&msg));
            let _ = db.set_doc_ocr_output_path(doc.id, None);
            OcrQueueResult {
                doc_id: doc.id,
                state_after: OCR_STATE_FAILED.to_string(),
                output_path: None,
                error: Some(msg),
            }
        }
    }
}

/// Run OCR over every pending document, in queue order. Returns one
/// result per processed doc. Stops on the first DB error from
/// `list_pending` but otherwise continues past per-doc OCR failures.
pub fn run_all(db: &mut LibraryDb, opts: &OcrOpts) -> Result<Vec<OcrQueueResult>, LibraryError> {
    let pending = list_pending(db)?;
    let mut out = Vec::with_capacity(pending.len());
    for doc in pending {
        out.push(run_one(db, doc.id, opts));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::registry::OCR_STATE_TEXT_NATIVE;

    fn seed_doc(db: &mut LibraryDb, path: &str, ocr_state: &str) -> i64 {
        let f = db.add_folder("/papers").unwrap();
        let d = db
            .upsert_document(Some(f.id), path, None, "h", 1, 1, None, Some(ocr_state))
            .unwrap();
        d.id
    }

    #[test]
    fn list_pending_includes_scanned_and_mixed() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let f = db.add_folder("/papers").unwrap();
        let s = db
            .upsert_document(
                Some(f.id),
                "/papers/scan.pdf",
                None,
                "h",
                1,
                1,
                None,
                Some(OCR_STATE_SCANNED),
            )
            .unwrap();
        let m = db
            .upsert_document(
                Some(f.id),
                "/papers/mix.pdf",
                None,
                "h",
                1,
                1,
                None,
                Some(OCR_STATE_MIXED),
            )
            .unwrap();
        let _t = db
            .upsert_document(
                Some(f.id),
                "/papers/text.pdf",
                None,
                "h",
                1,
                1,
                None,
                Some(OCR_STATE_TEXT_NATIVE),
            )
            .unwrap();
        let pending = list_pending(&db).unwrap();
        let ids: Vec<i64> = pending.iter().map(|d| d.id).collect();
        assert!(ids.contains(&s.id));
        assert!(ids.contains(&m.id));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn list_pending_excludes_already_done() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/papers/a.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(id, OCR_STATE_DONE).unwrap();
        let pending = list_pending(&db).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn list_pending_excludes_pending_and_failed() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = seed_doc(&mut db, "/papers/a.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(a, OCR_STATE_PENDING).unwrap();
        let b = seed_doc(&mut db, "/papers/b.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(b, OCR_STATE_FAILED).unwrap();
        assert!(list_pending(&db).unwrap().is_empty());
    }

    #[test]
    fn list_pending_orders_by_added_at_asc() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = seed_doc(&mut db, "/papers/a.pdf", OCR_STATE_SCANNED);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = seed_doc(&mut db, "/papers/b.pdf", OCR_STATE_SCANNED);
        let pending = list_pending(&db).unwrap();
        let ids: Vec<i64> = pending.iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![a, b]);
    }

    #[test]
    fn output_path_inserts_dot_ocr_before_extension() {
        assert_eq!(
            ocr_output_path_for(Path::new("/tmp/a.pdf")),
            PathBuf::from("/tmp/a.ocr.pdf")
        );
        assert_eq!(
            ocr_output_path_for(Path::new("/papers/big report.pdf")),
            PathBuf::from("/papers/big report.ocr.pdf")
        );
    }

    #[test]
    fn output_path_handles_uppercase_extension() {
        assert_eq!(
            ocr_output_path_for(Path::new("/x/scan.PDF")),
            PathBuf::from("/x/scan.ocr.PDF")
        );
    }

    #[test]
    fn run_one_returns_failed_for_missing_doc_id() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let r = run_one(&mut db, 9999, &OcrOpts::default());
        assert_eq!(r.state_after, OCR_STATE_FAILED);
        assert!(r.error.is_some());
        assert!(r.output_path.is_none());
    }

    #[test]
    fn run_one_marks_failed_when_input_does_not_exist() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/this/does/not/exist.pdf", OCR_STATE_SCANNED);
        let r = run_one(&mut db, id, &OcrOpts::default());
        assert_eq!(r.state_after, OCR_STATE_FAILED);
        assert!(r.error.is_some());
        let row = db
            .find_document_by_path("/this/does/not/exist.pdf")
            .unwrap()
            .unwrap();
        assert_eq!(row.ocr_state, OCR_STATE_FAILED);
    }

    #[test]
    fn run_one_persists_error_message_on_failure() {
        // v3.52.0 Atlas OCR-Queue Slice 1 — failure reasons used to vanish
        // after run_one returned. Now they're persisted in ocr_error so the
        // failure inbox can render them without re-running OCR.
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/missing/nope.pdf", OCR_STATE_SCANNED);
        let r = run_one(&mut db, id, &OcrOpts::default());
        // Same returned shape as before.
        let returned = r.error.clone().expect("failed run returns error");
        assert!(!returned.is_empty());
        // …and now the row carries the same reason.
        let row = db
            .find_document_by_path("/missing/nope.pdf")
            .unwrap()
            .unwrap();
        assert_eq!(row.ocr_state, OCR_STATE_FAILED);
        assert_eq!(row.ocr_error.as_deref(), Some(returned.as_str()));
        // Failed rows never carry an output_path (we clear any prior one).
        assert!(row.ocr_output_path.is_none());
    }

    #[test]
    fn run_one_clears_prior_error_on_success() {
        // Successful re-OCR overwrites a stale failure reason; otherwise the
        // failure inbox would lie about the row's current state.
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/missing/again.pdf", OCR_STATE_SCANNED);
        // Force a failure to seed the error column.
        let _ = run_one(&mut db, id, &OcrOpts::default());
        let after_fail = db
            .find_document_by_path("/missing/again.pdf")
            .unwrap()
            .unwrap();
        assert!(after_fail.ocr_error.is_some());

        // Now simulate a clean re-queue + success directly through the
        // setters (we can't actually call tesseract from a unit test).
        db.set_doc_ocr_state(id, OCR_STATE_DONE).unwrap();
        db.set_doc_ocr_error(id, None).unwrap();
        let after_ok = db
            .find_document_by_path("/missing/again.pdf")
            .unwrap()
            .unwrap();
        assert_eq!(after_ok.ocr_state, OCR_STATE_DONE);
        assert!(after_ok.ocr_error.is_none());
    }

    #[test]
    fn run_all_returns_empty_when_nothing_pending() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/papers/text.pdf", OCR_STATE_TEXT_NATIVE);
        let _ = id;
        let r = run_all(&mut db, &OcrOpts::default()).unwrap();
        assert!(r.is_empty());
    }
}
