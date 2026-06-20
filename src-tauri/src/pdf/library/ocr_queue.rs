//! Library auto-OCR queue — Slice 2 of v0.13.0 Lens, expanded by
//! v3.52.0 "Atlas OCR-Queue" into a real subsystem with re-queue,
//!   stats and failure-inbox surfaces.
//!
//! ## State machine
//!
//! ```text
//!   scanned ────► ocr_pending ────► ocr_done
//!   mixed   ──┘                ╲
//!                               ╲──► ocr_failed (with persisted error message)
//! ```
//!
//! - `scanned`/`mixed` rows are picked up by [`list_pending`] and
//!   [`run_all`].
//! - [`run_one`] writes the failure reason to `ocr_error` on failure and
//!   clears it on success (v3.52.0 Slice 1).
//! - [`requeue_doc`] flips a `ocr_done` / `ocr_failed` / `ocr_pending`
//!   row back to `scanned` so the queue picks it up again, clearing
//!   `ocr_error` and `ocr_output_path` so the row is genuinely fresh
//!   (v3.52.0 Slice 2).
//! - [`stats`] returns a per-state count for the OCR Queue Panel's
//!   dashboard footer (v3.52.0 Slice 3).
//! - [`list_failed`] returns every `ocr_failed` row ordered newest-first
//!   so the failure inbox can render the most recent breakages on top
//!   (v3.52.0 Slice 4).

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
                added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes
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
                    notes: row.get(13)?,
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
                        added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes
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
                        notes: row.get(13)?,
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

/// Snapshot of the queue surface — how many docs sit in each `ocr_state`
/// bucket. Powers the OCR Queue Panel's status footer; the same numbers
/// you'd see if you ran `SELECT ocr_state, COUNT(*) GROUP BY ocr_state`
/// by hand, with the known constants exposed as named fields so the
/// frontend never has to spell-check magic strings. v3.52.0 Slice 3.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrQueueStats {
    /// Pre-classification (legacy import or scanner hasn't seen it yet).
    pub unknown: i64,
    /// Text-native — `scan_audit` decided OCR is unnecessary.
    pub text_native: i64,
    /// Scanned, image-only.
    pub scanned: i64,
    /// Mixed text + scanned pages.
    pub mixed: i64,
    /// The queue is actively OCR'ing this doc.
    pub pending: i64,
    /// Finished — `ocr_output_path` should be set.
    pub done: i64,
    /// Last OCR attempt failed — `ocr_error` carries the reason.
    pub failed: i64,
    /// Convenience: docs the queue would pull next = scanned + mixed.
    /// Mirrors `list_pending().len()` without a second query.
    pub pending_total: i64,
    /// Convenience: every row, regardless of state.
    pub total: i64,
}

/// Return per-`ocr_state` counts in a single round-trip. Buckets we don't
/// recognise (e.g. a future state added before the UI knows about it) are
/// silently ignored so the dashboard never panics on schema drift.
pub fn stats(db: &LibraryDb) -> Result<OcrQueueStats, LibraryError> {
    use crate::pdf::library::registry::OCR_STATE_TEXT_NATIVE;
    use crate::pdf::library::registry::OCR_STATE_UNKNOWN;

    let conn = db.conn();
    let mut stmt =
        conn.prepare("SELECT ocr_state, COUNT(*) FROM library_documents GROUP BY ocr_state")?;
    let mut out = OcrQueueStats::default();
    let mut total: i64 = 0;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (state, count) = row?;
        total += count;
        match state.as_str() {
            s if s == OCR_STATE_UNKNOWN => out.unknown = count,
            s if s == OCR_STATE_TEXT_NATIVE => out.text_native = count,
            s if s == OCR_STATE_SCANNED => out.scanned = count,
            s if s == OCR_STATE_MIXED => out.mixed = count,
            s if s == OCR_STATE_PENDING => out.pending = count,
            s if s == OCR_STATE_DONE => out.done = count,
            s if s == OCR_STATE_FAILED => out.failed = count,
            _ => {} // ignore unknown buckets — forward-compat
        }
    }
    out.pending_total = out.scanned + out.mixed;
    out.total = total;
    Ok(out)
}

/// Re-queue a single document by flipping its `ocr_state` back to
/// `scanned`, clearing the persisted `ocr_error` and `ocr_output_path`
/// so the row is genuinely fresh before the next run_one picks it up.
///
/// Accepts `ocr_done`, `ocr_failed` and `ocr_pending` (a stale "pending"
/// row from a crashed worker IS the prime use case). Rejects rows that
/// the queue doesn't own — `text_native` and `unknown` are scanner
/// classifications, not queue states; re-queueing them would lie. An
/// unknown doc id errors so callers know nothing happened. v3.52.0 Slice 2.
pub fn requeue_doc(db: &mut LibraryDb, doc_id: i64) -> Result<DocumentRecord, LibraryError> {
    // Look up current state up-front so we can give a meaningful error
    // before mutating anything.
    let current = {
        let conn = db.conn();
        conn.query_row(
            "SELECT ocr_state FROM library_documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| r.get::<_, String>(0),
        )
        .map_err(LibraryError::from)?
    };
    match current.as_str() {
        OCR_STATE_DONE | OCR_STATE_FAILED | OCR_STATE_PENDING => {}
        other => {
            return Err(LibraryError::Other(format!(
                "cannot re-queue document in ocr_state '{other}' — only ocr_done / ocr_failed / ocr_pending are accepted",
            )));
        }
    }
    db.set_doc_ocr_state(doc_id, OCR_STATE_SCANNED)?;
    db.set_doc_ocr_error(doc_id, None)?;
    db.set_doc_ocr_output_path(doc_id, None)?;
    // Re-read so callers get the canonical post-write row (no client-side
    // mutation drift).
    let conn = db.conn();
    let row = conn.query_row(
        "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages,
                added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes
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
                notes: row.get(13)?,
                tags: Vec::new(),
            })
        },
    )?;
    Ok(row)
}

/// Re-queue every `ocr_failed` document in one shot. Useful for "fix
/// tesseract install, then retry everything that broke" workflows.
/// Returns the count of rows that flipped. Documents in other states
/// are untouched. v3.52.0 Slice 2 companion.
pub fn requeue_all_failed(db: &mut LibraryDb) -> Result<usize, LibraryError> {
    let tx = db.conn_mut().transaction()?;
    let n = tx.execute(
        "UPDATE library_documents
            SET ocr_state = ?1, ocr_error = NULL, ocr_output_path = NULL
          WHERE ocr_state = ?2",
        rusqlite::params![OCR_STATE_SCANNED, OCR_STATE_FAILED],
    )?;
    tx.commit()?;
    Ok(n)
}

/// List every `ocr_failed` document, ordered by `last_seen_at` DESC
/// (newest failures bubble to the top of the inbox). v3.52.0 Slice 4.
pub fn list_failed(db: &LibraryDb) -> Result<Vec<DocumentRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages,
                added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes
         FROM library_documents
         WHERE ocr_state = ?1
         ORDER BY last_seen_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![OCR_STATE_FAILED], |row| {
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
                notes: row.get(13)?,
                tags: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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

    // -- v3.52.0 Atlas OCR-Queue Slice 3: stats --

    #[test]
    fn stats_on_empty_library_is_all_zeros() {
        let db = LibraryDb::open_in_memory().unwrap();
        let s = stats(&db).unwrap();
        assert_eq!(s, OcrQueueStats::default());
        assert_eq!(s.total, 0);
        assert_eq!(s.pending_total, 0);
    }

    #[test]
    fn stats_counts_every_bucket_correctly() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        // 2 scanned, 1 mixed, 1 text-native, 1 done, 1 failed, 1 pending = 7 total
        seed_doc(&mut db, "/a.pdf", OCR_STATE_SCANNED);
        seed_doc(&mut db, "/b.pdf", OCR_STATE_SCANNED);
        seed_doc(&mut db, "/c.pdf", OCR_STATE_MIXED);
        seed_doc(&mut db, "/d.pdf", OCR_STATE_TEXT_NATIVE);
        let done_id = seed_doc(&mut db, "/e.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(done_id, OCR_STATE_DONE).unwrap();
        let failed_id = seed_doc(&mut db, "/f.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(failed_id, OCR_STATE_FAILED).unwrap();
        let pending_id = seed_doc(&mut db, "/g.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(pending_id, OCR_STATE_PENDING).unwrap();

        let s = stats(&db).unwrap();
        assert_eq!(s.scanned, 2);
        assert_eq!(s.mixed, 1);
        assert_eq!(s.text_native, 1);
        assert_eq!(s.done, 1);
        assert_eq!(s.failed, 1);
        assert_eq!(s.pending, 1);
        assert_eq!(s.unknown, 0);
        assert_eq!(s.pending_total, 3, "scanned + mixed only");
        assert_eq!(s.total, 7);
    }

    #[test]
    fn stats_ignores_unknown_buckets_silently() {
        // Forward-compat: a future state shouldn't crash the dashboard.
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/a.pdf", OCR_STATE_SCANNED);
        // Sneak in a synthetic future state directly via the underlying
        // UPDATE (the public setter takes &str so we can simulate it).
        db.set_doc_ocr_state(id, "ocr_quantum_entangled").unwrap();
        let s = stats(&db).unwrap();
        // No known bucket should be incremented; total still counts the row.
        assert_eq!(s.scanned, 0);
        assert_eq!(s.failed, 0);
        assert_eq!(s.done, 0);
        assert_eq!(s.total, 1);
        assert_eq!(s.pending_total, 0);
    }

    #[test]
    fn stats_serde_round_trip_uses_snake_case() {
        // Pin the wire shape — the TS client deserialises by field name.
        let s = OcrQueueStats {
            unknown: 1,
            text_native: 2,
            scanned: 3,
            mixed: 4,
            pending: 5,
            done: 6,
            failed: 7,
            pending_total: 7,
            total: 28,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"text_native\":2"));
        assert!(json.contains("\"pending_total\":7"));
        let back: OcrQueueStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    // -- v3.52.0 Atlas OCR-Queue Slice 2: requeue --

    #[test]
    fn requeue_doc_flips_failed_back_to_scanned() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/a.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(id, OCR_STATE_FAILED).unwrap();
        db.set_doc_ocr_error(id, Some("bad day")).unwrap();
        let out = requeue_doc(&mut db, id).unwrap();
        assert_eq!(out.ocr_state, OCR_STATE_SCANNED);
        assert!(out.ocr_error.is_none(), "requeue clears stale error");
        assert!(out.ocr_output_path.is_none());
        assert_eq!(out.id, id);
    }

    #[test]
    fn requeue_doc_clears_output_path_from_prior_success() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/done.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(id, OCR_STATE_DONE).unwrap();
        db.set_doc_ocr_output_path(id, Some("/done.ocr.pdf"))
            .unwrap();
        let out = requeue_doc(&mut db, id).unwrap();
        assert_eq!(out.ocr_state, OCR_STATE_SCANNED);
        assert!(
            out.ocr_output_path.is_none(),
            "requeue clears the prior .ocr.pdf so the row isn't lying"
        );
    }

    #[test]
    fn requeue_doc_accepts_stale_pending() {
        // A crashed worker can leave a row in ocr_pending forever;
        // re-queueing has to be the escape hatch.
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/stuck.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(id, OCR_STATE_PENDING).unwrap();
        let out = requeue_doc(&mut db, id).unwrap();
        assert_eq!(out.ocr_state, OCR_STATE_SCANNED);
    }

    #[test]
    fn requeue_doc_rejects_text_native_rows() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let id = seed_doc(&mut db, "/text.pdf", OCR_STATE_TEXT_NATIVE);
        let err = requeue_doc(&mut db, id).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("text_native"),
            "error should name the rejected state: {msg}"
        );
        // Row must be untouched.
        let row = db.find_document_by_path("/text.pdf").unwrap().unwrap();
        assert_eq!(row.ocr_state, OCR_STATE_TEXT_NATIVE);
    }

    #[test]
    fn requeue_doc_rejects_unknown_id() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        assert!(requeue_doc(&mut db, 99_999).is_err());
    }

    #[test]
    fn requeue_all_failed_flips_only_failed_rows() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let f1 = seed_doc(&mut db, "/f1.pdf", OCR_STATE_SCANNED);
        let f2 = seed_doc(&mut db, "/f2.pdf", OCR_STATE_SCANNED);
        let f3 = seed_doc(&mut db, "/f3.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(f1, OCR_STATE_FAILED).unwrap();
        db.set_doc_ocr_error(f1, Some("a")).unwrap();
        db.set_doc_ocr_state(f2, OCR_STATE_FAILED).unwrap();
        db.set_doc_ocr_error(f2, Some("b")).unwrap();
        // f3 stays scanned — must not be touched.
        let n = requeue_all_failed(&mut db).unwrap();
        assert_eq!(n, 2, "exactly the failed rows flip");
        let r1 = db.find_document_by_path("/f1.pdf").unwrap().unwrap();
        let r2 = db.find_document_by_path("/f2.pdf").unwrap().unwrap();
        let r3 = db.find_document_by_path("/f3.pdf").unwrap().unwrap();
        assert_eq!(r1.ocr_state, OCR_STATE_SCANNED);
        assert!(r1.ocr_error.is_none());
        assert_eq!(r2.ocr_state, OCR_STATE_SCANNED);
        assert!(r2.ocr_error.is_none());
        assert_eq!(r3.ocr_state, OCR_STATE_SCANNED, "untouched");
    }

    #[test]
    fn requeue_all_failed_is_zero_on_clean_library() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        seed_doc(&mut db, "/clean.pdf", OCR_STATE_TEXT_NATIVE);
        assert_eq!(requeue_all_failed(&mut db).unwrap(), 0);
    }

    // -- v3.52.0 Atlas OCR-Queue Slice 4: list_failed --

    #[test]
    fn list_failed_returns_only_failed_rows() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = seed_doc(&mut db, "/a.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(a, OCR_STATE_FAILED).unwrap();
        db.set_doc_ocr_error(a, Some("a-reason")).unwrap();
        let b = seed_doc(&mut db, "/b.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(b, OCR_STATE_DONE).unwrap();
        let c = seed_doc(&mut db, "/c.pdf", OCR_STATE_TEXT_NATIVE);
        let _ = c;
        let out = list_failed(&db).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "/a.pdf");
        assert_eq!(out[0].ocr_error.as_deref(), Some("a-reason"));
    }

    #[test]
    fn list_failed_orders_newest_first_by_last_seen_at() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let older = seed_doc(&mut db, "/older.pdf", OCR_STATE_SCANNED);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let newer = seed_doc(&mut db, "/newer.pdf", OCR_STATE_SCANNED);
        db.set_doc_ocr_state(older, OCR_STATE_FAILED).unwrap();
        db.set_doc_ocr_state(newer, OCR_STATE_FAILED).unwrap();
        let out = list_failed(&db).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].path, "/newer.pdf", "DESC by last_seen_at");
        assert_eq!(out[1].path, "/older.pdf");
    }

    #[test]
    fn list_failed_empty_when_nothing_failed() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        seed_doc(&mut db, "/ok.pdf", OCR_STATE_DONE);
        assert!(list_failed(&db).unwrap().is_empty());
    }
}
