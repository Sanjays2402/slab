// Atlas Suggest — rolling log of recent library searches.
//
// Every time the user runs a library search we record (query, ts,
// result_count). The suggestion engine (`folder_suggest`) reads the last
// N rows to propose personal Smart Folders.
//
// We cap the table at 500 rows on every insert so the file never bloats.
// Empty queries are ignored. Same-query within a 30s window deduplicates
// (we update the existing row's ts + result_count instead of inserting).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::registry::{LibraryDb, LibraryError};

/// Max retained rows in `library_search_log`.
pub const LOG_CAP: usize = 500;
/// Same-query coalescing window (seconds). Within this window we update
/// the existing row instead of inserting a duplicate.
const DEDUPE_WINDOW_SECS: i64 = 30;

/// One row in `library_search_log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRow {
    pub id: i64,
    pub query: String,
    pub ts: i64,
    pub result_count: i64,
}

/// Delete every row from `library_search_log`. Used by the search panel's
/// "Clear history" affordance so the user can wipe their recent searches
/// without nuking the rest of the library. Returns the number of rows
/// removed so the caller can decide whether to surface a "Cleared N searches"
/// toast or stay silent on an already-empty log. Idempotent — calling on an
/// empty log returns 0.
pub fn clear(db: &LibraryDb) -> Result<usize, LibraryError> {
    let n = db.conn().execute("DELETE FROM library_search_log", [])?;
    Ok(n)
}

/// Delete a SINGLE row from `library_search_log` by id. Backs the search
/// panel's per-chip delete affordance (an x on each recent-search chip, or
/// Backspace on the focused chip), complementing the all-or-nothing
/// `clear`. Returns true when a row was actually removed so the caller can
/// distinguish a real delete from a stale id (a chip already gone after a
/// concurrent clear). Leaves every other row — and the suggestion
/// dismissals next door — untouched.
pub fn delete_one(db: &LibraryDb, id: i64) -> Result<bool, LibraryError> {
    let n = db
        .conn()
        .execute("DELETE FROM library_search_log WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Record on a raw connection (used inline by `search::search`, which
/// only has a `&Connection`). Errors are intentionally swallowed by the
/// caller — logging is best-effort.
pub fn record_conn(conn: &Connection, query: &str, result_count: i64) -> Result<(), LibraryError> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(());
    }
    let now = now_unix();
    let recent: Option<i64> = conn
        .query_row(
            "SELECT id FROM library_search_log \
             WHERE query = ?1 AND ts >= ?2 \
             ORDER BY ts DESC LIMIT 1",
            params![q, now - DEDUPE_WINDOW_SECS],
            |row| row.get(0),
        )
        .ok();
    if let Some(id) = recent {
        conn.execute(
            "UPDATE library_search_log SET ts = ?1, result_count = ?2 WHERE id = ?3",
            params![now, result_count, id],
        )?;
    } else {
        conn.execute(
            "INSERT INTO library_search_log (query, ts, result_count) VALUES (?1, ?2, ?3)",
            params![q, now, result_count],
        )?;
        conn.execute(
            "DELETE FROM library_search_log WHERE id NOT IN \
             (SELECT id FROM library_search_log ORDER BY ts DESC LIMIT ?1)",
            params![LOG_CAP as i64],
        )?;
    }
    Ok(())
}

/// Record a library search. Empty / whitespace-only queries are ignored.
/// Within a 30s window the same query updates the existing row rather than
/// inserting a duplicate. Enforces the 500-row cap on every insert.
pub fn record(db: &LibraryDb, query: &str, result_count: i64) -> Result<(), LibraryError> {
    record_conn(db.conn(), query, result_count)
}

/// Return the most recent N queries, newest first.
pub fn recent_queries(db: &LibraryDb, limit: usize) -> Result<Vec<QueryRow>, LibraryError> {
    let mut stmt = db.conn().prepare(
        "SELECT id, query, ts, result_count FROM library_search_log \
         ORDER BY ts DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |r| {
            Ok(QueryRow {
                id: r.get(0)?,
                query: r.get(1)?,
                ts: r.get(2)?,
                result_count: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Total number of rows currently in the log.
pub fn count(db: &LibraryDb) -> Result<i64, LibraryError> {
    let n: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM library_search_log", [], |r| r.get(0))?;
    Ok(n)
}

/// Mark a suggestion cluster as dismissed so the engine doesn't re-emit it.
pub fn dismiss(db: &LibraryDb, cluster_hash: &str) -> Result<(), LibraryError> {
    db.conn().execute(
        "INSERT OR REPLACE INTO library_suggestion_dismissed (cluster_hash, ts) VALUES (?1, ?2)",
        params![cluster_hash, now_unix()],
    )?;
    Ok(())
}

/// Has this cluster been dismissed?
pub fn is_dismissed(db: &LibraryDb, cluster_hash: &str) -> Result<bool, LibraryError> {
    let n: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM library_suggestion_dismissed WHERE cluster_hash = ?1",
        params![cluster_hash],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().unwrap()
    }

    #[test]
    fn record_and_list_roundtrip() {
        let d = db();
        record(&d, "alpha", 1).unwrap();
        record(&d, "beta", 2).unwrap();
        record(&d, "gamma", 3).unwrap();
        let rows = recent_queries(&d, 10).unwrap();
        assert_eq!(rows.len(), 3);
        // newest-first, but ts may collide on fast machines — accept any
        // order containing all three.
        let mut qs: Vec<_> = rows.iter().map(|r| r.query.clone()).collect();
        qs.sort();
        assert_eq!(qs, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn empty_query_ignored() {
        let d = db();
        record(&d, "", 0).unwrap();
        record(&d, "   ", 0).unwrap();
        assert_eq!(count(&d).unwrap(), 0);
    }

    #[test]
    fn cap_enforced_at_500() {
        let d = db();
        // Insert 510 distinct queries to bypass the dedupe coalescer.
        for i in 0..510 {
            record(&d, &format!("q{}", i), 0).unwrap();
        }
        assert_eq!(count(&d).unwrap(), LOG_CAP as i64);
    }

    #[test]
    fn dismiss_persists() {
        let d = db();
        assert!(!is_dismissed(&d, "abc").unwrap());
        dismiss(&d, "abc").unwrap();
        assert!(is_dismissed(&d, "abc").unwrap());
        assert!(!is_dismissed(&d, "xyz").unwrap());
    }

    #[test]
    fn recent_queries_respects_limit() {
        let d = db();
        for i in 0..50 {
            record(&d, &format!("q{}", i), 0).unwrap();
        }
        let rows = recent_queries(&d, 10).unwrap();
        assert_eq!(rows.len(), 10);
    }

    #[test]
    fn dedupe_within_window_coalesces() {
        let d = db();
        record(&d, "invoice", 5).unwrap();
        record(&d, "invoice", 6).unwrap();
        record(&d, "invoice", 7).unwrap();
        assert_eq!(count(&d).unwrap(), 1);
        let rows = recent_queries(&d, 10).unwrap();
        assert_eq!(rows[0].result_count, 7);
    }

    #[test]
    fn clear_removes_every_row_and_returns_count() {
        let d = db();
        record(&d, "alpha", 1).unwrap();
        record(&d, "beta", 2).unwrap();
        record(&d, "gamma", 3).unwrap();
        assert_eq!(count(&d).unwrap(), 3);
        let removed = clear(&d).unwrap();
        assert_eq!(removed, 3, "clear must report rows removed");
        assert_eq!(count(&d).unwrap(), 0);
        assert!(recent_queries(&d, 10).unwrap().is_empty());
    }

    #[test]
    fn clear_on_empty_log_is_zero_noop() {
        let d = db();
        assert_eq!(clear(&d).unwrap(), 0);
        // And a second clear after a partial wipe also a no-op.
        record(&d, "x", 0).unwrap();
        assert_eq!(clear(&d).unwrap(), 1);
        assert_eq!(clear(&d).unwrap(), 0);
    }

    #[test]
    fn delete_one_removes_only_the_target_row() {
        let d = db();
        record(&d, "alpha", 1).unwrap();
        record(&d, "beta", 2).unwrap();
        record(&d, "gamma", 3).unwrap();
        // Find beta's id so we can drop exactly it.
        let rows = recent_queries(&d, 10).unwrap();
        let beta = rows.iter().find(|r| r.query == "beta").unwrap();
        let removed = delete_one(&d, beta.id).unwrap();
        assert!(removed, "delete_one must report a real removal");
        assert_eq!(count(&d).unwrap(), 2, "only one row removed");
        let after = recent_queries(&d, 10).unwrap();
        let mut qs: Vec<_> = after.iter().map(|r| r.query.clone()).collect();
        qs.sort();
        assert_eq!(qs, vec!["alpha", "gamma"], "alpha + gamma survive");
    }

    #[test]
    fn delete_one_on_missing_id_is_false_noop() {
        let d = db();
        record(&d, "alpha", 1).unwrap();
        // An id that never existed.
        assert!(!delete_one(&d, 999_999).unwrap(), "missing id -> false");
        assert_eq!(count(&d).unwrap(), 1, "no row removed for a stale id");
        // Deleting the same row twice: first true, second false.
        let id = recent_queries(&d, 1).unwrap()[0].id;
        assert!(delete_one(&d, id).unwrap(), "first delete removes it");
        assert!(
            !delete_one(&d, id).unwrap(),
            "second delete is a false no-op"
        );
        assert_eq!(count(&d).unwrap(), 0);
    }

    #[test]
    fn clear_does_not_touch_dismissals() {
        // The Atlas suggestion engine's dismissed-cluster table lives next door
        // (library_suggestion_dismissed); clearing the *search log* must leave
        // user-curated dismissals intact so suggestions don't reappear.
        let d = db();
        dismiss(&d, "cluster-keep-me").unwrap();
        record(&d, "alpha", 1).unwrap();
        clear(&d).unwrap();
        assert!(is_dismissed(&d, "cluster-keep-me").unwrap());
    }

    #[test]
    fn query_row_serde_roundtrip_uses_snake_case() {
        // The Tauri command for recent_searches returns Vec<QueryRow> across
        // the wire; pin the JSON shape (snake_case keys + i64 values) so the
        // TS client doesn't drift.
        let row = QueryRow {
            id: 7,
            query: "indemnification".into(),
            ts: 1_700_000_000,
            result_count: 42,
        };
        let json = serde_json::to_string(&row).unwrap();
        assert!(json.contains("\"id\":7"));
        assert!(json.contains("\"query\":\"indemnification\""));
        assert!(json.contains("\"ts\":1700000000"));
        assert!(json.contains("\"result_count\":42"));
        let back: QueryRow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
    }
}
