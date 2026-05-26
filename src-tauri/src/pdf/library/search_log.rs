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
use std::time::{SystemTime, UNIX_EPOCH};

use super::registry::{LibraryDb, LibraryError};

/// Max retained rows in `library_search_log`.
pub const LOG_CAP: usize = 500;
/// Same-query coalescing window (seconds). Within this window we update
/// the existing row instead of inserting a duplicate.
const DEDUPE_WINDOW_SECS: i64 = 30;

/// One row in `library_search_log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRow {
    pub id: i64,
    pub query: String,
    pub ts: i64,
    pub result_count: i64,
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
}
