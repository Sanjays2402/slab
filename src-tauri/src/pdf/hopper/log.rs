//! Hopper run log — append-only history of every pipeline invocation.
//!
//! Lives in the same sqlite DB as the registry, but managed via a
//! separate struct so the two can be locked independently. Used by the
//! `HopperPanel` "Live log" surface and by `slab_hopper_list_runs`.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Outcome of a single pipeline invocation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Success,
    Failed,
    Skipped,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Success => "success",
            RunStatus::Failed => "failed",
            RunStatus::Skipped => "skipped",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "success" => RunStatus::Success,
            "skipped" => RunStatus::Skipped,
            _ => RunStatus::Failed,
        }
    }
}

/// One persisted row in the `runs` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    pub id: i64,
    pub watch_id: i64,
    pub input_path: String,
    pub output_path: Option<String>,
    pub status: RunStatus,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub ai_title: Option<String>,
    pub started_at: String,
}

/// Append-only log over the `runs` sqlite table.
pub struct HopperLog {
    conn: Connection,
}

impl HopperLog {
    /// Open (or create) the log table at `path`. Safe to call against
    /// the same DB file the registry uses — different table.
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runs (
                id           INTEGER PRIMARY KEY,
                watch_id     INTEGER NOT NULL,
                input_path   TEXT NOT NULL,
                output_path  TEXT,
                status       TEXT NOT NULL,
                error        TEXT,
                duration_ms  INTEGER NOT NULL,
                ai_title     TEXT,
                started_at   TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS runs_watch_idx ON runs(watch_id);
            CREATE INDEX IF NOT EXISTS runs_started_idx ON runs(started_at DESC);
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Append a new run record. Returns the assigned id.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        watch_id: i64,
        input_path: &str,
        output_path: Option<&str>,
        status: RunStatus,
        error: Option<&str>,
        duration_ms: i64,
        ai_title: Option<&str>,
    ) -> rusqlite::Result<i64> {
        let started = unix_now();
        self.conn.execute(
            "INSERT INTO runs \
                (watch_id, input_path, output_path, status, error, \
                 duration_ms, ai_title, started_at) \
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                watch_id,
                input_path,
                output_path,
                status.as_str(),
                error,
                duration_ms,
                ai_title,
                started,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Fetch the most recent `limit` rows, newest first.
    pub fn list_recent(&self, limit: i64) -> rusqlite::Result<Vec<RunRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, watch_id, input_path, output_path, status, error, \
                    duration_ms, ai_title, started_at \
             FROM runs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let status_str: String = row.get(4)?;
    Ok(RunRecord {
        id: row.get(0)?,
        watch_id: row.get(1)?,
        input_path: row.get(2)?,
        output_path: row.get(3)?,
        status: RunStatus::parse(&status_str),
        error: row.get(5)?,
        duration_ms: row.get(6)?,
        ai_title: row.get(7)?,
        started_at: row.get(8)?,
    })
}

fn unix_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hopper.db");
        (dir, path)
    }

    #[test]
    fn record_and_list_round_trip() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        let id = log
            .record(
                7,
                "/in/Acme NDA.pdf",
                Some("/out/2026-05-24_NDA_Acme.pdf"),
                RunStatus::Success,
                None,
                1234,
                Some("NDA Acme Corp"),
            )
            .unwrap();
        assert!(id > 0);

        let recent = log.list_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        let r = &recent[0];
        assert_eq!(r.watch_id, 7);
        assert_eq!(r.status, RunStatus::Success);
        assert_eq!(r.duration_ms, 1234);
        assert_eq!(r.ai_title.as_deref(), Some("NDA Acme Corp"));
    }

    #[test]
    fn list_returns_newest_first() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        for i in 0..5 {
            log.record(
                1,
                &format!("/in/{i}.pdf"),
                None,
                RunStatus::Failed,
                Some("boom"),
                10,
                None,
            )
            .unwrap();
        }
        let recent = log.list_recent(3).unwrap();
        assert_eq!(recent.len(), 3);
        // Newest first — ids descending.
        assert!(recent[0].id > recent[1].id);
        assert!(recent[1].id > recent[2].id);
    }

    #[test]
    fn status_round_trips_through_string() {
        assert_eq!(RunStatus::parse("success"), RunStatus::Success);
        assert_eq!(RunStatus::parse("failed"), RunStatus::Failed);
        assert_eq!(RunStatus::parse("skipped"), RunStatus::Skipped);
        assert_eq!(RunStatus::parse("garbage"), RunStatus::Failed);
        assert_eq!(RunStatus::Success.as_str(), "success");
    }
}
