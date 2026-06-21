//! Marketplace install log — append-only history of every plugin install,
//! update, uninstall, and failed install attempt.
//!
//! Lives in its own sqlite DB at `~/.slab/marketplace-history.sqlite` so the
//! pluggable storage DB (`plugin-storage.sqlite`) and the hopper run DB stay
//! independent — failures here can't poison plugin runtime storage or hopper
//! routing. The schema is intentionally narrow: one row per install-pipeline
//! event with the fields the UI's "Activity" / "Recent installs" surfaces
//! need to render a timeline without round-tripping to the marketplace index.
//!
//! ## Why this exists
//!
//! Before this slice the marketplace install pipeline returned an
//! [`InstallReport`] to the caller and forgot the event ever happened. The
//! UI could show "you have v1.4 installed" but couldn't answer:
//!
//! - When did I install this? Which version did it replace?
//! - Did an update fail last week? With what error?
//! - What did I install across all plugins in the last month?
//!
//! Those are exactly the questions a paralegal triaging "why is this plugin
//! broken?" needs answered, and the questions the auditor reviewing a
//! firm's plugin posture needs in a one-line export.
//!
//! ## Schema (v1)
//!
//! ```sql
//! CREATE TABLE install_events (
//!     id                  INTEGER PRIMARY KEY,
//!     plugin_id           TEXT NOT NULL,
//!     version             TEXT NOT NULL,
//!     action              TEXT NOT NULL,    -- install | update | uninstall | failed
//!     occurred_at         INTEGER NOT NULL, -- unix seconds
//!     source              TEXT,             -- 'marketplace' | 'sideload' | ...; NULL on uninstall/failed
//!     bytes_written       INTEGER,          -- NULL on uninstall/failed
//!     files_extracted     INTEGER,          -- NULL on uninstall/failed
//!     replaced_existing   INTEGER,          -- 0/1; NULL on uninstall/failed
//!     prior_version       TEXT,             -- NULL on fresh install/uninstall/failed
//!     error_msg           TEXT              -- NULL on success rows
//! );
//! CREATE INDEX install_events_plugin_idx ON install_events(plugin_id, occurred_at DESC);
//! CREATE INDEX install_events_occurred_idx ON install_events(occurred_at DESC);
//! ```
//!
//! All NULL-able columns are populated only on the rows where they make
//! sense. The two indexes cover the only two read paths: per-plugin
//! timeline (Activity section on PluginDetailDrawer) and corpus-wide
//! recent (Recent installs drawer).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Schema version stamped into `PRAGMA user_version`. Bump + add a
/// migration arm in [`InstallLog::init_schema`] when changing the
/// table shape. v1: initial `install_events` table.
const SCHEMA_VERSION: u32 = 1;

/// Discriminator for what kind of event a row represents. Serialised
/// as the lowercase string the SQL column stores ("install" /
/// "update" / "uninstall" / "failed") so reading and writing share
/// one vocabulary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InstallAction {
    /// Fresh install — `<plugins_root>/<id>/` did not exist before.
    Install,
    /// Replaced an existing install at the same id. `prior_version`
    /// is populated if the prior install's version is known to the
    /// caller (the install pipeline doesn't read the prior manifest;
    /// the wiring layer in `lib.rs` does).
    Update,
    /// `<plugins_root>/<id>/` removed. `version` is the version that
    /// was just removed (caller resolves this from the registry
    /// before deletion).
    Uninstall,
    /// Install or update attempt that aborted before completing.
    /// `error_msg` carries the failure reason.
    Failed,
}

impl InstallAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update",
            Self::Uninstall => "uninstall",
            Self::Failed => "failed",
        }
    }

    /// Inverse of [`as_str`]. Unknown strings (e.g. a row written by
    /// a future schema version) parse as [`InstallAction::Failed`]
    /// so the reader doesn't have to handle a third "unknown" case
    /// — the conservative choice for an audit log.
    pub fn parse(s: &str) -> Self {
        match s {
            "install" => Self::Install,
            "update" => Self::Update,
            "uninstall" => Self::Uninstall,
            _ => Self::Failed,
        }
    }
}

/// One persisted row in the `install_events` table. NULL-able columns
/// only carry a value on the row kinds that need them — see
/// [`InstallAction`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallEvent {
    pub id: i64,
    pub plugin_id: String,
    pub version: String,
    pub action: InstallAction,
    /// Unix seconds (UTC).
    pub occurred_at: i64,
    /// Origin label — currently `"marketplace"` for entries fetched
    /// from the curated index, `"sideload"` for direct-bytes installs
    /// (no UI today; reserved for the future drag-drop install
    /// affordance). NULL on uninstall/failed rows.
    pub source: Option<String>,
    /// Total uncompressed bytes written. NULL on uninstall/failed.
    pub bytes_written: Option<i64>,
    /// Number of files (not directories) extracted. NULL on
    /// uninstall/failed.
    pub files_extracted: Option<i64>,
    /// True if the install overwrote a previously-installed copy.
    /// NULL on uninstall/failed.
    pub replaced_existing: Option<bool>,
    /// Version that was replaced on an update row. NULL on fresh
    /// install / uninstall / failed.
    pub prior_version: Option<String>,
    /// Failure reason for `failed` rows. NULL on success rows.
    pub error_msg: Option<String>,
}

/// Slim per-plugin counts payload — used by the "Activity" header in
/// PluginDetailDrawer to render "Installed 3× · 1 update · 0 failures"
/// without paging the full event list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InstallStats {
    pub installs: i64,
    pub updates: i64,
    pub uninstalls: i64,
    pub failures: i64,
}

#[derive(Debug, Error)]
pub enum InstallLogError {
    #[error("sqlite: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Default DB path: `~/.slab/marketplace-history.sqlite`. Falls back
/// to `./marketplace-history.sqlite` if `$HOME` is somehow unset —
/// matches the pattern used by `plugins::default_db_path()`.
pub fn default_log_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("marketplace-history.sqlite")
}

/// Owning handle around the per-process sqlite connection for the
/// install log. Cheap to construct (one open + one migration call);
/// callers can hold one for the app lifetime via a `OnceLock` if
/// needed.
pub struct InstallLog {
    conn: Connection,
}

impl InstallLog {
    /// Open (or create) the install log at `path`, ensuring the
    /// parent directory exists, and initialise the schema.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, InstallLogError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory log for tests. Always public (not
    /// `#[cfg(test)]`) so integration tests outside this module's
    /// tree can build a deterministic per-test log too.
    pub fn open_in_memory() -> Result<Self, InstallLogError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), InstallLogError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS install_events (
                id                INTEGER PRIMARY KEY,
                plugin_id         TEXT NOT NULL,
                version           TEXT NOT NULL,
                action            TEXT NOT NULL,
                occurred_at       INTEGER NOT NULL,
                source            TEXT,
                bytes_written     INTEGER,
                files_extracted   INTEGER,
                replaced_existing INTEGER,
                prior_version     TEXT,
                error_msg         TEXT
            );
            CREATE INDEX IF NOT EXISTS install_events_plugin_idx
                ON install_events(plugin_id, occurred_at DESC);
            CREATE INDEX IF NOT EXISTS install_events_occurred_idx
                ON install_events(occurred_at DESC);
            "#,
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Return the on-disk schema version. Useful for tests pinning
    /// migrations.
    pub fn schema_version(&self) -> Result<u32, InstallLogError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    // ─── Writers — one per InstallAction so callers don't have to
    // ─── remember which NULL-able columns to pass.

    /// Record a successful install or update. If `prior_version` is
    /// `Some`, the row's action is set to `Update` (and
    /// `replaced_existing` is forced true). Otherwise it's an
    /// `Install` row. Returns the assigned id.
    pub fn record_install(
        &mut self,
        plugin_id: &str,
        version: &str,
        source: &str,
        bytes_written: u64,
        files_extracted: u32,
        prior_version: Option<&str>,
    ) -> Result<i64, InstallLogError> {
        let action = if prior_version.is_some() {
            InstallAction::Update
        } else {
            InstallAction::Install
        };
        let replaced = prior_version.is_some();
        let occurred = unix_now();
        self.conn.execute(
            "INSERT INTO install_events
                (plugin_id, version, action, occurred_at, source,
                 bytes_written, files_extracted, replaced_existing,
                 prior_version, error_msg)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
            params![
                plugin_id,
                version,
                action.as_str(),
                occurred,
                source,
                bytes_written as i64,
                files_extracted as i64,
                replaced as i64,
                prior_version,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Record a successful uninstall. `version` is the version that
    /// was just removed (caller resolves this from the registry
    /// before deletion). Returns the assigned id.
    pub fn record_uninstall(
        &mut self,
        plugin_id: &str,
        version: &str,
    ) -> Result<i64, InstallLogError> {
        let occurred = unix_now();
        self.conn.execute(
            "INSERT INTO install_events
                (plugin_id, version, action, occurred_at, source,
                 bytes_written, files_extracted, replaced_existing,
                 prior_version, error_msg)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, NULL)",
            params![
                plugin_id,
                version,
                InstallAction::Uninstall.as_str(),
                occurred,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Record a failed install/update attempt. `version` is the
    /// version that was attempted (so the row can be joined back to
    /// the marketplace index in the UI). Returns the assigned id.
    pub fn record_failure(
        &mut self,
        plugin_id: &str,
        version: &str,
        error_msg: &str,
    ) -> Result<i64, InstallLogError> {
        let occurred = unix_now();
        self.conn.execute(
            "INSERT INTO install_events
                (plugin_id, version, action, occurred_at, source,
                 bytes_written, files_extracted, replaced_existing,
                 prior_version, error_msg)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, ?5)",
            params![
                plugin_id,
                version,
                InstallAction::Failed.as_str(),
                occurred,
                error_msg,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    // ─── Readers — kept on this side of the file so tests can
    // ─── exercise full round-trips without crossing modules.

    /// Per-plugin timeline, newest first, capped at `limit`. Returns
    /// `Vec` (never `Option`) — an unknown plugin id yields an empty
    /// timeline.
    pub fn list_events(
        &self,
        plugin_id: &str,
        limit: i64,
    ) -> Result<Vec<InstallEvent>, InstallLogError> {
        let limit = limit.max(0);
        let mut stmt = self.conn.prepare(
            "SELECT id, plugin_id, version, action, occurred_at, source,
                    bytes_written, files_extracted, replaced_existing,
                    prior_version, error_msg
             FROM install_events
             WHERE plugin_id = ?1
             ORDER BY occurred_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![plugin_id, limit], row_to_event)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Corpus-wide recent events, newest first, capped at `limit`.
    /// Drives the toolbar "Recent installs" drawer.
    pub fn list_recent(&self, limit: i64) -> Result<Vec<InstallEvent>, InstallLogError> {
        let limit = limit.max(0);
        let mut stmt = self.conn.prepare(
            "SELECT id, plugin_id, version, action, occurred_at, source,
                    bytes_written, files_extracted, replaced_existing,
                    prior_version, error_msg
             FROM install_events
             ORDER BY occurred_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], row_to_event)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Per-plugin counts of each action kind. Used by the Activity
    /// header to render "Installed 3× · 1 update · 0 failures" in
    /// one round-trip.
    pub fn install_stats(&self, plugin_id: &str) -> Result<InstallStats, InstallLogError> {
        let mut stmt = self.conn.prepare(
            "SELECT action, COUNT(*) FROM install_events
             WHERE plugin_id = ?1
             GROUP BY action",
        )?;
        let rows = stmt.query_map(params![plugin_id], |r| {
            let action: String = r.get(0)?;
            let count: i64 = r.get(1)?;
            Ok((action, count))
        })?;
        let mut stats = InstallStats::default();
        for r in rows {
            let (a, n) = r?;
            match InstallAction::parse(&a) {
                InstallAction::Install => stats.installs = n,
                InstallAction::Update => stats.updates = n,
                InstallAction::Uninstall => stats.uninstalls = n,
                InstallAction::Failed => stats.failures = n,
            }
        }
        Ok(stats)
    }

    /// Count of distinct plugin ids that ever appeared in the log.
    /// Used by the toolbar badge so the "History" button shows a
    /// count (`History · 7 plugins`) only when there's something to
    /// see. Cheap — one `SELECT COUNT(DISTINCT plugin_id)`.
    pub fn distinct_plugin_count(&self) -> Result<i64, InstallLogError> {
        let n: Option<i64> = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT plugin_id) FROM install_events",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(n.unwrap_or(0))
    }
}

fn row_to_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<InstallEvent> {
    let action_s: String = r.get(3)?;
    let replaced: Option<i64> = r.get(8)?;
    Ok(InstallEvent {
        id: r.get(0)?,
        plugin_id: r.get(1)?,
        version: r.get(2)?,
        action: InstallAction::parse(&action_s),
        occurred_at: r.get(4)?,
        source: r.get(5)?,
        bytes_written: r.get(6)?,
        files_extracted: r.get(7)?,
        replaced_existing: replaced.map(|v| v != 0),
        prior_version: r.get(9)?,
        error_msg: r.get(10)?,
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_v1_pragma_pinned() {
        let log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn install_action_round_trips_via_string() {
        for a in [
            InstallAction::Install,
            InstallAction::Update,
            InstallAction::Uninstall,
            InstallAction::Failed,
        ] {
            assert_eq!(InstallAction::parse(a.as_str()), a);
        }
    }

    #[test]
    fn install_action_parse_unknown_is_failed() {
        // Conservative behaviour: a row written by a future schema
        // surfaces as "failed" rather than panicking on an unknown
        // enum tag.
        assert_eq!(InstallAction::parse("gibberish"), InstallAction::Failed);
        assert_eq!(InstallAction::parse(""), InstallAction::Failed);
    }

    #[test]
    fn record_install_fresh_writes_install_row() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let id = log
            .record_install("com.example.x", "1.0.0", "marketplace", 12_345, 7, None)
            .unwrap();
        assert!(id > 0);
        let events = log.list_events("com.example.x", 10).unwrap();
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.action, InstallAction::Install);
        assert_eq!(e.version, "1.0.0");
        assert_eq!(e.source.as_deref(), Some("marketplace"));
        assert_eq!(e.bytes_written, Some(12_345));
        assert_eq!(e.files_extracted, Some(7));
        assert_eq!(e.replaced_existing, Some(false));
        assert!(e.prior_version.is_none());
        assert!(e.error_msg.is_none());
    }

    #[test]
    fn record_install_with_prior_writes_update_row() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.record_install("com.example.x", "2.0.0", "marketplace", 1, 1, Some("1.0.0"))
            .unwrap();
        let events = log.list_events("com.example.x", 10).unwrap();
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.action, InstallAction::Update);
        assert_eq!(e.version, "2.0.0");
        assert_eq!(e.prior_version.as_deref(), Some("1.0.0"));
        assert_eq!(e.replaced_existing, Some(true));
    }

    #[test]
    fn record_uninstall_writes_uninstall_row_with_nulls() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.record_uninstall("com.example.x", "1.4.2").unwrap();
        let events = log.list_events("com.example.x", 10).unwrap();
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.action, InstallAction::Uninstall);
        assert_eq!(e.version, "1.4.2");
        // All install-only fields should be NULL on an uninstall row.
        assert!(e.source.is_none());
        assert!(e.bytes_written.is_none());
        assert!(e.files_extracted.is_none());
        assert!(e.replaced_existing.is_none());
        assert!(e.prior_version.is_none());
        assert!(e.error_msg.is_none());
    }

    #[test]
    fn record_failure_writes_failed_row_with_error_msg() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.record_failure("com.example.x", "1.0.0", "sha256 mismatch")
            .unwrap();
        let events = log.list_events("com.example.x", 10).unwrap();
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.action, InstallAction::Failed);
        assert_eq!(e.error_msg.as_deref(), Some("sha256 mismatch"));
        // All success-only fields should be NULL on a failed row.
        assert!(e.source.is_none());
        assert!(e.bytes_written.is_none());
        assert!(e.files_extracted.is_none());
    }

    #[test]
    fn list_events_newest_first_per_plugin() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // Three events for plugin A; one event for B. Ids are
        // monotonic so the ORDER BY id DESC fallback also pins
        // newest-first when occurred_at ties (same second).
        log.record_install("com.a", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_install("com.a", "2", "marketplace", 1, 1, Some("1"))
            .unwrap();
        log.record_uninstall("com.a", "2").unwrap();
        log.record_install("com.b", "0.1", "marketplace", 1, 1, None)
            .unwrap();

        let a = log.list_events("com.a", 10).unwrap();
        assert_eq!(a.len(), 3);
        // Order: uninstall (latest), update, install (oldest).
        assert_eq!(a[0].action, InstallAction::Uninstall);
        assert_eq!(a[1].action, InstallAction::Update);
        assert_eq!(a[2].action, InstallAction::Install);

        let b = log.list_events("com.b", 10).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].plugin_id, "com.b");
    }

    #[test]
    fn list_events_limit_caps_returned_rows() {
        let mut log = InstallLog::open_in_memory().unwrap();
        for v in 1..=5 {
            log.record_install("com.x", &format!("0.{v}"), "marketplace", 1, 1, None)
                .unwrap();
        }
        let two = log.list_events("com.x", 2).unwrap();
        assert_eq!(two.len(), 2);
        let zero = log.list_events("com.x", 0).unwrap();
        assert_eq!(zero.len(), 0);
        // Negative limit clamps to zero rather than blowing up.
        let neg = log.list_events("com.x", -3).unwrap();
        assert_eq!(neg.len(), 0);
    }

    #[test]
    fn list_events_unknown_plugin_returns_empty() {
        let log = InstallLog::open_in_memory().unwrap();
        let events = log.list_events("com.nope", 10).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn list_recent_spans_all_plugins_newest_first() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.record_install("com.a", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_install("com.b", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_install("com.c", "1", "marketplace", 1, 1, None)
            .unwrap();
        let r = log.list_recent(2).unwrap();
        assert_eq!(r.len(), 2);
        // Latest two writes are com.c then com.b.
        assert_eq!(r[0].plugin_id, "com.c");
        assert_eq!(r[1].plugin_id, "com.b");
    }

    #[test]
    fn install_stats_counts_each_action_independently() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // 2 installs, 1 update, 1 uninstall, 2 failures for "com.x".
        log.record_install("com.x", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_install("com.x", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_install("com.x", "2", "marketplace", 1, 1, Some("1"))
            .unwrap();
        log.record_uninstall("com.x", "2").unwrap();
        log.record_failure("com.x", "3", "bad sig").unwrap();
        log.record_failure("com.x", "3", "bad sig").unwrap();
        // Plus an unrelated row that must NOT leak into com.x's
        // stats.
        log.record_install("com.y", "9", "marketplace", 1, 1, None)
            .unwrap();

        let s = log.install_stats("com.x").unwrap();
        assert_eq!(s.installs, 2);
        assert_eq!(s.updates, 1);
        assert_eq!(s.uninstalls, 1);
        assert_eq!(s.failures, 2);
    }

    #[test]
    fn install_stats_unknown_plugin_all_zeroes() {
        let log = InstallLog::open_in_memory().unwrap();
        let s = log.install_stats("com.nope").unwrap();
        assert_eq!(s, InstallStats::default());
    }

    #[test]
    fn distinct_plugin_count_dedupes_repeated_writes() {
        let mut log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.distinct_plugin_count().unwrap(), 0);
        log.record_install("com.a", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_install("com.a", "2", "marketplace", 1, 1, Some("1"))
            .unwrap();
        log.record_uninstall("com.a", "2").unwrap();
        // All three rows are the same plugin id.
        assert_eq!(log.distinct_plugin_count().unwrap(), 1);
        log.record_install("com.b", "1", "marketplace", 1, 1, None)
            .unwrap();
        assert_eq!(log.distinct_plugin_count().unwrap(), 2);
    }
}
