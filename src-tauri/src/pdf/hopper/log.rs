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

            -- v3.22.0 "Hopper Loop": batch backfill history. One row per
            -- `execute_backfill` invocation; `report_json` stores the
            -- full `BackfillRun` for the UI's expandable per-file view.
            CREATE TABLE IF NOT EXISTS backfill_runs (
                id           INTEGER PRIMARY KEY,
                folder       TEXT NOT NULL,
                scanned      INTEGER NOT NULL,
                applied      INTEGER NOT NULL,
                skipped      INTEGER NOT NULL,
                errored      INTEGER NOT NULL,
                started_at   INTEGER NOT NULL,
                finished_at  INTEGER NOT NULL,
                report_json  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS backfill_runs_folder_idx
                ON backfill_runs(folder, started_at DESC);
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

    // ── v3.22.0 backfill history ─────────────────────────────────────

    /// Persist a [`super::backfill::BackfillRun`] in the
    /// `backfill_runs` table. Returns the assigned id. The full run is
    /// also serialised into `report_json` so the UI can expand a row to
    /// show every per-file outcome without a join.
    pub fn record_backfill_run(
        &mut self,
        run: &super::backfill::BackfillRun,
    ) -> rusqlite::Result<i64> {
        let report_json = serde_json::to_string(run).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?;
        self.conn.execute(
            "INSERT INTO backfill_runs \
                (folder, scanned, applied, skipped, errored, \
                 started_at, finished_at, report_json) \
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.folder,
                run.scanned as i64,
                run.applied as i64,
                run.skipped as i64,
                run.errored as i64,
                run.started_at as i64,
                run.finished_at as i64,
                report_json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List the most recent backfill runs, optionally filtered to a
    /// single source folder. Pass `None` for `folder` to get the
    /// global tail (used by the Hopper home panel's history strip).
    pub fn list_backfill_runs(
        &self,
        folder: Option<&str>,
        limit: i64,
    ) -> rusqlite::Result<Vec<super::backfill::BackfillRun>> {
        // Delegate to the time-window variant with `since_unix = None`
        // (no temporal floor) so the two paths stay behaviour-identical.
        self.list_backfill_runs_since(folder, None, limit)
    }

    /// List backfill runs that *finished* at or after `since_unix`
    /// (inclusive unix-seconds), newest first. `None` for `since_unix`
    /// disables the temporal filter (equivalent to [`list_backfill_runs`]).
    ///
    /// Powers the panel's "Last 24h / Last 7d / All" history filter
    /// chips. Filtering happens in SQL so the panel doesn't pull
    /// thousands of rows over the wire just to drop them client-side.
    ///
    /// Both filters AND together when set: `folder = Some("/in/A")` +
    /// `since_unix = Some(...)` returns only runs in `/in/A` finished
    /// in the window.
    pub fn list_backfill_runs_since(
        &self,
        folder: Option<&str>,
        since_unix: Option<i64>,
        limit: i64,
    ) -> rusqlite::Result<Vec<super::backfill::BackfillRun>> {
        let rows = match (folder, since_unix) {
            (Some(f), Some(since)) => {
                let mut stmt = self.conn.prepare(
                    "SELECT report_json FROM backfill_runs \
                     WHERE folder = ?1 AND finished_at >= ?2 \
                     ORDER BY id DESC LIMIT ?3",
                )?;
                let mapped = stmt.query_map(params![f, since, limit], |r| r.get::<_, String>(0))?;
                mapped.collect::<rusqlite::Result<Vec<_>>>()?
            }
            (Some(f), None) => {
                let mut stmt = self.conn.prepare(
                    "SELECT report_json FROM backfill_runs \
                     WHERE folder = ?1 ORDER BY id DESC LIMIT ?2",
                )?;
                let mapped = stmt.query_map(params![f, limit], |r| r.get::<_, String>(0))?;
                mapped.collect::<rusqlite::Result<Vec<_>>>()?
            }
            (None, Some(since)) => {
                let mut stmt = self.conn.prepare(
                    "SELECT report_json FROM backfill_runs \
                     WHERE finished_at >= ?1 \
                     ORDER BY id DESC LIMIT ?2",
                )?;
                let mapped = stmt.query_map(params![since, limit], |r| r.get::<_, String>(0))?;
                mapped.collect::<rusqlite::Result<Vec<_>>>()?
            }
            (None, None) => {
                let mut stmt = self.conn.prepare(
                    "SELECT report_json FROM backfill_runs \
                     ORDER BY id DESC LIMIT ?1",
                )?;
                let mapped = stmt.query_map(params![limit], |r| r.get::<_, String>(0))?;
                mapped.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        let mut out = Vec::with_capacity(rows.len());
        for json in rows {
            let run: super::backfill::BackfillRun = serde_json::from_str(&json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(e.to_string())),
                )
            })?;
            out.push(run);
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

    // ── v3.22.0 backfill history ─────────────────────────────────────

    fn fake_run(folder: &str, applied: usize) -> super::super::backfill::BackfillRun {
        super::super::backfill::BackfillRun {
            folder: folder.into(),
            scanned: applied + 1,
            applied,
            skipped: 1,
            errored: 0,
            started_at: 1000,
            finished_at: 1010,
            per_file: vec![],
        }
    }

    #[test]
    fn record_and_list_backfill_runs_round_trip() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        let r = fake_run("/in/A", 4);
        let id = log.record_backfill_run(&r).unwrap();
        assert!(id > 0);

        let all = log.list_backfill_runs(None, 10).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], r);
    }

    #[test]
    fn list_backfill_runs_filters_by_folder() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        log.record_backfill_run(&fake_run("/in/A", 1)).unwrap();
        log.record_backfill_run(&fake_run("/in/B", 2)).unwrap();
        log.record_backfill_run(&fake_run("/in/A", 3)).unwrap();

        let only_a = log.list_backfill_runs(Some("/in/A"), 10).unwrap();
        assert_eq!(only_a.len(), 2);
        // Newest first.
        assert_eq!(only_a[0].applied, 3);
        assert_eq!(only_a[1].applied, 1);

        let all = log.list_backfill_runs(None, 10).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn list_backfill_runs_respects_limit() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        for i in 0..5 {
            log.record_backfill_run(&fake_run("/in/X", i)).unwrap();
        }
        let two = log.list_backfill_runs(Some("/in/X"), 2).unwrap();
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].applied, 4);
        assert_eq!(two[1].applied, 3);
    }

    // ── v3.39 round-10: time-window history filter ──────────────────

    fn run_with_finished(
        folder: &str,
        applied: usize,
        finished_at: u64,
    ) -> super::super::backfill::BackfillRun {
        super::super::backfill::BackfillRun {
            folder: folder.into(),
            scanned: applied + 1,
            applied,
            skipped: 1,
            errored: 0,
            started_at: finished_at.saturating_sub(10),
            finished_at,
            per_file: vec![],
        }
    }

    /// `since_unix = None` is behaviour-identical to the legacy
    /// `list_backfill_runs`. Pins the back-compat delegation.
    #[test]
    fn list_backfill_runs_since_with_none_matches_legacy() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 1, 100))
            .unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 2, 200))
            .unwrap();

        let legacy = log.list_backfill_runs(None, 10).unwrap();
        let windowed = log.list_backfill_runs_since(None, None, 10).unwrap();
        assert_eq!(legacy, windowed);
    }

    /// `since_unix = Some(t)` returns only runs finished at or after
    /// `t`. The boundary is inclusive — a run that finished exactly
    /// at the cutoff still appears.
    #[test]
    fn list_backfill_runs_since_filters_by_finished_at() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 1, 100))
            .unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 2, 200))
            .unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 3, 300))
            .unwrap();

        let window = log.list_backfill_runs_since(None, Some(200), 10).unwrap();
        assert_eq!(window.len(), 2);
        // Newest first.
        assert_eq!(window[0].applied, 3);
        assert_eq!(window[1].applied, 2);

        // Boundary inclusive — exact-match cutoff still appears.
        let boundary = log.list_backfill_runs_since(None, Some(300), 10).unwrap();
        assert_eq!(boundary.len(), 1);
        assert_eq!(boundary[0].applied, 3);
    }

    /// Both filters AND together — folder + since combine into one
    /// SQL where-clause, not two passes. Pins the panel's combined
    /// "/in/A in last 24h" lookup.
    #[test]
    fn list_backfill_runs_since_combines_with_folder() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 1, 100))
            .unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 2, 200))
            .unwrap();
        log.record_backfill_run(&run_with_finished("/in/B", 5, 250))
            .unwrap();

        let win = log
            .list_backfill_runs_since(Some("/in/A"), Some(150), 10)
            .unwrap();
        assert_eq!(win.len(), 1);
        assert_eq!(win[0].applied, 2);
        assert_eq!(win[0].folder, "/in/A");
    }

    /// A future cutoff returns an empty list — no false positives.
    #[test]
    fn list_backfill_runs_since_returns_empty_for_future_cutoff() {
        let (_g, path) = tmp_db();
        let mut log = HopperLog::open(&path).unwrap();
        log.record_backfill_run(&run_with_finished("/in/A", 1, 100))
            .unwrap();
        let nothing = log
            .list_backfill_runs_since(None, Some(1_000_000), 10)
            .unwrap();
        assert!(nothing.is_empty());
    }
}
