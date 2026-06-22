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
/// table shape.
///
/// - v1: initial `install_events` table.
/// - v2: `install_log_settings` key-value table for retention policy
///   (retain_days + last_auto_prune_at). Pure additive — every v1 row
///   stays valid; the new table starts empty and the policy reader
///   falls back to [`DEFAULT_RETAIN_DAYS`] when unset.
/// - v3: `install_log_plugin_retention(plugin_id PRIMARY KEY,
///   retain_days INTEGER NOT NULL)` per-plugin retention overrides.
///   Audit-critical plugins (compliance reports, redaction tooling)
///   want longer retention than the global default; noisy diagnostic
///   plugins want shorter. Pure additive: every v2 row stays valid;
///   the new table starts empty and the effective-retention resolver
///   falls back to the global `retain_days` when an id has no
///   override row.
const SCHEMA_VERSION: u32 = 3;

/// Default retention window when the user has never explicitly set
/// one. 365 days picked to match the round-12 design note: long
/// enough that quarterly + annual audits still resolve, short enough
/// that an installer-heavy workstation doesn't accumulate years of
/// dead rows. Same value the round-12 doc-comment on
/// [`InstallLog::prune_older_than`] recommends.
pub const DEFAULT_RETAIN_DAYS: i64 = 365;

/// Minimum days between auto-prune executions. The startup auto-prune
/// reads `last_auto_prune_at` and skips the prune call if the last
/// run was within this window — keeps the prune to roughly daily
/// even when the app is launched many times per day.
pub const AUTO_PRUNE_INTERVAL_SECS: i64 = 86_400; // 24h

/// Minimum allowed retain_days. Mirrors the floor
/// [`crate::slab_marketplace_install_log_prune`] enforces (the manual
/// "Clear older than" affordance also clamps at >=1) so the policy
/// surface and the one-shot prune share one floor.
pub const MIN_RETAIN_DAYS: i64 = 1;

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

/// One row of the per-plugin histogram aggregate (Slice 87). Carries
/// the same per-action counts as [`InstallStats`] plus the plugin id,
/// the precomputed total (sum of all four buckets) for sort + bar
/// scaling, and the last activity timestamp so the UI can render
/// "Last activity: 3d ago" next to each row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginHistogramRow {
    pub plugin_id: String,
    pub installs: i64,
    pub updates: i64,
    pub uninstalls: i64,
    pub failures: i64,
    /// Sum of installs + updates + uninstalls + failures within the
    /// queried window. Precomputed so the UI's bar-width and sort
    /// don't have to re-add four columns per row.
    pub total: i64,
    /// Unix seconds of the most recent event for this plugin within
    /// the queried window. The UI renders this as a relative "Xd
    /// ago" chip beside the bar.
    pub last_occurred_at: i64,
}

/// One persisted per-plugin retention override (Slice 113). Mirrors a
/// single row of `install_log_plugin_retention` after the storage
/// floor has been applied. `retain_days` is guaranteed `>=
/// MIN_RETAIN_DAYS` (clamped on read so a stored bad value never
/// surfaces to the auto-prune driver).
///
/// Serde-friendly so the wire layer (Slice 117) ships it straight to
/// the Retention section UI; PartialEq so tests pin equality on the
/// override list across writes/reads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRetentionOverride {
    pub plugin_id: String,
    pub retain_days: i64,
}

/// Granularity of the install-log activity timeline aggregate (Slice
/// 103). The three calendar buckets the UI offers — daily for
/// short-window drilldowns ("show me the last 14 days"), weekly for
/// medium windows ("the last 3 months"), and monthly for long
/// windows ("the year in review").
///
/// Bucket boundaries are computed in UTC:
///   - `Day`   → floor to the UTC midnight that the timestamp lives in
///   - `Week`  → floor to the UTC Monday (ISO-8601 week start) the
///     timestamp lives in
///   - `Month` → floor to the UTC first-of-month the timestamp lives in
///
/// UTC (not local) so the same audit query produces the same buckets
/// regardless of which machine ran it — a paralegal cross-referencing
/// timelines from two laptops in different timezones gets the same
/// answer either way. The UI is free to *render* the bucket labels in
/// local time; the boundaries themselves stay deterministic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TimeBucketGranularity {
    /// One bucket per UTC calendar day (86 400 seconds wide).
    Day,
    /// One bucket per ISO-8601 week, anchored at UTC Monday 00:00:00.
    Week,
    /// One bucket per UTC calendar month (variable width: 28-31 days).
    Month,
}

impl TimeBucketGranularity {
    /// Lowercase tag matching the serde representation. Used by the
    /// CSV exporter so a downstream reader can re-derive the bucket
    /// granularity without consulting the export filename.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }

    /// Inverse of [`as_str`]. Unknown strings fall back to
    /// [`TimeBucketGranularity::Day`] so a future schema addition
    /// reads as the smallest bucket rather than panicking — matches
    /// the conservative posture of [`InstallAction::parse`].
    pub fn parse(s: &str) -> Self {
        match s {
            "week" => Self::Week,
            "month" => Self::Month,
            _ => Self::Day,
        }
    }
}

/// One bucket in the install-log activity timeline aggregate (Slice
/// 103). Carries the same four per-action counts as
/// [`PluginHistogramRow`] plus a `total` for sort / bar scaling, but
/// is keyed by `bucket_start_unix` (the UTC-floored start of the
/// bucket window) instead of `plugin_id` — answers "WHEN was install
/// activity happening?" rather than "WHICH plugins were active?".
///
/// Bucket emit order is ASCENDING by `bucket_start_unix` so the UI
/// can render the timeline left-to-right (oldest → newest, the
/// natural reading direction). Sparse: only buckets with at least one
/// event are emitted. Densifying the timeline with zero-event buckets
/// for the missing days/weeks/months is the UI's job (so the
/// primitive stays cheap when a corpus is enormous but mostly idle).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityBucket {
    /// UTC-floored start of the bucket window (unix seconds). For
    /// `Day` it's UTC midnight; for `Week` it's the UTC Monday
    /// 00:00:00; for `Month` it's the UTC first-of-month 00:00:00.
    pub bucket_start_unix: i64,
    pub installs: i64,
    pub updates: i64,
    pub uninstalls: i64,
    pub failures: i64,
    /// Sum of installs + updates + uninstalls + failures within the
    /// bucket. Precomputed so the UI's bar-height computation doesn't
    /// have to re-add four columns per bucket.
    pub total: i64,
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
            CREATE TABLE IF NOT EXISTS install_log_settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS install_log_plugin_retention (
                plugin_id   TEXT PRIMARY KEY,
                retain_days INTEGER NOT NULL
            );
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

    // ─── Retention / pruning surface (Slice 56) ──────────────────────

    /// Unix seconds of the oldest row in the log, or `None` if the
    /// log is empty. Powers the UI's "Log spans X days" affordance
    /// in the Recent installs drawer so users know how far back the
    /// history goes.
    pub fn oldest_occurred_at(&self) -> Result<Option<i64>, InstallLogError> {
        // SELECT MIN(...) on an empty table returns ONE row whose
        // column is NULL — not zero rows. So `.optional()` doesn't
        // help here; we need the closure to read the column as
        // `Option<i64>` so NULL decodes cleanly to None.
        let n: Option<i64> =
            self.conn
                .query_row("SELECT MIN(occurred_at) FROM install_events", [], |r| {
                    r.get::<_, Option<i64>>(0)
                })?;
        Ok(n)
    }

    /// Delete all rows with `occurred_at < cutoff_unix`. Returns the
    /// number of rows removed. Idempotent: calling twice with the
    /// same cutoff is a no-op on the second call (the rows are
    /// already gone).
    ///
    /// Use this to bound on-disk audit-log growth. A reasonable
    /// retention policy is 365 days — long enough that "did I install
    /// this for the Q3 audit" still works, short enough that an
    /// installer-heavy workstation doesn't accumulate years of dead
    /// rows. The retention policy is a UI/settings concern; this
    /// primitive is just the executor.
    pub fn prune_older_than(&mut self, cutoff_unix: i64) -> Result<usize, InstallLogError> {
        let n = self.conn.execute(
            "DELETE FROM install_events WHERE occurred_at < ?1",
            params![cutoff_unix],
        )?;
        Ok(n)
    }

    /// Total row count in the install log. Cheap O(1) on sqlite's
    /// internal counters (no full-table scan). Used by the UI to
    /// pair with `oldest_occurred_at` for a "N events across X
    /// days" subtitle in the Recent installs drawer.
    pub fn total_event_count(&self) -> Result<i64, InstallLogError> {
        let n: Option<i64> = self
            .conn
            .query_row("SELECT COUNT(*) FROM install_events", [], |r| r.get(0))
            .optional()?;
        Ok(n.unwrap_or(0))
    }

    // ─── Time-window export reader (Slice 58) ────────────────────────

    /// Corpus-wide events whose `occurred_at` lies in the closed
    /// interval `[since_unix, until_unix]`, newest first, capped at
    /// `limit`. Either boundary may be `None` for "no lower / no
    /// upper bound"; passing `None` for both is equivalent to
    /// [`list_recent`] (every row).
    ///
    /// This is the read primitive that drives the install-log export
    /// surface — the Recent installs drawer's "Last 7d / Last 30d /
    /// All" window strip maps directly to a `since_unix` value, and
    /// the export menu funnels the selected window through this
    /// reader so the CSV/JSON file matches what the user sees in the
    /// drawer. Bounds are inclusive on both ends so a precisely-aligned
    /// boundary row (e.g. an event stamped at exactly the cutoff)
    /// always survives.
    ///
    /// Same `LIMIT` semantics as [`list_recent`]: a negative limit
    /// clamps to zero rather than panicking.
    pub fn list_events_between(
        &self,
        since_unix: Option<i64>,
        until_unix: Option<i64>,
        limit: i64,
    ) -> Result<Vec<InstallEvent>, InstallLogError> {
        let limit = limit.max(0);
        // Assemble the WHERE clause from whatever bounds were
        // supplied. Both unset → no WHERE at all (delegates to a
        // plain newest-first scan).
        let mut sql = String::from(
            "SELECT id, plugin_id, version, action, occurred_at, source,
                    bytes_written, files_extracted, replaced_existing,
                    prior_version, error_msg
             FROM install_events",
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let mut clauses: Vec<&'static str> = Vec::new();
        if let Some(since) = since_unix {
            clauses.push("occurred_at >= ?");
            params.push(rusqlite::types::Value::Integer(since));
        }
        if let Some(until) = until_unix {
            clauses.push("occurred_at <= ?");
            params.push(rusqlite::types::Value::Integer(until));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY occurred_at DESC, id DESC LIMIT ?");
        params.push(rusqlite::types::Value::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_event)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ─── Filtered reader (Slice 73) ──────────────────────────────────

    /// Like [`list_events_between`] but with two additional axes for the
    /// Recent installs drawer's filter bar:
    ///
    /// - `actions`: when `Some`, only rows whose action is in the set are
    ///   returned. `None` (or an empty slice — both behave the same) keeps
    ///   every action kind. Useful for "show me only failures" or
    ///   "installs+updates only".
    /// - `plugin_id_substr`: when `Some`, a case-INSENSITIVE substring
    ///   match against `plugin_id`. Empty / whitespace-only strings behave
    ///   like `None` so the UI can hand the raw input field through
    ///   without trimming first. The match is non-glob, anchored anywhere
    ///   in the id — a user typing "ocr" matches both `com.acme.ocr-pro`
    ///   and `org.studio.ocr-batch`.
    ///
    /// All three axes (time window, action set, plugin substring) compose
    /// via AND so a "Last 7d failures for com.acme.\*" query reads
    /// naturally: `list_events_filtered(Some(7d_ago), None, Some([Failed]),
    /// Some("com.acme"), 100)`.
    ///
    /// Same LIMIT semantics as [`list_events_between`]: a negative limit
    /// clamps to zero. Same ordering (occurred_at DESC, id DESC).
    pub fn list_events_filtered(
        &self,
        since_unix: Option<i64>,
        until_unix: Option<i64>,
        actions: Option<&[InstallAction]>,
        plugin_id_substr: Option<&str>,
        limit: i64,
    ) -> Result<Vec<InstallEvent>, InstallLogError> {
        let limit = limit.max(0);

        // Normalise the substring axis: trim, treat empty as "no filter".
        let substr = plugin_id_substr
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase());

        // Normalise the action axis: treat an empty slice as "no filter".
        let actions = actions.filter(|s| !s.is_empty());

        // Assemble the WHERE clause dynamically. The shape mirrors
        // list_events_between so a query plan walker would see the same
        // index usage (occurred_at DESC); the action / substring filters
        // are tail-scan predicates that the sqlite planner applies after
        // the index seek.
        let mut sql = String::from(
            "SELECT id, plugin_id, version, action, occurred_at, source,
                    bytes_written, files_extracted, replaced_existing,
                    prior_version, error_msg
             FROM install_events",
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let mut clauses: Vec<String> = Vec::new();
        if let Some(since) = since_unix {
            clauses.push("occurred_at >= ?".into());
            params.push(rusqlite::types::Value::Integer(since));
        }
        if let Some(until) = until_unix {
            clauses.push("occurred_at <= ?".into());
            params.push(rusqlite::types::Value::Integer(until));
        }
        if let Some(actions) = actions {
            // IN (?,?,?,?) — one placeholder per action. Always =< 4
            // (the enum has four variants) so the placeholder explosion
            // sqlite warns about doesn't apply.
            let placeholders: Vec<&str> = actions.iter().map(|_| "?").collect();
            clauses.push(format!("action IN ({})", placeholders.join(",")));
            for a in actions {
                params.push(rusqlite::types::Value::Text(a.as_str().into()));
            }
        }
        if let Some(s) = &substr {
            // Case-insensitive substring via LOWER(plugin_id) LIKE
            // '%needle%'. The LIKE escape characters (%, _, \) are
            // doubled up so a user typing "100%" doesn't accidentally
            // turn the second % into a wildcard. Same convention the
            // Hopper rule UI uses for its filename predicates.
            let escaped = like_escape(s);
            clauses.push("LOWER(plugin_id) LIKE ? ESCAPE '\\'".into());
            params.push(rusqlite::types::Value::Text(format!("%{escaped}%")));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY occurred_at DESC, id DESC LIMIT ?");
        params.push(rusqlite::types::Value::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_event)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Return the most-recently-active `limit` distinct plugin_ids in
    /// the log, newest activity first. Powers the filter bar's plugin
    /// autocomplete in the Recent installs drawer so a user typing the
    /// first few characters of a plugin id sees the recently-active set
    /// as suggestions instead of having to remember the full id. Uses a
    /// SUBQUERY-per-plugin shape so the GROUP BY collapses each id to
    /// its newest occurrence, then orders the result newest-first.
    /// Negative limit clamps to zero.
    pub fn recent_plugin_ids(&self, limit: i64) -> Result<Vec<String>, InstallLogError> {
        let limit = limit.max(0);
        let mut stmt = self.conn.prepare(
            "SELECT plugin_id FROM install_events
             GROUP BY plugin_id
             ORDER BY MAX(occurred_at) DESC, plugin_id ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ─── Per-plugin histogram aggregate (Slice 87) ───────────────────

    /// Aggregate install-log activity grouped by plugin_id within an
    /// optional time window. Returns one [`PluginHistogramRow`] per
    /// distinct plugin, ordered by total activity (descending) so the
    /// most-active plugin lands first — matches the "which plugins did
    /// I install the most this month?" question the UI surfaces.
    ///
    /// `since_unix` / `until_unix` are inclusive boundaries (same
    /// shape as [`list_events_between`]); pass `None` for either to
    /// disable that side of the window.
    ///
    /// `limit` clamps the number of returned plugins; negative
    /// clamps to zero. The default UI call uses 25, large enough that
    /// the typical paralegal install footprint fits but small enough
    /// that an enormous corpus doesn't drag a giant grid into IPC.
    ///
    /// Stable secondary sort: when two plugins have the same total
    /// activity, ties break ASC on plugin_id so the order is
    /// reproducible across calls — the UI's "Top plugins" list
    /// shouldn't reshuffle on a refresh.
    pub fn plugin_histogram(
        &self,
        since_unix: Option<i64>,
        until_unix: Option<i64>,
        limit: i64,
    ) -> Result<Vec<PluginHistogramRow>, InstallLogError> {
        let limit = limit.max(0);

        // Build the optional WHERE clause for the time-window axis.
        // Mirrors the list_events_between shape so the query plan
        // walker reuses the same occurred_at index.
        let mut sql = String::from(
            "SELECT plugin_id, action, COUNT(*) AS n, MAX(occurred_at) AS last
             FROM install_events",
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let mut clauses: Vec<&'static str> = Vec::new();
        if let Some(since) = since_unix {
            clauses.push("occurred_at >= ?");
            params.push(rusqlite::types::Value::Integer(since));
        }
        if let Some(until) = until_unix {
            clauses.push("occurred_at <= ?");
            params.push(rusqlite::types::Value::Integer(until));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" GROUP BY plugin_id, action");

        // Walk the (plugin_id, action) grid and reduce into per-plugin
        // rows. We do the action-bucket reduction in code rather than
        // SQL (CASE WHEN action = 'install' THEN COUNT...) so the
        // column shape is symmetric with InstallStats and a future
        // action addition is a one-line match arm rather than a SQL
        // schema bump.
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;

        let mut acc: std::collections::HashMap<String, PluginHistogramRow> =
            std::collections::HashMap::new();
        for r in rows {
            let (plugin_id, action_str, n, last) = r?;
            let row = acc
                .entry(plugin_id.clone())
                .or_insert_with(|| PluginHistogramRow {
                    plugin_id: plugin_id.clone(),
                    installs: 0,
                    updates: 0,
                    uninstalls: 0,
                    failures: 0,
                    total: 0,
                    last_occurred_at: 0,
                });
            match InstallAction::parse(&action_str) {
                InstallAction::Install => row.installs = n,
                InstallAction::Update => row.updates = n,
                InstallAction::Uninstall => row.uninstalls = n,
                InstallAction::Failed => row.failures = n,
            }
            row.total += n;
            if last > row.last_occurred_at {
                row.last_occurred_at = last;
            }
        }

        let mut out: Vec<PluginHistogramRow> = acc.into_values().collect();
        // Primary sort: total activity DESC. Secondary: plugin_id ASC
        // so ties are deterministic across calls.
        out.sort_by(|a, b| {
            b.total
                .cmp(&a.total)
                .then_with(|| a.plugin_id.cmp(&b.plugin_id))
        });
        out.truncate(limit as usize);
        Ok(out)
    }

    // ─── Activity timeline aggregate (Slice 103) ─────────────────────

    /// Aggregate install-log activity grouped by calendar bucket
    /// within an optional time window. Returns one [`ActivityBucket`]
    /// per non-empty bucket, ordered ASCENDING by `bucket_start_unix`
    /// so the UI can render the timeline left-to-right (oldest →
    /// newest, the natural reading direction).
    ///
    /// Answers "WHEN was install activity happening?" — complementary
    /// to [`plugin_histogram`] which answers "WHICH plugins were
    /// active?". The two aggregates are independent axes over the
    /// same event log.
    ///
    /// `since_unix` / `until_unix` are inclusive boundaries (same
    /// shape as [`list_events_between`]); pass `None` for either to
    /// disable that side of the window.
    ///
    /// `granularity` controls the bucket width — see
    /// [`TimeBucketGranularity`] for the calendar floor semantics.
    /// All boundaries are UTC so the same query on two different
    /// machines emits the same buckets.
    ///
    /// SPARSE OUTPUT: only buckets with at least one event are
    /// emitted. The UI densifies the timeline (inserts zero-event
    /// buckets for the missing days/weeks/months) so the primitive
    /// stays cheap when a corpus is enormous but mostly idle.
    pub fn activity_timeline(
        &self,
        since_unix: Option<i64>,
        until_unix: Option<i64>,
        granularity: TimeBucketGranularity,
    ) -> Result<Vec<ActivityBucket>, InstallLogError> {
        // Same WHERE-assembly pattern as plugin_histogram — the
        // sqlite planner can reuse the occurred_at index for the
        // time-window seek.
        let mut sql = String::from("SELECT action, occurred_at FROM install_events");
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let mut clauses: Vec<&'static str> = Vec::new();
        if let Some(since) = since_unix {
            clauses.push("occurred_at >= ?");
            params.push(rusqlite::types::Value::Integer(since));
        }
        if let Some(until) = until_unix {
            clauses.push("occurred_at <= ?");
            params.push(rusqlite::types::Value::Integer(until));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        // Walk the raw (action, occurred_at) grid and reduce into
        // per-bucket counts. We bucket in code (not SQL) because the
        // week/month flooring is calendar-aware — sqlite's strftime
        // could do it but the rounding edge cases for ISO weeks vs
        // %V-week-of-year are sharp enough that pushing the logic to
        // chrono is the safer call.
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;

        let mut acc: std::collections::HashMap<i64, ActivityBucket> =
            std::collections::HashMap::new();
        for r in rows {
            let (action_str, at) = r?;
            let bucket_start = bucket_floor_unix(at, granularity);
            let bucket = acc.entry(bucket_start).or_insert_with(|| ActivityBucket {
                bucket_start_unix: bucket_start,
                installs: 0,
                updates: 0,
                uninstalls: 0,
                failures: 0,
                total: 0,
            });
            match InstallAction::parse(&action_str) {
                InstallAction::Install => bucket.installs += 1,
                InstallAction::Update => bucket.updates += 1,
                InstallAction::Uninstall => bucket.uninstalls += 1,
                InstallAction::Failed => bucket.failures += 1,
            }
            bucket.total += 1;
        }

        // Sort ASC by bucket_start_unix so the timeline reads
        // left-to-right (oldest → newest).
        let mut out: Vec<ActivityBucket> = acc.into_values().collect();
        out.sort_by_key(|b| b.bucket_start_unix);
        Ok(out)
    }

    // ─── Activity bucket drilldown (Slice 109) ───────────────────────

    /// Per-plugin breakdown of activity inside a single activity-
    /// timeline bucket. Composes [`bucket_window_unix`] with
    /// [`Self::plugin_histogram`] so the UI can answer the natural
    /// follow-up to the Activity-over-time chart — "OK, but WHICH
    /// plugins drove that one spike?" — with one round-trip and no
    /// duplicated bucketing math.
    ///
    /// `bucket_start_unix` must be a calendar-floored boundary that
    /// matches `granularity` (i.e. came from
    /// [`Self::activity_timeline`]'s output). Callers that pass a
    /// non-floored timestamp will get a window centred on whatever
    /// bucket the wider helper would assign to it, which is well-
    /// defined but probably not what they meant — the wire-layer
    /// command floors via `bucket_floor_unix` defensively.
    ///
    /// `limit` clamps the number of returned plugins; negative
    /// clamps to zero. The UI default is 25 — same as
    /// `plugin_histogram` so the drilldown reads as "the same
    /// per-plugin lens, narrowed to one bucket".
    ///
    /// Cheap: one indexed scan over the bucket's events, the same
    /// GROUP BY plugin_id, action reduction `plugin_histogram` does,
    /// then the same DESC-by-total sort with plugin_id ASC tie-break
    /// for deterministic order across calls.
    pub fn bucket_drilldown(
        &self,
        bucket_start_unix: i64,
        granularity: TimeBucketGranularity,
        limit: i64,
    ) -> Result<Vec<PluginHistogramRow>, InstallLogError> {
        let (since, until) = bucket_window_unix(bucket_start_unix, granularity);
        self.plugin_histogram(Some(since), Some(until), limit)
    }

    // ─── Retention policy storage (Slice 63) ─────────────────────────
    //
    // Key/value rows in `install_log_settings` back the retention
    // policy surface. Two keys today:
    //
    //   retain_days          → i64; clamped to >= MIN_RETAIN_DAYS on
    //                          write so a malformed setter can't
    //                          accidentally disable retention.
    //   last_auto_prune_at   → i64 unix seconds; written by the startup
    //                          auto-prune to debounce repeated launches.
    //
    // Keys are intentionally plain strings (not an enum) so v3+
    // policy additions (e.g. a per-plugin retention override) are a
    // pure data migration with no enum bump.

    /// Read the user's retention window in days, or
    /// [`DEFAULT_RETAIN_DAYS`] if no row has been written yet.
    /// Never returns less than [`MIN_RETAIN_DAYS`] — a stored value
    /// below the floor (theoretically possible if a future bug or
    /// downgrade wrote one) clamps up so the auto-prune never wipes
    /// the entire log.
    pub fn retain_days(&self) -> Result<i64, InstallLogError> {
        let raw = self.read_setting_i64("retain_days")?;
        Ok(raw.unwrap_or(DEFAULT_RETAIN_DAYS).max(MIN_RETAIN_DAYS))
    }

    /// Persist the retention window. Clamps `days` to
    /// [`MIN_RETAIN_DAYS`] so the floor is enforced at the storage
    /// boundary (commands also clamp; double-defence is cheap).
    /// Returns the value actually stored after clamping so the caller
    /// can surface the corrected value in the UI without re-reading.
    pub fn set_retain_days(&mut self, days: i64) -> Result<i64, InstallLogError> {
        let clamped = days.max(MIN_RETAIN_DAYS);
        self.write_setting("retain_days", &clamped.to_string())?;
        Ok(clamped)
    }

    /// Unix seconds when the startup auto-prune last ran, or `None`
    /// if it has never run on this install. Used by
    /// [`auto_prune_if_due`] to debounce repeated launches.
    pub fn last_auto_prune_at(&self) -> Result<Option<i64>, InstallLogError> {
        self.read_setting_i64("last_auto_prune_at")
    }

    /// Mark the auto-prune as having just run at `at_unix`. Public so
    /// tests can pin the timestamp; production callers go through
    /// [`auto_prune_if_due`] which stamps `unix_now()`.
    pub fn set_last_auto_prune_at(&mut self, at_unix: i64) -> Result<(), InstallLogError> {
        self.write_setting("last_auto_prune_at", &at_unix.to_string())
    }

    fn read_setting_i64(&self, key: &str) -> Result<Option<i64>, InstallLogError> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM install_log_settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.and_then(|s| s.parse::<i64>().ok()))
    }

    fn write_setting(&mut self, key: &str, value: &str) -> Result<(), InstallLogError> {
        self.conn.execute(
            "INSERT INTO install_log_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ─── Per-plugin retention overrides (Slice 113) ──────────────────
    //
    // The global `retain_days` setting is one window for every plugin
    // in the install log. That's wrong for two recurring cases on a
    // production workstation:
    //
    //   1. Audit-critical plugins (compliance reports, redaction
    //      tooling, billing automations) want LONGER retention than
    //      the global default so a quarterly audit can still resolve
    //      the install/update trail.
    //   2. Noisy diagnostic plugins (telemetry collectors, profilers,
    //      preview-build sideloads) want SHORTER retention so the
    //      install log doesn't drown in events the user doesn't
    //      care about retaining.
    //
    // The override table is a thin key/value: one row per plugin id
    // that has a non-default window. Absence means "use the global
    // `retain_days`". The auto-prune driver (Slice 114) composes the
    // overrides with the global into per-plugin cutoffs.

    /// Read a single plugin's retention override, or `None` if no
    /// override row exists for that id. A value below
    /// [`MIN_RETAIN_DAYS`] clamps up on read so a stored bad value
    /// (legacy migration, future bug) never wipes the entire log for
    /// that plugin.
    pub fn plugin_retention_days(&self, plugin_id: &str) -> Result<Option<i64>, InstallLogError> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT retain_days FROM install_log_plugin_retention WHERE plugin_id = ?1",
                params![plugin_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.map(|d| d.max(MIN_RETAIN_DAYS)))
    }

    /// Persist a per-plugin retention override. Clamps `days` to
    /// [`MIN_RETAIN_DAYS`] at the storage boundary (commands also
    /// clamp; double-defence is cheap). Returns the value actually
    /// stored after clamping so the caller can surface the corrected
    /// value in the UI without re-reading.
    pub fn set_plugin_retention_days(
        &mut self,
        plugin_id: &str,
        days: i64,
    ) -> Result<i64, InstallLogError> {
        let clamped = days.max(MIN_RETAIN_DAYS);
        self.conn.execute(
            "INSERT INTO install_log_plugin_retention (plugin_id, retain_days)
             VALUES (?1, ?2)
             ON CONFLICT(plugin_id) DO UPDATE SET retain_days = excluded.retain_days",
            params![plugin_id, clamped],
        )?;
        Ok(clamped)
    }

    /// Remove a plugin's retention override. The plugin falls back to
    /// the global `retain_days` on its next auto-prune evaluation.
    /// Returns `true` if a row was actually removed, `false` if no
    /// override existed (idempotent — calling twice is a no-op on
    /// the second call).
    pub fn clear_plugin_retention(&mut self, plugin_id: &str) -> Result<bool, InstallLogError> {
        let n = self.conn.execute(
            "DELETE FROM install_log_plugin_retention WHERE plugin_id = ?1",
            params![plugin_id],
        )?;
        Ok(n > 0)
    }

    /// List every persisted per-plugin retention override. ORDER:
    /// DESC by `retain_days` (longest retention first), ASC by
    /// `plugin_id` for tie-break — same deterministic ordering as
    /// `plugin_histogram` so the UI's "Overrides" list reads top-to-
    /// bottom as "longest retention wins". Cheap O(N) on the
    /// overrides table; in practice N is small (a handful of
    /// audit-critical or diagnostic plugins per workstation).
    pub fn plugin_retention_overrides(
        &self,
    ) -> Result<Vec<PluginRetentionOverride>, InstallLogError> {
        let mut stmt = self.conn.prepare(
            "SELECT plugin_id, retain_days
             FROM install_log_plugin_retention
             ORDER BY retain_days DESC, plugin_id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PluginRetentionOverride {
                plugin_id: r.get(0)?,
                retain_days: r.get::<_, i64>(1)?.max(MIN_RETAIN_DAYS),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ─── Auto-prune driver (Slice 64) ────────────────────────────────

    /// Effective retention window in days for a single plugin id.
    /// Composes the per-plugin override (Slice 113) with the global
    /// `retain_days`: returns the override if one exists, otherwise
    /// the global setting. Floor-clamped (`>= MIN_RETAIN_DAYS`) on
    /// both sides so the resolver can't return a value that would
    /// wipe a plugin's log even if both axes contained bad data.
    ///
    /// Used by the per-plugin auto-prune driver (Slice 114) and by
    /// the wire layer when the UI needs to render "this plugin
    /// retains for N days" beside an effective-window pill.
    pub fn effective_retain_days(&self, plugin_id: &str) -> Result<i64, InstallLogError> {
        match self.plugin_retention_days(plugin_id)? {
            Some(d) => Ok(d.max(MIN_RETAIN_DAYS)),
            None => self.retain_days(),
        }
    }

    /// Run the retention policy if the debounce window has elapsed.
    ///
    /// - If `last_auto_prune_at` is missing or older than
    ///   `now_unix - AUTO_PRUNE_INTERVAL_SECS`, prune rows older
    ///   than each plugin's effective `retain_days` (per-plugin
    ///   overrides take precedence over the global) and stamp
    ///   `last_auto_prune_at = now_unix`.
    /// - Otherwise, no-op (returns
    ///   [`AutoPruneOutcome::Skipped { next_due_unix }`]).
    ///
    /// Designed to be called once on app startup. The debounce keeps
    /// the auto-prune to roughly daily even when the user launches
    /// the app many times per day (CI-style restarts, dev iteration).
    /// `now_unix` is an explicit parameter so tests can pin it
    /// deterministically; the production wrapper uses
    /// [`auto_prune_if_due_now`].
    ///
    /// Per-plugin overrides (Slice 113) are applied as scoped
    /// `DELETE WHERE plugin_id = ? AND occurred_at < ?` statements
    /// — one per override row. The global cutoff then runs as
    /// `DELETE WHERE plugin_id NOT IN (?,...,?) AND occurred_at < ?`
    /// so overridden plugins are skipped by the global pass. Two
    /// equivalent invariants fall out: (1) every event surviving an
    /// auto-prune satisfies its plugin's effective window; (2) two
    /// consecutive `auto_prune_if_due` calls with no new events
    /// between them remove zero rows on the second call.
    pub fn auto_prune_if_due(
        &mut self,
        now_unix: i64,
    ) -> Result<AutoPruneOutcome, InstallLogError> {
        let last = self.last_auto_prune_at()?;
        let due_at = last.map(|t| t + AUTO_PRUNE_INTERVAL_SECS);
        if let Some(due) = due_at {
            if now_unix < due {
                return Ok(AutoPruneOutcome::Skipped { next_due_unix: due });
            }
        }
        let retain_days = self.retain_days()?;
        let cutoff = now_unix - retain_days * 86_400;
        let overrides = self.plugin_retention_overrides()?;

        // Per-plugin pass: one DELETE per override row, scoped to
        // that plugin_id only. Sum the removed-row counts so the
        // caller sees the total work done.
        let mut overrides_rows_removed: usize = 0;
        for ov in &overrides {
            let plugin_cutoff = now_unix - ov.retain_days * 86_400;
            let n = self.conn.execute(
                "DELETE FROM install_events
                 WHERE plugin_id = ?1 AND occurred_at < ?2",
                params![ov.plugin_id, plugin_cutoff],
            )?;
            overrides_rows_removed += n;
        }

        // Global pass: everything NOT in the override set, against
        // the global cutoff. Assembled with an IN-list of overridden
        // plugin ids so the global pass and the per-plugin pass are
        // disjoint (no double-counting, no plugin getting pruned
        // against both windows).
        let global_rows_removed: usize = if overrides.is_empty() {
            self.conn.execute(
                "DELETE FROM install_events WHERE occurred_at < ?1",
                params![cutoff],
            )?
        } else {
            let mut sql = String::from(
                "DELETE FROM install_events WHERE occurred_at < ? AND plugin_id NOT IN (",
            );
            for (i, _) in overrides.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
            }
            sql.push(')');
            let mut params_v: Vec<rusqlite::types::Value> = Vec::with_capacity(overrides.len() + 1);
            params_v.push(rusqlite::types::Value::Integer(cutoff));
            for ov in &overrides {
                params_v.push(rusqlite::types::Value::Text(ov.plugin_id.clone()));
            }
            self.conn
                .execute(&sql, rusqlite::params_from_iter(params_v.iter()))?
        };

        let rows_removed = overrides_rows_removed + global_rows_removed;
        let overrides_applied = overrides.len();
        self.set_last_auto_prune_at(now_unix)?;
        Ok(AutoPruneOutcome::Pruned {
            rows_removed,
            retain_days,
            cutoff_unix: cutoff,
            overrides_applied,
            overrides_rows_removed,
        })
    }

    /// Convenience wrapper that stamps `now` from the system clock.
    /// Production callers (lib.rs startup wiring + the Tauri command)
    /// use this; tests use [`auto_prune_if_due`] directly.
    pub fn auto_prune_if_due_now(&mut self) -> Result<AutoPruneOutcome, InstallLogError> {
        self.auto_prune_if_due(unix_now())
    }
}

/// Outcome of [`InstallLog::auto_prune_if_due`]. Returned to the
/// caller so the startup wiring can log what happened (or the UI can
/// surface "Next auto-prune in 4h" when the user opens the Retention
/// section before the debounce elapses).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AutoPruneOutcome {
    /// The prune ran. `rows_removed` is the TOTAL delete count
    /// across both the per-plugin override pass and the global pass;
    /// the other fields describe what window was applied so the UI
    /// can show "Auto-pruned 23 events older than 2025-06-21
    /// (365d)" plus "(3 plugin overrides applied; 5 of 23 events
    /// from overrides)".
    ///
    /// `overrides_applied` (Slice 114) is the number of per-plugin
    /// retention overrides considered during this run (== `len()` of
    /// `plugin_retention_overrides()` at prune time);
    /// `overrides_rows_removed` is the subset of `rows_removed`
    /// attributable to those per-plugin passes. Together they let
    /// the UI surface "3 overrides cleared 5 of 23 rows" without
    /// re-querying the log.
    Pruned {
        rows_removed: usize,
        retain_days: i64,
        cutoff_unix: i64,
        /// Number of per-plugin retention overrides considered
        /// during this run. Zero on a workstation with no overrides.
        overrides_applied: usize,
        /// Subset of `rows_removed` attributable to the per-plugin
        /// override passes (rest are the global pass). Always
        /// `<= rows_removed`.
        overrides_rows_removed: usize,
    },
    /// The debounce window had not yet elapsed; no rows were
    /// touched. `next_due_unix` is when the next call will actually
    /// prune (== last_auto_prune_at + AUTO_PRUNE_INTERVAL_SECS).
    Skipped { next_due_unix: i64 },
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

/// Escape the three special characters in a SQLite LIKE pattern so a
/// user-supplied substring stays a literal match. The companion ESCAPE
/// clause in the prepared statement (`ESCAPE '\\'`) tells SQLite to
/// treat the backslash as the escape character. Matches the convention
/// the Hopper rule filter uses for its substring predicates.
///
/// Order matters: backslash MUST be replaced FIRST so we don't escape
/// the backslashes we're about to insert in front of `%` and `_`.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// ─── Export serialisers (Slice 59) ───────────────────────────────────

/// Header row for the RFC-4180 CSV export. Kept as a module constant so
/// the tests (and any future column-reorder) have a single source of
/// truth.
pub const INSTALL_LOG_CSV_HEADER: &str =
    "id,plugin_id,version,action,occurred_at_unix,occurred_at_iso,source,bytes_written,files_extracted,replaced_existing,prior_version,error_msg";

/// Render a slice of [`InstallEvent`] rows as RFC-4180 CSV. Columns
/// match what the Recent installs drawer + PluginDetailDrawer
/// Activity section show, plus the canonical audit-trail fields a
/// compliance auditor needs:
///
/// `id, plugin_id, version, action, occurred_at_unix, occurred_at_iso,
///  source, bytes_written, files_extracted, replaced_existing,
///  prior_version, error_msg`
///
/// Two timestamp columns — the raw unix-seconds value (machine-friendly
/// for joining with other audit logs) and the ISO-8601 UTC string
/// (human-friendly for direct review in Excel). Both come from the
/// same `occurred_at` field so they can't drift.
///
/// Escaping policy (RFC 4180 §2):
/// - Fields containing `,`, `"`, `\r`, `\n` are wrapped in `"`.
/// - Embedded `"` is doubled (`""`).
/// - NULL-able columns render as empty when missing — never the
///   string "None" or "null" which would trip downstream parsers.
/// - Boolean `replaced_existing` renders as `true`/`false` when
///   present, empty when NULL.
///
/// Pure function — never touches the filesystem. The Tauri command
/// layer (Slice 61) owns disk I/O.
pub fn install_log_to_csv(events: &[InstallEvent], include_header: bool) -> String {
    let mut out = String::new();
    if include_header {
        out.push_str(INSTALL_LOG_CSV_HEADER);
        out.push('\n');
    }
    for ev in events {
        let row = [
            ev.id.to_string(),
            csv_escape(&ev.plugin_id),
            csv_escape(&ev.version),
            ev.action.as_str().to_string(),
            ev.occurred_at.to_string(),
            csv_escape(&iso8601_utc(ev.occurred_at)),
            csv_escape(ev.source.as_deref().unwrap_or("")),
            opt_i64_to_string(ev.bytes_written),
            opt_i64_to_string(ev.files_extracted),
            opt_bool_to_string(ev.replaced_existing),
            csv_escape(ev.prior_version.as_deref().unwrap_or("")),
            csv_escape(ev.error_msg.as_deref().unwrap_or("")),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// Escape a single CSV field per RFC 4180. Only wraps in quotes when
/// the field actually contains a special char so the common case of
/// bare plugin ids and ASCII versions stays human-readable.
fn csv_escape(field: &str) -> String {
    let needs_quoting =
        field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r');
    if !needs_quoting {
        return field.to_string();
    }
    let escaped = field.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn opt_i64_to_string(v: Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => String::new(),
    }
}

fn opt_bool_to_string(v: Option<bool>) -> String {
    match v {
        Some(true) => "true".to_string(),
        Some(false) => "false".to_string(),
        None => String::new(),
    }
}

/// Render a unix-seconds value as a canonical ISO-8601 UTC string
/// (`2024-09-15T13:47:02Z`). Used as the `occurred_at_iso` column
/// so a CSV opened in Excel is readable without a formula. Falls
/// back to the empty string for the (pathological) case where the
/// value can't be represented — keeps the column shape consistent.
fn iso8601_utc(unix_seconds: i64) -> String {
    // chrono is already a transitive dep across the workspace; the
    // hopper module uses it for its own timestamps. We use the
    // low-level `from_timestamp` form so a negative or out-of-range
    // value gracefully degrades to empty rather than panicking.
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix_seconds, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// Floor a unix-seconds timestamp to the start of the calendar bucket
/// it lives in, per the requested [`TimeBucketGranularity`]. All
/// arithmetic is UTC so the result is deterministic across machines /
/// timezones — the UI is free to render bucket labels in local time,
/// but the boundaries themselves don't drift.
///
/// Bucket semantics:
///   - `Day`   → floor to UTC midnight (`00:00:00`)
///   - `Week`  → floor to UTC Monday `00:00:00` (ISO-8601 week start)
///   - `Month` → floor to the UTC first-of-month `00:00:00`
///
/// Falls back to the input value when the timestamp can't be
/// represented as a UTC datetime (extreme i64 bounds) — keeps the
/// bucket-key contract intact at the type boundary even when the
/// underlying calendar math would overflow. The pathological input
/// would already be unreachable for a realistic install-log row.
pub(crate) fn bucket_floor_unix(unix_seconds: i64, granularity: TimeBucketGranularity) -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let Some(dt) = chrono::DateTime::<Utc>::from_timestamp(unix_seconds, 0) else {
        return unix_seconds;
    };
    let date = dt.date_naive();
    let floored = match granularity {
        TimeBucketGranularity::Day => date,
        TimeBucketGranularity::Week => {
            // ISO-8601 week starts on Monday. weekday().num_days_from_monday()
            // returns 0 for Monday … 6 for Sunday — exactly the number of
            // days we need to subtract to reach Monday.
            let from_monday = date.weekday().num_days_from_monday() as i64;
            date - chrono::Duration::days(from_monday)
        }
        TimeBucketGranularity::Month => {
            // First of the month. Year + month are always valid for a
            // valid input date; with_day(1) cannot fail for any real
            // calendar date.
            date.with_day(1).unwrap_or(date)
        }
    };
    Utc.from_utc_datetime(
        &floored
            .and_hms_opt(0, 0, 0)
            .unwrap_or_else(|| floored.and_hms_opt(0, 0, 0).unwrap()),
    )
    .timestamp()
}

// ─── Bucket window helper (Slice 108) ────────────────────────────────

/// Given a bucket start (already calendar-floored to UTC midnight for
/// Day, ISO-Monday for Week, first-of-month for Month) and the
/// granularity that produced it, return the half-open
/// `[since_unix, until_unix]` window covering exactly that bucket.
///
/// The returned `until_unix` is **inclusive on the last second of the
/// bucket** (i.e. one second before the next bucket starts) so the
/// caller can pass it straight to [`InstallLog::list_events_between`]
/// / [`InstallLog::plugin_histogram`] whose boundaries are both
/// inclusive. This keeps the bucket-drilldown surface fully aligned
/// with the activity-timeline aggregate that produced the bucket —
/// the union of every bucket's drilldown is bit-for-bit the full
/// activity-timeline window, with no events double-counted at a
/// boundary.
///
/// Bucket lengths:
/// - `Day`   → +86_400 s   (24h, exact in UTC because the bucket
///   floor is UTC midnight; DST is a local-time concern that doesn't
///   touch UTC arithmetic).
/// - `Week`  → +7 × 86_400 (7 UTC days, exact).
/// - `Month` → calendar-aware via chrono. We compute the start of
///   the NEXT month (year/month + 1, with December rolling into
///   January of year+1) and subtract one second — so February drills
///   as 28 / 29 days, November as 30, January as 31. The `with_year` /
///   `with_month` plumbing handles year overflow.
///
/// Falls back to the input `bucket_start` for both bounds when the
/// timestamp can't be represented as a UTC datetime — same defensive
/// posture as [`bucket_floor_unix`]. The pathological input would
/// already be unreachable for a realistic activity-timeline row.
pub(crate) fn bucket_window_unix(
    bucket_start: i64,
    granularity: TimeBucketGranularity,
) -> (i64, i64) {
    use chrono::{Datelike, TimeZone, Utc};
    match granularity {
        TimeBucketGranularity::Day => (bucket_start, bucket_start + 86_400 - 1),
        TimeBucketGranularity::Week => (bucket_start, bucket_start + 7 * 86_400 - 1),
        TimeBucketGranularity::Month => {
            let Some(dt) = chrono::DateTime::<Utc>::from_timestamp(bucket_start, 0) else {
                return (bucket_start, bucket_start);
            };
            // Start of the NEXT month. December → January of year+1.
            let (next_year, next_month) = if dt.month() == 12 {
                (dt.year() + 1, 1u32)
            } else {
                (dt.year(), dt.month() + 1)
            };
            let next_start = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp())
                .unwrap_or(bucket_start);
            (bucket_start, next_start - 1)
        }
    }
}

// ─── Top plugins histogram CSV export (Slice 98) ─────────────────────

/// Header row for the per-plugin histogram CSV export. Kept as a
/// module constant so tests + future column reorders share one
/// source of truth — same convention as [`INSTALL_LOG_CSV_HEADER`]
/// and `SAMPLE_DRILLDOWN_CSV_HEADER` over in the hopper module.
///
/// Eight columns matching the [`PluginHistogramRow`] shape plus a
/// precomputed `last_occurred_at_iso` companion for human review:
///
/// ```text
/// plugin_id,installs,updates,uninstalls,failures,total,last_occurred_at_unix,last_occurred_at_iso
/// ```
pub const PLUGIN_HISTOGRAM_CSV_HEADER: &str =
    "plugin_id,installs,updates,uninstalls,failures,total,last_occurred_at_unix,last_occurred_at_iso";

/// Render a slice of [`PluginHistogramRow`] as RFC-4180 CSV. Columns
/// match what the Recent installs drawer's "Top plugins" section
/// shows, plus the canonical machine-friendly columns an auditor or
/// downstream script needs:
///
/// `plugin_id, installs, updates, uninstalls, failures, total,
///  last_occurred_at_unix, last_occurred_at_iso`
///
/// Two timestamp columns — the raw unix-seconds value (machine-friendly
/// for joining with other audit logs) and the ISO-8601 UTC string
/// (human-friendly for direct review in a spreadsheet). Both come
/// from the same `last_occurred_at` field so they can't drift.
///
/// `last_occurred_at_unix` renders as `0` (not empty) when the row
/// happens to carry a zero timestamp — matches the integer-column
/// contract of the upstream sqlite schema (the column is `NOT NULL`
/// for histogram rows since they're aggregates, not raw events).
/// The ISO column degrades to empty when the unix value can't be
/// represented, same as the install-log CSV's ISO column.
///
/// Escaping policy (RFC 4180 §2): same as [`install_log_to_csv`] —
/// fields containing `,`, `"`, `\r`, `\n` are wrapped in `"`,
/// embedded `"` is doubled. The plugin_id is the only string column
/// that can contain those characters in practice (reverse-DNS ids
/// occasionally carry hyphens but never the four trip characters);
/// the escaping shields against a future relaxation of the id format.
///
/// Pure function — never touches the filesystem. The Tauri command
/// layer (Slice 100) owns disk I/O.
pub fn plugin_histogram_to_csv(rows: &[PluginHistogramRow], include_header: bool) -> String {
    let mut out = String::new();
    if include_header {
        out.push_str(PLUGIN_HISTOGRAM_CSV_HEADER);
        out.push('\n');
    }
    for r in rows {
        let row = [
            csv_escape(&r.plugin_id),
            r.installs.to_string(),
            r.updates.to_string(),
            r.uninstalls.to_string(),
            r.failures.to_string(),
            r.total.to_string(),
            r.last_occurred_at.to_string(),
            csv_escape(&iso8601_utc(r.last_occurred_at)),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

// ─── Activity timeline CSV export (Slice 104) ────────────────────────

/// Header row for the activity-timeline CSV export. Kept as a module
/// constant so tests + future column reorders share one source of
/// truth — same convention as [`INSTALL_LOG_CSV_HEADER`] and
/// [`PLUGIN_HISTOGRAM_CSV_HEADER`].
///
/// Eight columns matching the [`ActivityBucket`] shape plus a
/// precomputed `bucket_start_iso` companion for human review AND a
/// `granularity` tag column so a downstream consumer can re-derive
/// the bucket semantics without the export filename:
///
/// ```text
/// granularity,bucket_start_unix,bucket_start_iso,installs,updates,uninstalls,failures,total
/// ```
///
/// The granularity tag is THE FIRST column rather than a constant
/// trailing column because a downstream pipeline that concatenates
/// day/week/month exports for archival reads the first column to
/// dispatch — same reasoning as why the `bucket_kind` column in the
/// drilldown CSV (slice 88) is positioned where it is.
pub const ACTIVITY_TIMELINE_CSV_HEADER: &str =
    "granularity,bucket_start_unix,bucket_start_iso,installs,updates,uninstalls,failures,total";

/// Render a slice of [`ActivityBucket`] rows as RFC-4180 CSV. Columns
/// match what the Recent installs drawer's "Activity over time"
/// section will show, plus the canonical machine-friendly columns an
/// auditor or downstream script needs:
///
/// `granularity, bucket_start_unix, bucket_start_iso, installs,
///  updates, uninstalls, failures, total`
///
/// Two timestamp-related columns — the raw unix-seconds value
/// (machine-friendly for joining with other audit logs) and the
/// ISO-8601 UTC string (human-friendly for direct review in a
/// spreadsheet). Both come from the same `bucket_start_unix` field
/// so they can't drift, matching the install-log CSV's two-column
/// timestamp pattern.
///
/// `bucket_start_unix` renders as `0` (not empty) when the bucket
/// happens to carry a zero timestamp — matches the integer-column
/// contract of the upstream sqlite schema. The ISO column degrades
/// to empty when the unix value can't be represented, same as the
/// install-log + histogram CSVs' ISO columns.
///
/// `granularity` is written verbatim as the input enum's lowercase
/// tag (`"day"` / `"week"` / `"month"`) — same value across every
/// row because a single export carries one granularity. Putting it
/// on every row (rather than once in a comment header) lets a
/// downstream pipeline concatenate day/week/month exports without
/// losing the discriminator.
///
/// Escaping policy (RFC 4180 §2): same as [`install_log_to_csv`].
/// In practice only the ISO column ever needs escaping (the integer
/// columns never contain trip characters, and the granularity tag is
/// a fixed lowercase enum) but the escaping is applied uniformly so
/// a future field addition can't slip past.
///
/// Pure function — never touches the filesystem. The Tauri command
/// layer (Slice 106) owns disk I/O.
pub fn activity_timeline_to_csv(
    buckets: &[ActivityBucket],
    granularity: TimeBucketGranularity,
    include_header: bool,
) -> String {
    let mut out = String::new();
    if include_header {
        out.push_str(ACTIVITY_TIMELINE_CSV_HEADER);
        out.push('\n');
    }
    let gran_str = granularity.as_str();
    for b in buckets {
        let row = [
            gran_str.to_string(),
            b.bucket_start_unix.to_string(),
            csv_escape(&iso8601_utc(b.bucket_start_unix)),
            b.installs.to_string(),
            b.updates.to_string(),
            b.uninstalls.to_string(),
            b.failures.to_string(),
            b.total.to_string(),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

// ─── Bucket drilldown CSV export (Slice 110) ─────────────────────────

/// Header row for the activity-bucket drilldown CSV export. Kept as
/// a module constant so tests + future column reorders share one
/// source of truth — same convention as [`PLUGIN_HISTOGRAM_CSV_HEADER`]
/// + [`ACTIVITY_TIMELINE_CSV_HEADER`].
///
/// Eleven columns matching the [`PluginHistogramRow`] body plus
/// THREE bucket-coordinate columns (`granularity`, `bucket_start_unix`,
/// `bucket_start_iso`) leading the row so a downstream pipeline
/// concatenating drilldown exports across multiple buckets can
/// dispatch on the first three cells without parsing the filename:
///
/// ```text
/// granularity,bucket_start_unix,bucket_start_iso,plugin_id,installs,updates,uninstalls,failures,total,last_occurred_at_unix,last_occurred_at_iso
/// ```
///
/// The 11-column shape extends the 8-column histogram CSV with the
/// three bucket-coordinate columns at the front. A consumer that
/// only knows about histogram CSVs can still read the per-plugin
/// columns by skipping the first three — same "extends, doesn't
/// replace" composition the round-19 drilldown CSV uses relative to
/// the install-log CSV.
pub const BUCKET_DRILLDOWN_CSV_HEADER: &str =
    "granularity,bucket_start_unix,bucket_start_iso,plugin_id,installs,updates,uninstalls,failures,total,last_occurred_at_unix,last_occurred_at_iso";

/// Render a slice of [`PluginHistogramRow`] rows scoped to a single
/// activity-timeline bucket as RFC-4180 CSV. Columns match what the
/// "Activity over time → bucket drilldown" surface shows, plus the
/// canonical machine-friendly columns an auditor or downstream script
/// needs:
///
/// `granularity, bucket_start_unix, bucket_start_iso, plugin_id,
///  installs, updates, uninstalls, failures, total,
///  last_occurred_at_unix, last_occurred_at_iso`
///
/// The first three columns identify the bucket the rows belong to.
/// Both timestamp columns (`bucket_start_iso` + `last_occurred_at_iso`)
/// are fed by the SAME [`iso8601_utc`] helper as the install-log /
/// histogram / activity-timeline CSVs so an ISO column in any of the
/// four exports matches byte-for-byte — keeps cross-export joining
/// reliable for a paralegal pivoting between surfaces.
///
/// `total` is written verbatim (NOT re-summed from the four bucket
/// columns) so a future axis added to `PluginHistogramRow` (e.g. a
/// future `rolled_back` event kind) doesn't silently corrupt totals
/// in the lag window — same defence-in-depth as the histogram /
/// activity-timeline CSV serialisers.
///
/// Escaping policy (RFC 4180 §2): same as [`install_log_to_csv`]. In
/// practice only the plugin_id column ever needs escaping; the
/// integer + ISO columns + the fixed granularity tag never carry
/// the four trip characters.
///
/// Pure function — never touches the filesystem. The Tauri command
/// layer (Slice 112) owns disk I/O.
pub fn bucket_drilldown_to_csv(
    rows: &[PluginHistogramRow],
    bucket_start_unix: i64,
    granularity: TimeBucketGranularity,
    include_header: bool,
) -> String {
    let mut out = String::new();
    if include_header {
        out.push_str(BUCKET_DRILLDOWN_CSV_HEADER);
        out.push('\n');
    }
    let gran_str = granularity.as_str();
    let bucket_iso = iso8601_utc(bucket_start_unix);
    for r in rows {
        let row = [
            gran_str.to_string(),
            bucket_start_unix.to_string(),
            csv_escape(&bucket_iso),
            csv_escape(&r.plugin_id),
            r.installs.to_string(),
            r.updates.to_string(),
            r.uninstalls.to_string(),
            r.failures.to_string(),
            r.total.to_string(),
            r.last_occurred_at.to_string(),
            csv_escape(&iso8601_utc(r.last_occurred_at)),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

// ─── JSON export envelope (Slice 60) ─────────────────────────────────

/// Wire shape for the JSON install-log export. Lifts the raw
/// [`InstallEvent`] rows into an envelope so a downstream consumer
/// can see at a glance what schema it's reading, how big the export
/// is, and when it was produced — without re-counting the array or
/// guessing at fields.
///
/// The envelope is what `slab_marketplace_install_log_export_json`
/// (Slice 61) writes to disk. The shape mirrors a generic "audit
/// export" pattern (schema_version + generated_at_iso + body) so a
/// future surface (Hopper run log export, plugin-storage backup,
/// …) can adopt the same envelope without inventing a third one.
///
/// Each event carries its own `occurred_at_iso` precomputed by the
/// serialiser so the JSON file is self-describing — a script that
/// reads the export doesn't need to know about unix-seconds or
/// install a date library to render the timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstallLogExportEnvelope {
    /// Schema version of the envelope itself (NOT of the install
    /// log's sqlite schema). Bumped when this envelope's shape
    /// changes in a non-additive way.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of when the export was produced.
    pub generated_at_iso: String,
    /// Total number of events in `events`. Redundant with
    /// `events.len()` but cheap and saves consumers a parse step.
    pub event_count: usize,
    /// Window the export was filtered by, mirroring the
    /// `list_events_between` boundaries. `null` on either side
    /// means "no bound" — i.e. the export covers everything before
    /// (or after) the other boundary.
    pub since_unix: Option<i64>,
    pub since_iso: Option<String>,
    pub until_unix: Option<i64>,
    pub until_iso: Option<String>,
    /// The events themselves, each annotated with an additional
    /// `occurred_at_iso` field so a downstream consumer can render
    /// timestamps without arithmetic.
    pub events: Vec<InstallEventExport>,
}

/// One row in the JSON export array. Wraps [`InstallEvent`] and
/// flattens its fields so the serialised form looks like a plain
/// `InstallEvent` plus the `occurred_at_iso` companion. The
/// `#[serde(flatten)]` keeps the JSON readable (no nested `event:`
/// container) while still letting us add the extra ISO column.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstallEventExport {
    #[serde(flatten)]
    pub event: InstallEvent,
    /// `event.occurred_at` rendered as an ISO-8601 UTC string.
    /// Empty when the unix-seconds value can't be represented (the
    /// same fallback the CSV serialiser uses).
    pub occurred_at_iso: String,
}

/// Schema version of the JSON export envelope. Bumped on
/// non-additive shape changes only — adding a new optional field is
/// backward-compatible at v1.
pub const INSTALL_LOG_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Build the envelope from a slice of events + the same window
/// boundaries that produced them. The envelope's `generated_at_iso`
/// stamp uses the wall clock at call time; tests pass a fixed
/// timestamp via [`install_log_to_json_with_now`].
pub fn install_log_to_json(
    events: &[InstallEvent],
    since_unix: Option<i64>,
    until_unix: Option<i64>,
) -> InstallLogExportEnvelope {
    install_log_to_json_with_now(events, since_unix, until_unix, unix_now())
}

/// Same as [`install_log_to_json`] but takes an explicit unix-seconds
/// "now" so unit tests don't race the wall clock.
pub fn install_log_to_json_with_now(
    events: &[InstallEvent],
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    now_unix: i64,
) -> InstallLogExportEnvelope {
    let events_export: Vec<InstallEventExport> = events
        .iter()
        .map(|ev| InstallEventExport {
            occurred_at_iso: iso8601_utc(ev.occurred_at),
            event: ev.clone(),
        })
        .collect();
    InstallLogExportEnvelope {
        schema_version: INSTALL_LOG_EXPORT_SCHEMA_VERSION,
        generated_at_iso: iso8601_utc(now_unix),
        event_count: events_export.len(),
        since_unix,
        since_iso: since_unix.map(iso8601_utc),
        until_unix,
        until_iso: until_unix.map(iso8601_utc),
        events: events_export,
    }
}

// ─── Top plugins histogram JSON export envelope (Slice 99) ───────────

/// Wire shape for the JSON top-plugins histogram export. Lifts the
/// raw [`PluginHistogramRow`] entries into an envelope so a downstream
/// consumer can see at a glance which schema it's reading, the
/// effective window the export covers, when it was produced, and the
/// pre-summed corpus total — without re-summing client-side or
/// guessing at provenance.
///
/// The envelope is what `slab_marketplace_install_log_export_histogram_json`
/// (Slice 100) writes to disk. Same envelope pattern as
/// [`InstallLogExportEnvelope`] (schema_version + generated_at_iso +
/// window + body) so a downstream script reading either v1 Slab
/// audit-export JSON file recognises the family by name.
///
/// `row_count` mirrors `rows.len()` and `grand_total` mirrors the
/// server's `PluginHistogramResult.grand_total` — both redundant
/// with the body but cheap to pre-compute and save a consumer a
/// parse step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginHistogramExportEnvelope {
    /// Schema version of the envelope (NOT of the histogram aggregate
    /// itself). Bumped on a non-additive shape change.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of when the export was produced.
    pub generated_at_iso: String,
    /// Number of rows in `rows`. Redundant with `rows.len()` but
    /// cheap and saves consumers a parse step.
    pub row_count: usize,
    /// Sum of every row's `total`. The corpus-wide event count within
    /// the window across the returned plugins. Pre-summed so a
    /// downstream pipeline (Splunk, Excel) can read one number instead
    /// of iterating the rows.
    pub grand_total: i64,
    /// Window the export was filtered by, mirroring the
    /// `plugin_histogram` boundaries. `null` on either side means
    /// "no bound" — i.e. the export covers everything before (or
    /// after) the other boundary.
    pub since_unix: Option<i64>,
    pub since_iso: Option<String>,
    pub until_unix: Option<i64>,
    pub until_iso: Option<String>,
    /// The histogram rows themselves. Order is the caller's order
    /// verbatim — the server emits sorted-by-total-DESC + plugin_id
    /// ASC tiebreak; the UI may have re-sorted client-side via
    /// `sortHistogramRows`. Either way, the envelope ships exactly
    /// what it gets.
    pub rows: Vec<PluginHistogramRow>,
}

/// Schema version of the JSON histogram export envelope. Starts at
/// v1; bumped independently of [`INSTALL_LOG_EXPORT_SCHEMA_VERSION`]
/// because the two envelopes' bodies are unrelated (a future shape
/// change in one shouldn't drag the other forward). Same parallel-
/// versioning reasoning as the drilldown envelope (slice 93).
pub const PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Build the envelope from a slice of histogram rows + the same
/// window boundaries that produced them + the corpus-wide
/// `grand_total` the server pre-computed. The envelope's
/// `generated_at_iso` stamp uses the wall clock at call time; tests
/// pass a fixed timestamp via [`plugin_histogram_to_json_with_now`].
pub fn plugin_histogram_to_json(
    rows: &[PluginHistogramRow],
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    grand_total: i64,
) -> PluginHistogramExportEnvelope {
    plugin_histogram_to_json_with_now(rows, since_unix, until_unix, grand_total, unix_now())
}

/// Same as [`plugin_histogram_to_json`] but takes an explicit
/// unix-seconds "now" so unit tests don't race the wall clock.
pub fn plugin_histogram_to_json_with_now(
    rows: &[PluginHistogramRow],
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    grand_total: i64,
    now_unix: i64,
) -> PluginHistogramExportEnvelope {
    PluginHistogramExportEnvelope {
        schema_version: PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION,
        generated_at_iso: iso8601_utc(now_unix),
        row_count: rows.len(),
        grand_total,
        since_unix,
        since_iso: since_unix.map(iso8601_utc),
        until_unix,
        until_iso: until_unix.map(iso8601_utc),
        rows: rows.to_vec(),
    }
}

// ─── Activity timeline JSON envelope (Slice 105) ─────────────────────

/// Wire shape for the JSON activity-timeline export. Lifts the raw
/// [`ActivityBucket`] rows into an envelope so a downstream consumer
/// can see at a glance what schema it's reading, what granularity the
/// buckets carry, how big the export is, and when it was produced —
/// without re-counting the array or guessing at fields.
///
/// Same envelope shape as [`InstallLogExportEnvelope`] (slice 60) +
/// [`PluginHistogramExportEnvelope`] (slice 99) +
/// `DrilldownExportEnvelope` (slice 93): `schema_version` +
/// `generated_at_iso` + window + body. Adds one extra discriminator
/// field — `granularity` — because the timeline body carries
/// per-bucket counts whose meaning depends on the bucket width.
///
/// `bucket_count` mirrors `buckets.len()` and `grand_total` mirrors
/// the sum of every bucket's `total` (caller-supplied verbatim, NOT
/// re-summed here — same defence-in-depth as the histogram envelope's
/// `grand_total`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityTimelineExportEnvelope {
    /// Schema version of the envelope (NOT of the activity-timeline
    /// aggregate itself). Bumped on a non-additive shape change.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of when the export was produced.
    pub generated_at_iso: String,
    /// Bucket width — `"day"` / `"week"` / `"month"`. Carries the
    /// semantics needed to interpret each bucket's
    /// `bucket_start_unix` as the START of a day / week / month
    /// window. Without this discriminator a downstream consumer
    /// would have to infer the granularity from the bucket gaps —
    /// fragile when the timeline is sparse.
    pub granularity: TimeBucketGranularity,
    /// Number of buckets in `buckets`. Redundant with
    /// `buckets.len()` but cheap and saves consumers a parse step.
    pub bucket_count: usize,
    /// Sum of every bucket's `total`. The corpus-wide event count
    /// within the window across the returned buckets. Caller-
    /// supplied verbatim (NOT re-summed here) so a future change
    /// to ActivityBucket.total semantics doesn't silently diverge.
    pub grand_total: i64,
    /// Window the export was filtered by, mirroring the
    /// `activity_timeline` boundaries. `null` on either side means
    /// "no bound" — same shape as the other v1 audit-export
    /// envelopes.
    pub since_unix: Option<i64>,
    pub since_iso: Option<String>,
    pub until_unix: Option<i64>,
    pub until_iso: Option<String>,
    /// The buckets themselves. Order is the caller's order
    /// verbatim — the server emits ASC by `bucket_start_unix`; the
    /// UI may have densified (zero-fill gap buckets) before export.
    /// Either way, the envelope ships exactly what it gets.
    pub buckets: Vec<ActivityBucket>,
}

/// Schema version of the JSON activity-timeline export envelope.
/// Starts at v1; bumped independently of the other audit-export
/// envelope constants because their bodies are unrelated (a future
/// shape change here shouldn't drag the install-log / histogram /
/// drilldown envelopes forward). Same parallel-versioning reasoning
/// as the histogram envelope (slice 99).
pub const ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Build the envelope from a slice of activity buckets + the
/// granularity + window boundaries that produced them + the corpus-
/// wide `grand_total`. The envelope's `generated_at_iso` stamp uses
/// the wall clock at call time; tests pass a fixed timestamp via
/// [`activity_timeline_to_json_with_now`].
pub fn activity_timeline_to_json(
    buckets: &[ActivityBucket],
    granularity: TimeBucketGranularity,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    grand_total: i64,
) -> ActivityTimelineExportEnvelope {
    activity_timeline_to_json_with_now(
        buckets,
        granularity,
        since_unix,
        until_unix,
        grand_total,
        unix_now(),
    )
}

/// Same as [`activity_timeline_to_json`] but takes an explicit
/// unix-seconds "now" so unit tests don't race the wall clock.
pub fn activity_timeline_to_json_with_now(
    buckets: &[ActivityBucket],
    granularity: TimeBucketGranularity,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    grand_total: i64,
    now_unix: i64,
) -> ActivityTimelineExportEnvelope {
    ActivityTimelineExportEnvelope {
        schema_version: ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION,
        generated_at_iso: iso8601_utc(now_unix),
        granularity,
        bucket_count: buckets.len(),
        grand_total,
        since_unix,
        since_iso: since_unix.map(iso8601_utc),
        until_unix,
        until_iso: until_unix.map(iso8601_utc),
        buckets: buckets.to_vec(),
    }
}

// ─── Bucket drilldown JSON export envelope (Slice 111) ───────────────

/// JSON envelope for the activity-bucket drilldown export. Mirrors
/// the [`ActivityTimelineExportEnvelope`] (slice 105) shape but
/// scopes to a SINGLE bucket and ships the per-plugin breakdown
/// instead of per-bucket counts:
///
///   `schema_version` + `generated_at_iso` + `granularity` +
///   `bucket_start_unix` + `bucket_start_iso` + `row_count` +
///   `grand_total` + `rows` (Vec<PluginHistogramRow>)
///
/// The bucket coordinates (`granularity` + `bucket_start_unix` +
/// `bucket_start_iso`) reproduce the first three columns of the CSV
/// export (slice 110) — same provenance, different surface.
///
/// `row_count` mirrors `rows.len()` and `grand_total` mirrors the
/// sum of every row's `total` (caller-supplied verbatim, NOT re-
/// summed here — same defence-in-depth as the histogram +
/// activity-timeline envelopes' `grand_total`).
///
/// No window-bounds fields (unlike the wider envelopes) because the
/// drilldown is INHERENTLY scoped to one bucket — the bucket
/// coordinates ARE the window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BucketDrilldownExportEnvelope {
    /// Schema version of the envelope (NOT of the
    /// [`PluginHistogramRow`] body itself). Bumped on a non-additive
    /// shape change.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of when the export was produced.
    pub generated_at_iso: String,
    /// Bucket width — `"day"` / `"week"` / `"month"`. Matches the
    /// activity-timeline envelope's `granularity` semantics.
    pub granularity: TimeBucketGranularity,
    /// Unix-seconds start of the bucket the rows belong to.
    pub bucket_start_unix: i64,
    /// ISO-8601 UTC form of `bucket_start_unix` — same iso8601_utc
    /// helper as every other audit export so the two timestamp
    /// representations cannot drift.
    pub bucket_start_iso: String,
    /// Number of rows in `rows`. Redundant with `rows.len()` but
    /// cheap and saves consumers a parse step.
    pub row_count: usize,
    /// Sum of every row's `total`. Caller-supplied verbatim (NOT
    /// re-summed here) so a future change to `PluginHistogramRow.total`
    /// semantics doesn't silently diverge.
    pub grand_total: i64,
    /// The per-plugin rows scoped to this bucket. Order is the
    /// caller's order verbatim — `bucket_drilldown()` emits DESC by
    /// total with plugin_id ASC tie-break; the envelope ships
    /// whatever it gets.
    pub rows: Vec<PluginHistogramRow>,
}

/// Schema version of the JSON bucket-drilldown export envelope.
/// Starts at v1; bumped independently of the other audit-export
/// envelope constants because their bodies are unrelated. Same
/// parallel-versioning reasoning as the histogram + activity-
/// timeline envelopes (slices 99 + 105).
pub const BUCKET_DRILLDOWN_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Build the envelope from a slice of per-plugin rows + the bucket
/// coordinates that produced them + the caller's `grand_total`. The
/// envelope's `generated_at_iso` stamp uses the wall clock at call
/// time; tests pass a fixed timestamp via
/// [`bucket_drilldown_to_json_with_now`].
pub fn bucket_drilldown_to_json(
    rows: &[PluginHistogramRow],
    bucket_start_unix: i64,
    granularity: TimeBucketGranularity,
    grand_total: i64,
) -> BucketDrilldownExportEnvelope {
    bucket_drilldown_to_json_with_now(
        rows,
        bucket_start_unix,
        granularity,
        grand_total,
        unix_now(),
    )
}

/// Same as [`bucket_drilldown_to_json`] but takes an explicit
/// unix-seconds "now" so unit tests don't race the wall clock.
pub fn bucket_drilldown_to_json_with_now(
    rows: &[PluginHistogramRow],
    bucket_start_unix: i64,
    granularity: TimeBucketGranularity,
    grand_total: i64,
    now_unix: i64,
) -> BucketDrilldownExportEnvelope {
    BucketDrilldownExportEnvelope {
        schema_version: BUCKET_DRILLDOWN_EXPORT_SCHEMA_VERSION,
        generated_at_iso: iso8601_utc(now_unix),
        granularity,
        bucket_start_unix,
        bucket_start_iso: iso8601_utc(bucket_start_unix),
        row_count: rows.len(),
        grand_total,
        rows: rows.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_pragma_pinned() {
        let log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.schema_version().unwrap(), SCHEMA_VERSION);
        // v3: install_log_plugin_retention table added (Slice 113
        // per-plugin retention overrides). Pure additive — every v2
        // row stays valid; the new table starts empty. Bump in
        // lockstep with init_schema arms.
        assert_eq!(SCHEMA_VERSION, 3);
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

    // ─── Retention / pruning (Slice 56) ──────────────────────────────

    /// Test-only writer that lets us pin `occurred_at` to a known
    /// unix-seconds value. We bypass the public writers (which call
    /// `unix_now()`) so the prune / oldest tests don't have to
    /// race the clock.
    fn insert_at(log: &mut InstallLog, plugin_id: &str, version: &str, at: i64) {
        log.conn
            .execute(
                "INSERT INTO install_events
                    (plugin_id, version, action, occurred_at, source,
                     bytes_written, files_extracted, replaced_existing,
                     prior_version, error_msg)
                 VALUES (?1, ?2, 'install', ?3, 'marketplace', 1, 1, 0, NULL, NULL)",
                params![plugin_id, version, at],
            )
            .unwrap();
    }

    #[test]
    fn oldest_occurred_at_empty_is_none() {
        let log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.oldest_occurred_at().unwrap(), None);
    }

    #[test]
    fn oldest_occurred_at_returns_earliest_row() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 1_000);
        insert_at(&mut log, "com.b", "1", 2_000);
        insert_at(&mut log, "com.c", "1", 500);
        assert_eq!(log.oldest_occurred_at().unwrap(), Some(500));
    }

    #[test]
    fn prune_older_than_removes_strict_predecessors() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        insert_at(&mut log, "com.b", "1", 200);
        insert_at(&mut log, "com.c", "1", 300);
        // Cutoff 200 — only the row at 100 deletes (row at 200 is
        // boundary and survives by the strict `<` predicate).
        let removed = log.prune_older_than(200).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(log.total_event_count().unwrap(), 2);
        assert_eq!(log.oldest_occurred_at().unwrap(), Some(200));
    }

    #[test]
    fn prune_older_than_empty_log_returns_zero() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let removed = log.prune_older_than(123_456_789).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn prune_older_than_is_idempotent() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        let first = log.prune_older_than(150).unwrap();
        let second = log.prune_older_than(150).unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 0);
    }

    #[test]
    fn prune_older_than_cutoff_zero_removes_nothing() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        // No row has occurred_at < 0 since unix-seconds are
        // non-negative for any wall-clock event.
        let removed = log.prune_older_than(0).unwrap();
        assert_eq!(removed, 0);
        assert_eq!(log.total_event_count().unwrap(), 1);
    }

    #[test]
    fn total_event_count_matches_inserts() {
        let mut log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.total_event_count().unwrap(), 0);
        log.record_install("com.a", "1", "marketplace", 1, 1, None)
            .unwrap();
        assert_eq!(log.total_event_count().unwrap(), 1);
        log.record_install("com.b", "1", "marketplace", 1, 1, None)
            .unwrap();
        log.record_uninstall("com.a", "1").unwrap();
        assert_eq!(log.total_event_count().unwrap(), 3);
        // After pruning, the count drops by the same number returned.
        let removed = log.prune_older_than(unix_now() + 100).unwrap();
        assert_eq!(removed, 3);
        assert_eq!(log.total_event_count().unwrap(), 0);
    }

    // ─── Time-window reader (Slice 58) ───────────────────────────────

    #[test]
    fn list_events_between_no_bounds_matches_list_recent() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        insert_at(&mut log, "com.b", "1", 200);
        insert_at(&mut log, "com.c", "1", 300);
        let bounded = log.list_events_between(None, None, 100).unwrap();
        let recent = log.list_recent(100).unwrap();
        assert_eq!(bounded.len(), recent.len());
        for (a, b) in bounded.iter().zip(recent.iter()) {
            assert_eq!(a.id, b.id);
        }
    }

    #[test]
    fn list_events_between_since_only_filters_lower_bound() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        insert_at(&mut log, "com.b", "1", 200);
        insert_at(&mut log, "com.c", "1", 300);
        let recent = log.list_events_between(Some(200), None, 100).unwrap();
        // 200 + 300 survive; 100 drops below the bound.
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].occurred_at, 300);
        assert_eq!(recent[1].occurred_at, 200);
    }

    #[test]
    fn list_events_between_until_only_filters_upper_bound() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        insert_at(&mut log, "com.b", "1", 200);
        insert_at(&mut log, "com.c", "1", 300);
        let older = log.list_events_between(None, Some(200), 100).unwrap();
        // 100 + 200 survive; 300 drops above the bound.
        assert_eq!(older.len(), 2);
        assert_eq!(older[0].occurred_at, 200);
        assert_eq!(older[1].occurred_at, 100);
    }

    #[test]
    fn list_events_between_inclusive_boundaries() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        insert_at(&mut log, "com.b", "1", 200);
        insert_at(&mut log, "com.c", "1", 300);
        // Window exactly [200, 300] — both boundaries survive.
        let win = log.list_events_between(Some(200), Some(300), 100).unwrap();
        assert_eq!(win.len(), 2);
        assert_eq!(win[0].occurred_at, 300);
        assert_eq!(win[1].occurred_at, 200);
    }

    #[test]
    fn list_events_between_empty_window_returns_empty() {
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at(&mut log, "com.a", "1", 100);
        insert_at(&mut log, "com.b", "1", 200);
        // Window that brackets nothing (gap between rows).
        let none = log.list_events_between(Some(150), Some(199), 100).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn list_events_between_limit_clamps_results() {
        let mut log = InstallLog::open_in_memory().unwrap();
        for i in 0..5 {
            insert_at(&mut log, "com.x", &format!("v{i}"), 100 + i as i64);
        }
        let two = log.list_events_between(None, None, 2).unwrap();
        assert_eq!(two.len(), 2);
        // Newest first.
        assert_eq!(two[0].occurred_at, 104);
        assert_eq!(two[1].occurred_at, 103);
        let zero = log.list_events_between(None, None, 0).unwrap();
        assert!(zero.is_empty());
        let neg = log.list_events_between(None, None, -3).unwrap();
        assert!(neg.is_empty());
    }

    // ─── Filtered reader (Slice 73) ──────────────────────────────────

    /// Test helper that lets us pin both action and `occurred_at` so the
    /// filter tests don't have to race the clock OR call four different
    /// `record_*` writers to seed a mixed-action fixture.
    fn insert_at_action(
        log: &mut InstallLog,
        plugin_id: &str,
        version: &str,
        action: InstallAction,
        at: i64,
    ) {
        log.conn
            .execute(
                "INSERT INTO install_events
                    (plugin_id, version, action, occurred_at, source,
                     bytes_written, files_extracted, replaced_existing,
                     prior_version, error_msg)
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, NULL)",
                params![plugin_id, version, action.as_str(), at],
            )
            .unwrap();
    }

    fn seed_mixed_log() -> InstallLog {
        // 8 rows, four plugins, all four actions represented:
        //   com.acme.ocr       install    @ 100
        //   com.acme.ocr-pro   update     @ 101
        //   org.studio.batch   uninstall  @ 102
        //   com.acme.ocr       failed     @ 103
        //   org.studio.batch   install    @ 104
        //   com.acme.ocr-pro   failed     @ 105
        //   org.other.app      install    @ 106
        //   com.acme.ocr       update     @ 107
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at_action(&mut log, "com.acme.ocr", "1.0", InstallAction::Install, 100);
        insert_at_action(
            &mut log,
            "com.acme.ocr-pro",
            "2.0",
            InstallAction::Update,
            101,
        );
        insert_at_action(
            &mut log,
            "org.studio.batch",
            "0.1",
            InstallAction::Uninstall,
            102,
        );
        insert_at_action(&mut log, "com.acme.ocr", "1.1", InstallAction::Failed, 103);
        insert_at_action(
            &mut log,
            "org.studio.batch",
            "0.2",
            InstallAction::Install,
            104,
        );
        insert_at_action(
            &mut log,
            "com.acme.ocr-pro",
            "2.1",
            InstallAction::Failed,
            105,
        );
        insert_at_action(
            &mut log,
            "org.other.app",
            "9.9",
            InstallAction::Install,
            106,
        );
        insert_at_action(&mut log, "com.acme.ocr", "1.2", InstallAction::Update, 107);
        log
    }

    #[test]
    fn list_events_filtered_no_axes_matches_list_recent() {
        // With all three filter axes set to None, the reader degenerates
        // to a plain newest-first scan equivalent to `list_recent`.
        let log = seed_mixed_log();
        let recent = log.list_recent(100).unwrap();
        let filtered = log
            .list_events_filtered(None, None, None, None, 100)
            .unwrap();
        assert_eq!(recent.len(), filtered.len());
        for (a, b) in recent.iter().zip(filtered.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.occurred_at, b.occurred_at);
            assert_eq!(a.action, b.action);
        }
    }

    #[test]
    fn list_events_filtered_actions_single_kind() {
        // Single-action filter: "only failures".
        let log = seed_mixed_log();
        let failures = log
            .list_events_filtered(None, None, Some(&[InstallAction::Failed]), None, 100)
            .unwrap();
        assert_eq!(failures.len(), 2);
        assert!(failures.iter().all(|e| e.action == InstallAction::Failed));
        // Newest first.
        assert_eq!(failures[0].occurred_at, 105);
        assert_eq!(failures[1].occurred_at, 103);
    }

    #[test]
    fn list_events_filtered_actions_set_with_multiple() {
        // "Installs + updates" excludes uninstall and failed.
        let log = seed_mixed_log();
        let want = log
            .list_events_filtered(
                None,
                None,
                Some(&[InstallAction::Install, InstallAction::Update]),
                None,
                100,
            )
            .unwrap();
        assert_eq!(want.len(), 5);
        for e in &want {
            assert!(matches!(
                e.action,
                InstallAction::Install | InstallAction::Update
            ));
        }
    }

    #[test]
    fn list_events_filtered_empty_action_set_is_no_filter() {
        // Empty slice MUST behave the same as None — the UI may hand
        // through an empty selection rather than special-casing it.
        let log = seed_mixed_log();
        let all = log
            .list_events_filtered(None, None, None, None, 100)
            .unwrap();
        let with_empty = log
            .list_events_filtered(None, None, Some(&[]), None, 100)
            .unwrap();
        assert_eq!(all.len(), with_empty.len());
    }

    #[test]
    fn list_events_filtered_plugin_substr_anchored_anywhere() {
        // Substring match — "acme" matches both com.acme.ocr and com.acme.ocr-pro.
        let log = seed_mixed_log();
        let acme = log
            .list_events_filtered(None, None, None, Some("acme"), 100)
            .unwrap();
        // 3 ocr rows + 2 ocr-pro rows = 5.
        assert_eq!(acme.len(), 5);
        for e in &acme {
            assert!(e.plugin_id.contains("acme"));
        }
    }

    #[test]
    fn list_events_filtered_plugin_substr_case_insensitive() {
        let log = seed_mixed_log();
        let upper = log
            .list_events_filtered(None, None, None, Some("ACME"), 100)
            .unwrap();
        let lower = log
            .list_events_filtered(None, None, None, Some("acme"), 100)
            .unwrap();
        assert_eq!(upper.len(), lower.len());
        assert!(upper.len() > 0);
    }

    #[test]
    fn list_events_filtered_plugin_substr_empty_or_whitespace_is_no_filter() {
        let log = seed_mixed_log();
        let all = log
            .list_events_filtered(None, None, None, None, 100)
            .unwrap();
        let empty = log
            .list_events_filtered(None, None, None, Some(""), 100)
            .unwrap();
        let spaces = log
            .list_events_filtered(None, None, None, Some("   "), 100)
            .unwrap();
        assert_eq!(empty.len(), all.len());
        assert_eq!(spaces.len(), all.len());
    }

    #[test]
    fn list_events_filtered_plugin_substr_no_match_returns_empty() {
        let log = seed_mixed_log();
        let none = log
            .list_events_filtered(None, None, None, Some("xyzzy"), 100)
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn list_events_filtered_plugin_substr_escapes_like_wildcards() {
        // A user pasting a literal "%" in the search box must NOT
        // accidentally become a SQL wildcard. Insert two ids — one
        // containing "%", one not — and confirm only the literal match
        // comes back.
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at_action(
            &mut log,
            "com.literal%percent",
            "1",
            InstallAction::Install,
            100,
        );
        insert_at_action(
            &mut log,
            "com.regular.name",
            "1",
            InstallAction::Install,
            101,
        );
        // Searching for "%percent" must find ONLY the literal-percent row.
        let hits = log
            .list_events_filtered(None, None, None, Some("%percent"), 100)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].plugin_id, "com.literal%percent");
        // And the underscore wildcard is also escaped.
        let mut log2 = InstallLog::open_in_memory().unwrap();
        insert_at_action(
            &mut log2,
            "com.literal_under",
            "1",
            InstallAction::Install,
            100,
        );
        insert_at_action(
            &mut log2,
            "com.regular.name",
            "1",
            InstallAction::Install,
            101,
        );
        let hits = log2
            .list_events_filtered(None, None, None, Some("l_under"), 100)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].plugin_id, "com.literal_under");
    }

    #[test]
    fn list_events_filtered_composes_all_axes_with_and() {
        // "Last events since t=102 (inclusive), failures only, plugin
        // id containing 'acme'" → only the row at 105 (com.acme.ocr-pro
        // failed) qualifies. 103 is also a failure on com.acme.ocr but
        // it's at t=103 ≥ 102 too — so two should match.
        let log = seed_mixed_log();
        let hits = log
            .list_events_filtered(
                Some(102),
                None,
                Some(&[InstallAction::Failed]),
                Some("acme"),
                100,
            )
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].occurred_at, 105);
        assert_eq!(hits[1].occurred_at, 103);
        // Narrow the window to exclude the t=103 row.
        let hits = log
            .list_events_filtered(
                Some(104),
                None,
                Some(&[InstallAction::Failed]),
                Some("acme"),
                100,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].occurred_at, 105);
    }

    #[test]
    fn list_events_filtered_limit_clamps() {
        let log = seed_mixed_log();
        let two = log.list_events_filtered(None, None, None, None, 2).unwrap();
        assert_eq!(two.len(), 2);
        let zero = log.list_events_filtered(None, None, None, None, 0).unwrap();
        assert!(zero.is_empty());
        let neg = log
            .list_events_filtered(None, None, None, None, -1)
            .unwrap();
        assert!(neg.is_empty());
    }

    #[test]
    fn recent_plugin_ids_returns_distinct_newest_first() {
        let log = seed_mixed_log();
        // 4 distinct plugin ids in the seed. Newest activity per plugin:
        //   com.acme.ocr     @ 107
        //   org.other.app    @ 106
        //   com.acme.ocr-pro @ 105
        //   org.studio.batch @ 104
        let ids = log.recent_plugin_ids(10).unwrap();
        assert_eq!(
            ids,
            vec![
                "com.acme.ocr",
                "org.other.app",
                "com.acme.ocr-pro",
                "org.studio.batch",
            ]
        );
    }

    #[test]
    fn recent_plugin_ids_caps_limit() {
        let log = seed_mixed_log();
        let two = log.recent_plugin_ids(2).unwrap();
        assert_eq!(two.len(), 2);
        // Top two newest-active ids.
        assert_eq!(two[0], "com.acme.ocr");
        assert_eq!(two[1], "org.other.app");
        let zero = log.recent_plugin_ids(0).unwrap();
        assert!(zero.is_empty());
        let neg = log.recent_plugin_ids(-1).unwrap();
        assert!(neg.is_empty());
    }

    #[test]
    fn recent_plugin_ids_empty_log_returns_empty() {
        let log = InstallLog::open_in_memory().unwrap();
        let ids = log.recent_plugin_ids(10).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn like_escape_doubles_special_chars_in_order() {
        // The order matters — backslash MUST be first so we don't
        // re-escape our own escapes.
        assert_eq!(like_escape("plain"), "plain");
        assert_eq!(like_escape("100%"), "100\\%");
        assert_eq!(like_escape("a_b"), "a\\_b");
        assert_eq!(like_escape(r"raw\path"), r"raw\\path");
        // All three together — the backslash from the raw string gets
        // doubled, the % gets prefixed with a fresh backslash.
        assert_eq!(like_escape(r"a\b_c%d"), r"a\\b\_c\%d");
    }

    // ─── CSV serialiser (Slice 59) ───────────────────────────────────

    fn sample_install_event(id: i64, plugin_id: &str, at: i64) -> InstallEvent {
        InstallEvent {
            id,
            plugin_id: plugin_id.into(),
            version: "1.2.3".into(),
            action: InstallAction::Install,
            occurred_at: at,
            source: Some("marketplace".into()),
            bytes_written: Some(42_000),
            files_extracted: Some(7),
            replaced_existing: Some(false),
            prior_version: None,
            error_msg: None,
        }
    }

    #[test]
    fn csv_header_inclusion_is_caller_controlled() {
        let events = vec![sample_install_event(1, "com.x", 1_700_000_000)];
        let with_header = install_log_to_csv(&events, true);
        assert!(with_header.starts_with(INSTALL_LOG_CSV_HEADER));
        let lines: Vec<_> = with_header.lines().collect();
        assert_eq!(lines.len(), 2); // header + one row

        let without = install_log_to_csv(&events, false);
        assert!(!without.starts_with("id,plugin_id"));
        assert_eq!(without.lines().count(), 1);
    }

    #[test]
    fn csv_empty_with_header_is_header_only() {
        let csv = install_log_to_csv(&[], true);
        assert_eq!(csv, format!("{}\n", INSTALL_LOG_CSV_HEADER));
        let bare = install_log_to_csv(&[], false);
        assert!(bare.is_empty());
    }

    #[test]
    fn csv_renders_iso_and_unix_timestamps_in_pair() {
        // Pin a well-known unix-seconds value to a known ISO string.
        // 1_700_000_000 = 2023-11-14T22:13:20Z.
        let events = vec![sample_install_event(99, "com.x", 1_700_000_000)];
        let csv = install_log_to_csv(&events, false);
        assert!(csv.contains("1700000000"));
        assert!(csv.contains("2023-11-14T22:13:20Z"));
    }

    #[test]
    fn csv_null_columns_render_as_empty_not_string_none() {
        // Uninstall row carries empty source, bytes_written, files_extracted,
        // replaced_existing, prior_version, error_msg.
        let uninstall = InstallEvent {
            id: 5,
            plugin_id: "com.x".into(),
            version: "0.1".into(),
            action: InstallAction::Uninstall,
            occurred_at: 1_700_000_000,
            source: None,
            bytes_written: None,
            files_extracted: None,
            replaced_existing: None,
            prior_version: None,
            error_msg: None,
        };
        let csv = install_log_to_csv(&[uninstall], false);
        // Cells are comma-separated. No "None", no "null".
        assert!(!csv.contains("None"));
        assert!(!csv.to_lowercase().contains("null"));
        // Tail of the row should be the six trailing empties separated
        // by commas — "uninstall,1700000000,2023-11-14T22:13:20Z,,,,,,\n"
        // (source,bytes,files,replaced,prior,error all empty).
        assert!(csv.trim_end().ends_with(",,,,,,"));
    }

    #[test]
    fn csv_escapes_commas_and_quotes_and_newlines() {
        let nasty = InstallEvent {
            id: 7,
            plugin_id: "com.acme,inc".into(), // comma
            version: "1.0\"beta\"".into(),    // quotes
            action: InstallAction::Failed,
            occurred_at: 1_700_000_000,
            source: None,
            bytes_written: None,
            files_extracted: None,
            replaced_existing: None,
            prior_version: None,
            error_msg: Some("line1\nline2".into()), // newline
        };
        let csv = install_log_to_csv(&[nasty], false);
        // Comma in plugin_id forces quotes.
        assert!(csv.contains("\"com.acme,inc\""));
        // Embedded quotes are doubled.
        assert!(csv.contains("\"1.0\"\"beta\"\"\""));
        // Newline in error_msg forces quotes around the whole field.
        assert!(csv.contains("\"line1\nline2\""));
    }

    #[test]
    fn csv_action_column_matches_serde_vocabulary() {
        // All four action kinds should round-trip the canonical
        // serde-string value used in JSON elsewhere.
        let mut events = Vec::new();
        for (i, action) in [
            (1, InstallAction::Install),
            (2, InstallAction::Update),
            (3, InstallAction::Uninstall),
            (4, InstallAction::Failed),
        ] {
            events.push(InstallEvent {
                id: i,
                plugin_id: "com.x".into(),
                version: "1.0".into(),
                action,
                occurred_at: 1_700_000_000,
                source: None,
                bytes_written: None,
                files_extracted: None,
                replaced_existing: None,
                prior_version: None,
                error_msg: None,
            });
        }
        let csv = install_log_to_csv(&events, false);
        let lines: Vec<_> = csv.lines().collect();
        assert_eq!(lines.len(), 4);
        // Column 4 (0-indexed: 3) is the action; verify it's lowercase
        // and matches the four canonical tokens.
        for (line, want) in lines
            .iter()
            .zip(["install", "update", "uninstall", "failed"])
        {
            let cells: Vec<_> = line.split(',').collect();
            assert_eq!(cells[3], want);
        }
    }

    #[test]
    fn csv_boolean_column_renders_true_false_or_empty() {
        let install_update = InstallEvent {
            id: 1,
            plugin_id: "com.x".into(),
            version: "2.0".into(),
            action: InstallAction::Update,
            occurred_at: 1_700_000_000,
            source: Some("marketplace".into()),
            bytes_written: Some(1),
            files_extracted: Some(1),
            replaced_existing: Some(true),
            prior_version: Some("1.0".into()),
            error_msg: None,
        };
        let install_fresh = sample_install_event(2, "com.y", 1_700_000_001);
        // 3rd row mimics an uninstall — replaced_existing = NULL.
        let uninstall = InstallEvent {
            replaced_existing: None,
            ..sample_install_event(3, "com.z", 1_700_000_002)
        };
        let csv = install_log_to_csv(&[install_update, install_fresh, uninstall], false);
        let lines: Vec<_> = csv.lines().collect();
        // Column 10 (0-indexed: 9) is replaced_existing.
        let col9 = |row: &str| -> String { row.split(',').nth(9).unwrap_or_default().to_string() };
        assert_eq!(col9(lines[0]), "true");
        assert_eq!(col9(lines[1]), "false");
        assert_eq!(col9(lines[2]), "");
    }

    // ─── JSON serialiser (Slice 60) ──────────────────────────────────

    #[test]
    fn json_envelope_carries_schema_and_generated_timestamp() {
        let events = vec![sample_install_event(1, "com.x", 1_700_000_000)];
        let env = install_log_to_json_with_now(&events, None, None, 1_710_000_000);
        assert_eq!(env.schema_version, INSTALL_LOG_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.generated_at_iso, "2024-03-09T16:00:00Z");
        assert_eq!(env.event_count, 1);
        assert_eq!(env.events.len(), 1);
    }

    #[test]
    fn json_envelope_window_bounds_round_trip_iso() {
        let env = install_log_to_json_with_now(&[], Some(1_700_000_000), None, 1_700_000_010);
        assert_eq!(env.since_unix, Some(1_700_000_000));
        assert_eq!(env.since_iso.as_deref(), Some("2023-11-14T22:13:20Z"));
        assert_eq!(env.until_unix, None);
        assert_eq!(env.until_iso, None);

        let env2 = install_log_to_json_with_now(
            &[],
            Some(1_700_000_000),
            Some(1_700_001_000),
            1_700_001_010,
        );
        assert_eq!(env2.until_unix, Some(1_700_001_000));
        assert_eq!(env2.until_iso.as_deref(), Some("2023-11-14T22:30:00Z"));
    }

    #[test]
    fn json_event_export_flattens_event_with_iso_companion() {
        let events = vec![sample_install_event(7, "com.x", 1_700_000_000)];
        let env = install_log_to_json_with_now(&events, None, None, 1_710_000_000);
        let ev = &env.events[0];
        assert_eq!(ev.event.id, 7);
        assert_eq!(ev.event.plugin_id, "com.x");
        assert_eq!(ev.occurred_at_iso, "2023-11-14T22:13:20Z");
        // serialise + re-parse to confirm the flatten works on the wire.
        let s = serde_json::to_string(&ev).unwrap();
        // Should contain both the flattened event field AND the ISO
        // companion at the top level (no `event:` nesting).
        assert!(s.contains("\"id\":7"));
        assert!(s.contains("\"plugin_id\":\"com.x\""));
        assert!(s.contains("\"occurred_at_iso\":\"2023-11-14T22:13:20Z\""));
        assert!(!s.contains("\"event\":"));
    }

    #[test]
    fn json_envelope_empty_events_still_renders() {
        let env = install_log_to_json_with_now(&[], None, None, 1_710_000_000);
        assert_eq!(env.event_count, 0);
        assert!(env.events.is_empty());
        // serde round-trip on the empty envelope.
        let s = serde_json::to_string(&env).unwrap();
        let back: InstallLogExportEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn json_envelope_serializes_full_roundtrip() {
        let events = vec![
            sample_install_event(1, "com.x", 1_700_000_000),
            InstallEvent {
                id: 2,
                plugin_id: "com.y".into(),
                version: "1.0".into(),
                action: InstallAction::Failed,
                occurred_at: 1_700_000_100,
                source: None,
                bytes_written: None,
                files_extracted: None,
                replaced_existing: None,
                prior_version: None,
                error_msg: Some("verify failed".into()),
            },
        ];
        let env = install_log_to_json_with_now(&events, Some(1_700_000_000), None, 1_710_000_000);
        let s = serde_json::to_string_pretty(&env).unwrap();
        let back: InstallLogExportEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, env);
        // Surface-level invariants the audit reader cares about.
        assert!(s.contains("\"schema_version\": 1"));
        assert!(s.contains("\"event_count\": 2"));
        assert!(s.contains("\"action\": \"install\""));
        assert!(s.contains("\"action\": \"failed\""));
        assert!(s.contains("\"error_msg\": \"verify failed\""));
    }

    // ─── Slice 63: retention policy storage ──────────────────────────

    #[test]
    fn retain_days_defaults_when_unset() {
        let log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.retain_days().unwrap(), DEFAULT_RETAIN_DAYS);
        assert_eq!(DEFAULT_RETAIN_DAYS, 365);
    }

    #[test]
    fn set_retain_days_persists_round_trip() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let stored = log.set_retain_days(30).unwrap();
        assert_eq!(stored, 30);
        assert_eq!(log.retain_days().unwrap(), 30);
        // Overwrite cleanly.
        let stored = log.set_retain_days(180).unwrap();
        assert_eq!(stored, 180);
        assert_eq!(log.retain_days().unwrap(), 180);
    }

    #[test]
    fn set_retain_days_clamps_below_floor() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // Storing 0 or a negative value must clamp up to
        // MIN_RETAIN_DAYS so the auto-prune can never wipe the entire
        // log (cutoff = now - 0 * 86_400 == now → deletes everything).
        let stored = log.set_retain_days(0).unwrap();
        assert_eq!(stored, MIN_RETAIN_DAYS);
        assert_eq!(log.retain_days().unwrap(), MIN_RETAIN_DAYS);
        let stored = log.set_retain_days(-9999).unwrap();
        assert_eq!(stored, MIN_RETAIN_DAYS);
    }

    #[test]
    fn last_auto_prune_at_round_trip() {
        let mut log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.last_auto_prune_at().unwrap(), None);
        log.set_last_auto_prune_at(1_700_000_000).unwrap();
        assert_eq!(log.last_auto_prune_at().unwrap(), Some(1_700_000_000));
        // Overwrites cleanly.
        log.set_last_auto_prune_at(1_700_086_400).unwrap();
        assert_eq!(log.last_auto_prune_at().unwrap(), Some(1_700_086_400));
    }

    #[test]
    fn install_log_settings_table_present_at_schema_v2() {
        // Existence check via a write+read round-trip — the migration
        // arm that creates install_log_settings is the only path that
        // gets us here without a sqlite error.
        let mut log = InstallLog::open_in_memory().unwrap();
        log.write_setting("probe", "v").unwrap();
        let v = log.read_setting_i64("nonexistent_int_key").unwrap();
        assert_eq!(v, None); // missing key → None, not error
    }

    #[test]
    fn read_setting_i64_returns_none_for_malformed_value() {
        // If somehow a non-numeric string lands in the settings table
        // (downgrade, future schema, manual sqlite poke), the reader
        // surfaces None rather than panicking. The caller falls back
        // to the default.
        let mut log = InstallLog::open_in_memory().unwrap();
        log.write_setting("retain_days", "not_a_number").unwrap();
        // retain_days() reads raw via read_setting_i64; malformed → None → default.
        assert_eq!(log.retain_days().unwrap(), DEFAULT_RETAIN_DAYS);
    }

    // ─── Slice 113: per-plugin retention overrides ───────────────────

    #[test]
    fn plugin_retention_days_unset_returns_none() {
        let log = InstallLog::open_in_memory().unwrap();
        // No override row exists; the reader returns None so the
        // effective-retention resolver (Slice 114) falls back to the
        // global retain_days.
        assert_eq!(log.plugin_retention_days("com.example.x").unwrap(), None);
    }

    #[test]
    fn set_plugin_retention_days_round_trips() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let stored = log
            .set_plugin_retention_days("com.example.audit", 1825)
            .unwrap();
        assert_eq!(stored, 1825);
        assert_eq!(
            log.plugin_retention_days("com.example.audit").unwrap(),
            Some(1825)
        );
        // Per-plugin override does NOT affect the global setting.
        assert_eq!(log.retain_days().unwrap(), DEFAULT_RETAIN_DAYS);
    }

    #[test]
    fn set_plugin_retention_days_clamps_below_floor() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let stored_zero = log.set_plugin_retention_days("com.x", 0).unwrap();
        assert_eq!(stored_zero, MIN_RETAIN_DAYS);
        let stored_neg = log.set_plugin_retention_days("com.y", -5).unwrap();
        assert_eq!(stored_neg, MIN_RETAIN_DAYS);
        // Stored value must also read back clamped (no auto-prune
        // surprise where a plugin gets wiped because a stored value
        // slipped past the floor).
        assert_eq!(
            log.plugin_retention_days("com.x").unwrap(),
            Some(MIN_RETAIN_DAYS)
        );
    }

    #[test]
    fn set_plugin_retention_days_upserts_in_place() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.set_plugin_retention_days("com.example.x", 30).unwrap();
        log.set_plugin_retention_days("com.example.x", 180).unwrap();
        // ON CONFLICT replaces the value in place — not a second row.
        assert_eq!(
            log.plugin_retention_days("com.example.x").unwrap(),
            Some(180)
        );
        let all = log.plugin_retention_overrides().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].retain_days, 180);
    }

    #[test]
    fn clear_plugin_retention_returns_removed_flag() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // Removing a non-existent override is a no-op; returns false.
        assert!(!log.clear_plugin_retention("com.nope").unwrap());
        log.set_plugin_retention_days("com.example.x", 30).unwrap();
        // First removal returns true.
        assert!(log.clear_plugin_retention("com.example.x").unwrap());
        // Second removal returns false (idempotent).
        assert!(!log.clear_plugin_retention("com.example.x").unwrap());
        assert_eq!(log.plugin_retention_days("com.example.x").unwrap(), None);
    }

    #[test]
    fn plugin_retention_overrides_empty_when_unset() {
        let log = InstallLog::open_in_memory().unwrap();
        assert!(log.plugin_retention_overrides().unwrap().is_empty());
    }

    #[test]
    fn plugin_retention_overrides_orders_desc_then_id_asc() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // Insert intentionally out-of-order so the ORDER BY clause
        // (not the insertion order) is what's pinned.
        log.set_plugin_retention_days("com.b.audit", 30).unwrap();
        log.set_plugin_retention_days("com.a.diag", 1825).unwrap();
        log.set_plugin_retention_days("com.a.audit", 30).unwrap();
        log.set_plugin_retention_days("com.c.tail", 365).unwrap();
        let rows = log.plugin_retention_overrides().unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.plugin_id.as_str()).collect();
        // 1825 (com.a.diag), 365 (com.c.tail), then both 30s ordered
        // by plugin_id ASC tie-break (com.a.audit before com.b.audit).
        assert_eq!(
            ids,
            vec!["com.a.diag", "com.c.tail", "com.a.audit", "com.b.audit"]
        );
    }

    #[test]
    fn plugin_retention_overrides_reads_clamp_below_floor() {
        // A stored value below the floor (theoretical legacy /
        // downgrade) reads back clamped, so the overrides list never
        // surfaces a value that would wipe a plugin's log.
        let log = InstallLog::open_in_memory().unwrap();
        log.conn
            .execute(
                "INSERT INTO install_log_plugin_retention (plugin_id, retain_days)
                 VALUES ('com.legacy.x', 0)",
                [],
            )
            .unwrap();
        let rows = log.plugin_retention_overrides().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].retain_days, MIN_RETAIN_DAYS);
        // Same defence on the single-plugin reader.
        assert_eq!(
            log.plugin_retention_days("com.legacy.x").unwrap(),
            Some(MIN_RETAIN_DAYS)
        );
    }

    #[test]
    fn plugin_retention_overrides_independent_per_id() {
        // Setting one plugin's override does not affect any other
        // plugin's lookup. Catches accidental "global state via
        // settings key" regression where a future refactor folds the
        // table into install_log_settings.
        let mut log = InstallLog::open_in_memory().unwrap();
        log.set_plugin_retention_days("com.audit.long", 1825)
            .unwrap();
        log.set_plugin_retention_days("com.diag.short", 7).unwrap();
        assert_eq!(
            log.plugin_retention_days("com.audit.long").unwrap(),
            Some(1825)
        );
        assert_eq!(
            log.plugin_retention_days("com.diag.short").unwrap(),
            Some(7)
        );
        assert_eq!(log.plugin_retention_days("com.other").unwrap(), None);
    }

    #[test]
    fn plugin_retention_overrides_struct_serde_round_trip() {
        // PluginRetentionOverride is exposed on the wire (Slice 117);
        // pin the field names + serde shape so a refactor that
        // renames the fields will break the test, not silently break
        // the JS bridge.
        let row = PluginRetentionOverride {
            plugin_id: "com.example.x".into(),
            retain_days: 365,
        };
        let json = serde_json::to_string(&row).unwrap();
        assert!(json.contains("\"plugin_id\":\"com.example.x\""));
        assert!(json.contains("\"retain_days\":365"));
        let back: PluginRetentionOverride = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
    }

    #[test]
    fn plugin_retention_table_present_at_schema_v3() {
        // Existence check via a write+read round-trip — the v3
        // migration arm is the only path that gets us here without a
        // sqlite "no such table" error.
        let mut log = InstallLog::open_in_memory().unwrap();
        log.set_plugin_retention_days("com.probe", 42).unwrap();
        // And the table is independent of install_log_settings —
        // writing here doesn't pollute the global retain_days key.
        assert_eq!(log.retain_days().unwrap(), DEFAULT_RETAIN_DAYS);
    }

    // ─── Slice 64: auto-prune driver ─────────────────────────────────

    #[test]
    fn auto_prune_first_call_prunes_and_stamps() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // Three events: one ancient, one boundary, one recent.
        // retain_days = 30 → cutoff = now - 30*86_400.
        // ancient < cutoff → pruned; boundary == cutoff and recent > cutoff → kept.
        let now: i64 = 1_700_000_000;
        let cutoff = now - 30 * 86_400;
        insert_at(&mut log, "com.a", "1", cutoff - 1); // pruned
        insert_at(&mut log, "com.a", "2", cutoff); // kept (>= cutoff)
        insert_at(&mut log, "com.a", "3", now); // kept
        log.set_retain_days(30).unwrap();

        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                retain_days,
                cutoff_unix,
                ..
            } => {
                assert_eq!(rows_removed, 1);
                assert_eq!(retain_days, 30);
                assert_eq!(cutoff_unix, cutoff);
            }
            AutoPruneOutcome::Skipped { .. } => panic!("first call must prune, not skip"),
        }
        assert_eq!(log.last_auto_prune_at().unwrap(), Some(now));
        assert_eq!(log.total_event_count().unwrap(), 2);
    }

    #[test]
    fn auto_prune_debounces_within_24h() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let now: i64 = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        // First call: stamps last_auto_prune_at = now.
        log.auto_prune_if_due(now).unwrap();
        // Insert a fresh prunable event right after.
        insert_at(&mut log, "com.a", "1", now - 100 * 86_400);
        // Second call within the 24h debounce window must skip.
        let outcome = log.auto_prune_if_due(now + 100).unwrap();
        match outcome {
            AutoPruneOutcome::Skipped { next_due_unix } => {
                assert_eq!(next_due_unix, now + AUTO_PRUNE_INTERVAL_SECS);
            }
            AutoPruneOutcome::Pruned { .. } => panic!("debounce should have skipped"),
        }
        // The prunable row is still there because the prune didn't run.
        assert_eq!(log.total_event_count().unwrap(), 1);
    }

    #[test]
    fn auto_prune_runs_again_after_debounce_window() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let now: i64 = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        log.auto_prune_if_due(now).unwrap();
        insert_at(&mut log, "com.a", "1", now - 100 * 86_400);
        // Advance the clock past the debounce; the prune runs again.
        let later = now + AUTO_PRUNE_INTERVAL_SECS;
        let outcome = log.auto_prune_if_due(later).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned { rows_removed, .. } => {
                assert_eq!(rows_removed, 1);
            }
            AutoPruneOutcome::Skipped { .. } => panic!("should have pruned after debounce"),
        }
        assert_eq!(log.last_auto_prune_at().unwrap(), Some(later));
    }

    #[test]
    fn auto_prune_with_no_events_succeeds_zero_rows() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let outcome = log.auto_prune_if_due(1_700_000_000).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned { rows_removed, .. } => assert_eq!(rows_removed, 0),
            AutoPruneOutcome::Skipped { .. } => panic!("first call on empty log must prune"),
        }
    }

    #[test]
    fn auto_prune_outcome_serde_tag_is_snake_case() {
        let pruned = AutoPruneOutcome::Pruned {
            rows_removed: 5,
            retain_days: 30,
            cutoff_unix: 1_700_000_000,
            overrides_applied: 0,
            overrides_rows_removed: 0,
        };
        let s = serde_json::to_string(&pruned).unwrap();
        assert!(s.contains("\"outcome\":\"pruned\""), "got {s}");
        let back: AutoPruneOutcome = serde_json::from_str(&s).unwrap();
        assert_eq!(back, pruned);

        let skipped = AutoPruneOutcome::Skipped {
            next_due_unix: 1_700_086_400,
        };
        let s = serde_json::to_string(&skipped).unwrap();
        assert!(s.contains("\"outcome\":\"skipped\""), "got {s}");
        let back: AutoPruneOutcome = serde_json::from_str(&s).unwrap();
        assert_eq!(back, skipped);
    }

    // ─── Slice 114: effective retention resolver + per-plugin auto-prune

    #[test]
    fn effective_retain_days_falls_back_to_global_when_unset() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.set_retain_days(30).unwrap();
        // No override → global wins.
        assert_eq!(log.effective_retain_days("com.example.x").unwrap(), 30);
    }

    #[test]
    fn effective_retain_days_uses_override_when_set() {
        let mut log = InstallLog::open_in_memory().unwrap();
        log.set_retain_days(30).unwrap();
        log.set_plugin_retention_days("com.audit", 1825).unwrap();
        // Override wins; global stays in place for other plugins.
        assert_eq!(log.effective_retain_days("com.audit").unwrap(), 1825);
        assert_eq!(log.effective_retain_days("com.other").unwrap(), 30);
    }

    #[test]
    fn effective_retain_days_floors_both_axes() {
        // Override-side: the storage layer already clamps writes, but
        // the resolver re-applies the floor as defence-in-depth so a
        // future direct sqlite poke can't surface a bad value.
        let mut log = InstallLog::open_in_memory().unwrap();
        log.set_retain_days(1).unwrap(); // already at floor
        log.set_plugin_retention_days("com.x", 1).unwrap();
        assert_eq!(log.effective_retain_days("com.x").unwrap(), MIN_RETAIN_DAYS);
        // Global-side floor is enforced by retain_days() itself.
        assert_eq!(
            log.effective_retain_days("com.unset").unwrap(),
            MIN_RETAIN_DAYS
        );
    }

    #[test]
    fn auto_prune_applies_per_plugin_overrides() {
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        // com.audit has a 1825d override; the 100d-old row stays.
        // com.noisy has a 7d override; the 30d-old row goes.
        // com.global has no override; uses the 365d global; 100d-old
        // row stays, 400d-old row goes.
        log.set_retain_days(365).unwrap();
        log.set_plugin_retention_days("com.audit", 1825).unwrap();
        log.set_plugin_retention_days("com.noisy", 7).unwrap();
        insert_at(&mut log, "com.audit", "1", now - 100 * 86_400); // kept (override 1825d)
        insert_at(&mut log, "com.noisy", "2", now - 30 * 86_400); // pruned (override 7d)
        insert_at(&mut log, "com.noisy", "3", now - 3 * 86_400); // kept (override 7d)
        insert_at(&mut log, "com.global", "4", now - 100 * 86_400); // kept (global 365d)
        insert_at(&mut log, "com.global", "5", now - 400 * 86_400); // pruned (global 365d)
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_applied,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 2);
                assert_eq!(overrides_applied, 2);
                assert_eq!(overrides_rows_removed, 1); // only the com.noisy 30d row
            }
            AutoPruneOutcome::Skipped { .. } => panic!("first call must prune"),
        }
        assert_eq!(log.total_event_count().unwrap(), 3);
    }

    #[test]
    fn auto_prune_override_protects_above_global() {
        // The override window is LONGER than the global. The audit
        // plugin's old events must SURVIVE the global cutoff because
        // the override's longer window protects them.
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        log.set_plugin_retention_days("com.audit", 365).unwrap();
        insert_at(&mut log, "com.audit", "1", now - 100 * 86_400); // 100d old; override 365d protects
        insert_at(&mut log, "com.other", "2", now - 100 * 86_400); // 100d old; global 30d prunes
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 1);
                assert_eq!(overrides_rows_removed, 0); // override pass removed nothing
            }
            AutoPruneOutcome::Skipped { .. } => panic!("must prune"),
        }
        // com.audit row survived; com.other row pruned.
        assert_eq!(log.total_event_count().unwrap(), 1);
        let kept = log.list_recent(10).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].plugin_id, "com.audit");
    }

    #[test]
    fn auto_prune_override_shorter_than_global_prunes_more() {
        // The override window is SHORTER than the global. The noisy
        // plugin's mid-aged events must be PRUNED even though the
        // global would have kept them.
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(365).unwrap();
        log.set_plugin_retention_days("com.noisy", 7).unwrap();
        insert_at(&mut log, "com.noisy", "1", now - 30 * 86_400); // 30d old; override 7d prunes
        insert_at(&mut log, "com.other", "2", now - 30 * 86_400); // 30d old; global 365d keeps
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 1);
                assert_eq!(overrides_rows_removed, 1); // override pass removed the com.noisy row
            }
            AutoPruneOutcome::Skipped { .. } => panic!("must prune"),
        }
        assert_eq!(log.total_event_count().unwrap(), 1);
        let kept = log.list_recent(10).unwrap();
        assert_eq!(kept[0].plugin_id, "com.other");
    }

    #[test]
    fn auto_prune_disjoint_passes_no_double_count() {
        // The override pass and the global pass must be disjoint:
        // an overridden plugin's old events go through the override
        // pass only, never the global pass. Pin by counting that a
        // single old event for an overridden plugin produces
        // overrides_rows_removed=1 + rows_removed=1 (not 2).
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        log.set_plugin_retention_days("com.both", 7).unwrap();
        insert_at(&mut log, "com.both", "1", now - 60 * 86_400); // beyond both windows
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 1);
                assert_eq!(overrides_rows_removed, 1);
            }
            AutoPruneOutcome::Skipped { .. } => panic!("must prune"),
        }
    }

    #[test]
    fn auto_prune_idempotent_with_overrides() {
        // Invariant: two consecutive auto_prune_if_due calls with no
        // new events between them remove zero rows on the second call.
        // Same invariant the no-override path enforces — must hold
        // with per-plugin overrides too.
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        log.set_plugin_retention_days("com.audit", 1825).unwrap();
        insert_at(&mut log, "com.audit", "1", now - 100 * 86_400);
        insert_at(&mut log, "com.other", "2", now - 100 * 86_400);
        let _ = log.auto_prune_if_due(now).unwrap();
        // Second call past debounce; no new events.
        let later = now + AUTO_PRUNE_INTERVAL_SECS;
        let outcome = log.auto_prune_if_due(later).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned { rows_removed, .. } => assert_eq!(rows_removed, 0),
            AutoPruneOutcome::Skipped { .. } => panic!("debounce should have elapsed"),
        }
    }

    #[test]
    fn auto_prune_no_overrides_matches_legacy_behaviour() {
        // Zero overrides → behaviour matches the round-14 single-pass
        // version: overrides_applied=0, overrides_rows_removed=0, and
        // rows_removed equals the count older than the global cutoff.
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        insert_at(&mut log, "com.a", "1", now - 100 * 86_400);
        insert_at(&mut log, "com.b", "2", now - 100 * 86_400);
        insert_at(&mut log, "com.a", "3", now);
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_applied,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 2);
                assert_eq!(overrides_applied, 0);
                assert_eq!(overrides_rows_removed, 0);
            }
            AutoPruneOutcome::Skipped { .. } => panic!("must prune"),
        }
    }

    #[test]
    fn auto_prune_overrides_for_unknown_plugin_no_op() {
        // An override for a plugin that has no events in the log
        // is fine — the per-plugin DELETE removes zero rows but the
        // override counter still increments because the plugin had
        // a policy considered. Conservative bookkeeping; UI surfaces
        // "3 overrides applied" honestly even if some matched no
        // rows.
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        log.set_plugin_retention_days("com.absent", 7).unwrap();
        insert_at(&mut log, "com.real", "1", now - 100 * 86_400);
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_applied,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 1);
                assert_eq!(overrides_applied, 1);
                assert_eq!(overrides_rows_removed, 0);
            }
            AutoPruneOutcome::Skipped { .. } => panic!("must prune"),
        }
    }

    #[test]
    fn auto_prune_overrides_in_clause_handles_many_plugins() {
        // Stress the IN-list path with many overrides — pin that
        // sqlite handles a long IN-clause cleanly and that the
        // disjoint invariant holds across all of them.
        let mut log = InstallLog::open_in_memory().unwrap();
        let now = 1_700_000_000;
        log.set_retain_days(30).unwrap();
        for i in 0..25 {
            let id = format!("com.override.{i}");
            log.set_plugin_retention_days(&id, 7).unwrap();
            insert_at(&mut log, &id, "1", now - 100 * 86_400);
        }
        insert_at(&mut log, "com.global", "1", now - 100 * 86_400);
        let outcome = log.auto_prune_if_due(now).unwrap();
        match outcome {
            AutoPruneOutcome::Pruned {
                rows_removed,
                overrides_applied,
                overrides_rows_removed,
                ..
            } => {
                assert_eq!(rows_removed, 26);
                assert_eq!(overrides_applied, 25);
                assert_eq!(overrides_rows_removed, 25);
            }
            AutoPruneOutcome::Skipped { .. } => panic!("must prune"),
        }
    }

    #[test]
    fn auto_prune_outcome_serde_has_overrides_fields() {
        // Wire compatibility: the new fields must serialise as
        // overrides_applied + overrides_rows_removed (snake_case
        // matching the existing fields' convention).
        let pruned = AutoPruneOutcome::Pruned {
            rows_removed: 5,
            retain_days: 30,
            cutoff_unix: 1_700_000_000,
            overrides_applied: 2,
            overrides_rows_removed: 3,
        };
        let s = serde_json::to_string(&pruned).unwrap();
        assert!(s.contains("\"overrides_applied\":2"), "got {s}");
        assert!(s.contains("\"overrides_rows_removed\":3"), "got {s}");
        // Round-trip preserves the new fields.
        let back: AutoPruneOutcome = serde_json::from_str(&s).unwrap();
        assert_eq!(back, pruned);
    }

    // ─── Per-plugin histogram aggregate (Slice 87) ────────────────────

    /// Compose a small histogram-shaped fixture. Three plugins with
    /// different total activity counts so a sort + cap is meaningfully
    /// observable.
    fn seed_histogram_log() -> InstallLog {
        let mut log = InstallLog::open_in_memory().unwrap();
        // com.acme.ocr: 3 installs + 1 update + 1 failed = 5 (top)
        insert_at_action(&mut log, "com.acme.ocr", "1", InstallAction::Install, 100);
        insert_at_action(&mut log, "com.acme.ocr", "2", InstallAction::Install, 101);
        insert_at_action(&mut log, "com.acme.ocr", "3", InstallAction::Install, 102);
        insert_at_action(&mut log, "com.acme.ocr", "3", InstallAction::Update, 110);
        insert_at_action(&mut log, "com.acme.ocr", "3", InstallAction::Failed, 111);
        // org.studio.batch: 2 installs + 1 uninstall = 3
        insert_at_action(
            &mut log,
            "org.studio.batch",
            "1",
            InstallAction::Install,
            200,
        );
        insert_at_action(
            &mut log,
            "org.studio.batch",
            "2",
            InstallAction::Install,
            205,
        );
        insert_at_action(
            &mut log,
            "org.studio.batch",
            "2",
            InstallAction::Uninstall,
            220,
        );
        // org.other.app: 1 install = 1
        insert_at_action(
            &mut log,
            "org.other.app",
            "0.1",
            InstallAction::Install,
            300,
        );
        log
    }

    #[test]
    fn plugin_histogram_orders_by_total_desc() {
        let log = seed_histogram_log();
        let rows = log.plugin_histogram(None, None, 100).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].plugin_id, "com.acme.ocr");
        assert_eq!(rows[0].total, 5);
        assert_eq!(rows[1].plugin_id, "org.studio.batch");
        assert_eq!(rows[1].total, 3);
        assert_eq!(rows[2].plugin_id, "org.other.app");
        assert_eq!(rows[2].total, 1);
    }

    #[test]
    fn plugin_histogram_buckets_actions_per_plugin() {
        let log = seed_histogram_log();
        let rows = log.plugin_histogram(None, None, 100).unwrap();
        let acme = &rows[0];
        assert_eq!(acme.installs, 3);
        assert_eq!(acme.updates, 1);
        assert_eq!(acme.uninstalls, 0);
        assert_eq!(acme.failures, 1);
        let batch = &rows[1];
        assert_eq!(batch.installs, 2);
        assert_eq!(batch.updates, 0);
        assert_eq!(batch.uninstalls, 1);
        assert_eq!(batch.failures, 0);
    }

    #[test]
    fn plugin_histogram_carries_last_occurred_at() {
        let log = seed_histogram_log();
        let rows = log.plugin_histogram(None, None, 100).unwrap();
        // The newest event for each plugin:
        //   com.acme.ocr: 111 (failed at 111)
        //   org.studio.batch: 220 (uninstall at 220)
        //   org.other.app: 300 (install at 300)
        assert_eq!(rows[0].last_occurred_at, 111);
        assert_eq!(rows[1].last_occurred_at, 220);
        assert_eq!(rows[2].last_occurred_at, 300);
    }

    #[test]
    fn plugin_histogram_window_filters_since() {
        let log = seed_histogram_log();
        // Filter to events at or after 200 — drops all 5 acme rows
        // (occurred at 100-111). Only batch (200/205/220) and
        // other (300) remain.
        let rows = log.plugin_histogram(Some(200), None, 100).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].plugin_id, "org.studio.batch");
        assert_eq!(rows[0].total, 3);
        assert_eq!(rows[1].plugin_id, "org.other.app");
        assert_eq!(rows[1].total, 1);
    }

    #[test]
    fn plugin_histogram_window_filters_until() {
        let log = seed_histogram_log();
        // Filter to events <= 150 — keeps the three acme installs
        // (100/101/102) and the acme update/failed (110/111). Drops
        // batch (200+) and other (300).
        let rows = log.plugin_histogram(None, Some(150), 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].plugin_id, "com.acme.ocr");
        assert_eq!(rows[0].total, 5);
    }

    #[test]
    fn plugin_histogram_window_filters_both_bounds() {
        let log = seed_histogram_log();
        // Closed interval [105, 220] keeps acme's update + failed
        // (110/111), all of batch (200/205/220), drops everything
        // else.
        let rows = log.plugin_histogram(Some(105), Some(220), 100).unwrap();
        assert_eq!(rows.len(), 2);
        let acme = rows.iter().find(|r| r.plugin_id == "com.acme.ocr").unwrap();
        assert_eq!(acme.total, 2);
        assert_eq!(acme.updates, 1);
        assert_eq!(acme.failures, 1);
        let batch = rows
            .iter()
            .find(|r| r.plugin_id == "org.studio.batch")
            .unwrap();
        assert_eq!(batch.total, 3);
    }

    #[test]
    fn plugin_histogram_empty_window_returns_empty() {
        let log = seed_histogram_log();
        // Window with no rows -> empty vec, not an error.
        let rows = log.plugin_histogram(Some(1_000), Some(2_000), 100).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn plugin_histogram_empty_log_returns_empty() {
        let log = InstallLog::open_in_memory().unwrap();
        let rows = log.plugin_histogram(None, None, 100).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn plugin_histogram_limit_caps_results() {
        let log = seed_histogram_log();
        let two = log.plugin_histogram(None, None, 2).unwrap();
        assert_eq!(two.len(), 2);
        // Top 2 by total: acme (5), batch (3).
        assert_eq!(two[0].plugin_id, "com.acme.ocr");
        assert_eq!(two[1].plugin_id, "org.studio.batch");
        let one = log.plugin_histogram(None, None, 1).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].plugin_id, "com.acme.ocr");
    }

    #[test]
    fn plugin_histogram_negative_limit_clamps_to_zero() {
        let log = seed_histogram_log();
        let rows = log.plugin_histogram(None, None, -5).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn plugin_histogram_tiebreak_on_plugin_id_ascending() {
        // Three plugins with equal totals — confirm the secondary
        // ASCENDING sort on plugin_id pins a deterministic order so
        // refreshes don't reshuffle the "Top plugins" list.
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at_action(&mut log, "zzz.plug", "1", InstallAction::Install, 100);
        insert_at_action(&mut log, "aaa.plug", "1", InstallAction::Install, 101);
        insert_at_action(&mut log, "mmm.plug", "1", InstallAction::Install, 102);
        let rows = log.plugin_histogram(None, None, 100).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].plugin_id, "aaa.plug");
        assert_eq!(rows[1].plugin_id, "mmm.plug");
        assert_eq!(rows[2].plugin_id, "zzz.plug");
    }

    #[test]
    fn plugin_histogram_total_equals_sum_of_buckets() {
        // Conservation invariant: total == installs + updates +
        // uninstalls + failures. Protects future refactors from
        // silently dropping a bucket.
        let log = seed_histogram_log();
        let rows = log.plugin_histogram(None, None, 100).unwrap();
        for r in &rows {
            assert_eq!(
                r.total,
                r.installs + r.updates + r.uninstalls + r.failures,
                "plugin {} total mismatch",
                r.plugin_id
            );
        }
    }

    #[test]
    fn plugin_histogram_serde_round_trip() {
        let row = PluginHistogramRow {
            plugin_id: "com.acme.ocr".into(),
            installs: 3,
            updates: 1,
            uninstalls: 0,
            failures: 2,
            total: 6,
            last_occurred_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&row).unwrap();
        assert!(json.contains("\"plugin_id\":\"com.acme.ocr\""));
        assert!(json.contains("\"total\":6"));
        assert!(json.contains("\"last_occurred_at\":1700000000"));
        let back: PluginHistogramRow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
    }

    // ─── Activity timeline aggregate (Slice 103) ─────────────────────

    /// Pick a known unix-seconds value to seed the timeline tests
    /// against. 1_700_000_000 = 2023-11-14T22:13:20Z (a Tuesday).
    /// Building all the test fixtures relative to this constant
    /// keeps the expected bucket-start values readable.
    const TEST_NOW_UNIX: i64 = 1_700_000_000;

    /// Compose a small activity-timeline fixture. Events spread
    /// across three calendar days (and two calendar months) so a
    /// Day/Week/Month bucketing pass produces three distinct,
    /// hand-checkable shapes.
    fn seed_timeline_log() -> InstallLog {
        let mut log = InstallLog::open_in_memory().unwrap();
        // 2023-11-14T22:13:20Z (Tuesday) — installs on day 1
        insert_at_action(&mut log, "p.a", "1", InstallAction::Install, TEST_NOW_UNIX);
        insert_at_action(
            &mut log,
            "p.b",
            "1",
            InstallAction::Install,
            TEST_NOW_UNIX + 30,
        );
        // 2023-11-15T10:00:00Z (Wednesday, same week) — update +
        // failure on day 2
        insert_at_action(&mut log, "p.a", "2", InstallAction::Update, 1_700_042_400);
        insert_at_action(&mut log, "p.c", "1", InstallAction::Failed, 1_700_042_500);
        // 2023-12-04T08:00:00Z (Monday, next month, next ISO-week)
        // — single uninstall
        insert_at_action(
            &mut log,
            "p.b",
            "1",
            InstallAction::Uninstall,
            1_701_676_800,
        );
        log
    }

    #[test]
    fn time_bucket_granularity_round_trips_via_string() {
        for g in [
            TimeBucketGranularity::Day,
            TimeBucketGranularity::Week,
            TimeBucketGranularity::Month,
        ] {
            assert_eq!(TimeBucketGranularity::parse(g.as_str()), g);
        }
    }

    #[test]
    fn time_bucket_granularity_parse_unknown_is_day() {
        // Conservative fallback: unknown string reads as the smallest
        // bucket (Day) rather than panicking.
        assert_eq!(
            TimeBucketGranularity::parse("gibberish"),
            TimeBucketGranularity::Day
        );
        assert_eq!(TimeBucketGranularity::parse(""), TimeBucketGranularity::Day);
        assert_eq!(
            TimeBucketGranularity::parse("DAY"),
            TimeBucketGranularity::Day
        );
    }

    #[test]
    fn time_bucket_granularity_serde_is_lowercase_tag() {
        // Serde rename_all = "lowercase" — the wire form is "day"/
        // "week"/"month", matching as_str(). This invariant lets a
        // TS client send the string form directly to the Tauri layer.
        let json = serde_json::to_string(&TimeBucketGranularity::Week).unwrap();
        assert_eq!(json, "\"week\"");
        let back: TimeBucketGranularity = serde_json::from_str("\"month\"").unwrap();
        assert_eq!(back, TimeBucketGranularity::Month);
    }

    #[test]
    fn bucket_floor_day_floors_to_utc_midnight() {
        // 2023-11-14T22:13:20Z floors to 2023-11-14T00:00:00Z =
        // 1_699_920_000. The 22:13:20 inside the day should be
        // erased.
        assert_eq!(
            bucket_floor_unix(TEST_NOW_UNIX, TimeBucketGranularity::Day),
            1_699_920_000
        );
    }

    #[test]
    fn bucket_floor_day_is_idempotent_at_midnight() {
        // A timestamp already on a UTC midnight floors to itself.
        let midnight = 1_699_920_000; // 2023-11-14T00:00:00Z
        assert_eq!(
            bucket_floor_unix(midnight, TimeBucketGranularity::Day),
            midnight
        );
    }

    #[test]
    fn bucket_floor_week_floors_to_iso_monday() {
        // 2023-11-14T22:13:20Z is a Tuesday — flooring to ISO-week
        // start (Monday) yields 2023-11-13T00:00:00Z = 1_699_833_600.
        assert_eq!(
            bucket_floor_unix(TEST_NOW_UNIX, TimeBucketGranularity::Week),
            1_699_833_600
        );
    }

    #[test]
    fn bucket_floor_week_for_sunday_floors_to_previous_monday() {
        // 2023-11-19T15:00:00Z is a Sunday — flooring to the
        // PREVIOUS Monday yields 2023-11-13T00:00:00Z = 1_699_833_600.
        // The ISO-8601 week convention puts Sunday at the END of the
        // week (not the start) — a common point of confusion with
        // US-week conventions.
        let sunday = 1_700_406_000;
        assert_eq!(
            bucket_floor_unix(sunday, TimeBucketGranularity::Week),
            1_699_833_600
        );
    }

    #[test]
    fn bucket_floor_week_monday_floors_to_itself() {
        // 2023-11-13T00:00:00Z is exactly the start of the ISO
        // week — flooring is idempotent.
        let monday = 1_699_833_600;
        assert_eq!(
            bucket_floor_unix(monday, TimeBucketGranularity::Week),
            monday
        );
    }

    #[test]
    fn bucket_floor_month_floors_to_first_of_month() {
        // 2023-11-14T22:13:20Z floors to 2023-11-01T00:00:00Z =
        // 1_698_796_800.
        assert_eq!(
            bucket_floor_unix(TEST_NOW_UNIX, TimeBucketGranularity::Month),
            1_698_796_800
        );
    }

    #[test]
    fn bucket_floor_month_first_of_month_floors_to_itself() {
        let first = 1_698_796_800; // 2023-11-01T00:00:00Z
        assert_eq!(
            bucket_floor_unix(first, TimeBucketGranularity::Month),
            first
        );
    }

    #[test]
    fn bucket_floor_handles_unix_epoch() {
        // Edge case: epoch zero. 1970-01-01 was a Thursday in UTC.
        // - Day:   already UTC midnight → 0
        // - Week:  floors to the prior Monday (1969-12-29) = -259_200
        //          (Thursday is 3 days into the ISO week, so 3 * 86_400)
        // - Month: 1970-01-01 is already first-of-month → 0
        assert_eq!(bucket_floor_unix(0, TimeBucketGranularity::Day), 0);
        assert_eq!(bucket_floor_unix(0, TimeBucketGranularity::Week), -259_200);
        assert_eq!(bucket_floor_unix(0, TimeBucketGranularity::Month), 0);
    }

    // ── Slice 108 — bucket_window_unix ───────────────────────────────

    #[test]
    fn bucket_window_day_spans_exactly_86_399_seconds() {
        // A daily bucket runs from its UTC midnight start to one
        // second before the next UTC midnight (so back-to-back buckets
        // never overlap and the union covers every second of the
        // calendar day).
        let start = 1_699_920_000; // 2023-11-14T00:00:00Z
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Day);
        assert_eq!(since, start);
        assert_eq!(until, start + 86_399);
        assert_eq!(until - since, 86_399);
    }

    #[test]
    fn bucket_window_week_spans_exactly_seven_days_minus_one_second() {
        let start = 1_699_833_600; // 2023-11-13T00:00:00Z Monday
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Week);
        assert_eq!(since, start);
        assert_eq!(until, start + 7 * 86_400 - 1);
    }

    #[test]
    fn bucket_window_month_january_has_31_days() {
        // 2024-01 is a 31-day month → window = 31 * 86_400 - 1.
        let start = 1_704_067_200; // 2024-01-01T00:00:00Z
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Month);
        assert_eq!(since, start);
        assert_eq!(until, start + 31 * 86_400 - 1);
    }

    #[test]
    fn bucket_window_month_february_2024_is_leap_year_29_days() {
        // 2024 is a leap year — February runs 29 days.
        let start = 1_706_745_600; // 2024-02-01T00:00:00Z
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Month);
        assert_eq!(since, start);
        assert_eq!(until, start + 29 * 86_400 - 1);
    }

    #[test]
    fn bucket_window_month_february_2023_is_28_days() {
        // 2023 is NOT a leap year — February runs 28 days. Pins the
        // calendar-aware month length against a hand-built 28-day
        // baseline so a future drift in chrono's leap-year math
        // surfaces here rather than in a stale assertion.
        let start = 1_675_209_600; // 2023-02-01T00:00:00Z
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Month);
        assert_eq!(since, start);
        assert_eq!(until, start + 28 * 86_400 - 1);
    }

    #[test]
    fn bucket_window_month_november_has_30_days() {
        // 2023-11 is a 30-day month.
        let start = 1_698_796_800; // 2023-11-01T00:00:00Z
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Month);
        assert_eq!(since, start);
        assert_eq!(until, start + 30 * 86_400 - 1);
    }

    #[test]
    fn bucket_window_month_december_rolls_into_january_next_year() {
        // December → next month is January of year+1. Without the
        // year-overflow plumbing in bucket_window_unix this would
        // produce month=13 which chrono rejects and the helper
        // would fall back to (start, start).
        let start = 1_701_388_800; // 2023-12-01T00:00:00Z
        let (since, until) = bucket_window_unix(start, TimeBucketGranularity::Month);
        assert_eq!(since, start);
        // Dec 2023 has 31 days.
        assert_eq!(until, start + 31 * 86_400 - 1);
    }

    #[test]
    fn bucket_window_back_to_back_buckets_never_overlap() {
        // Two consecutive daily buckets: the end of bucket N must be
        // exactly one second before the start of bucket N+1, so no
        // event with a unix-second timestamp can be counted twice and
        // no event can fall in a gap.
        let start_a = 1_699_920_000; // 2023-11-14T00:00:00Z
        let start_b = 1_700_006_400; // 2023-11-15T00:00:00Z
        let (_, until_a) = bucket_window_unix(start_a, TimeBucketGranularity::Day);
        let (since_b, _) = bucket_window_unix(start_b, TimeBucketGranularity::Day);
        assert_eq!(since_b, until_a + 1);
    }

    #[test]
    fn bucket_window_compose_with_bucket_floor_round_trip() {
        // For any timestamp ts in (bucket_window of floor(ts)) is an
        // invariant: floor(ts) ≤ ts ≤ floor(ts) + bucket_length - 1.
        // Pin this for one timestamp per granularity so a future
        // change in either helper (floor or window) that breaks the
        // composition surfaces.
        for g in [
            TimeBucketGranularity::Day,
            TimeBucketGranularity::Week,
            TimeBucketGranularity::Month,
        ] {
            let ts = 1_700_059_200; // 2023-11-15T12:00:00Z (mid-day)
            let start = bucket_floor_unix(ts, g);
            let (since, until) = bucket_window_unix(start, g);
            assert!(since <= ts, "since {since} > ts {ts} for {g:?}");
            assert!(ts <= until, "ts {ts} > until {until} for {g:?}");
        }
    }

    #[test]
    fn activity_timeline_empty_log_returns_empty() {
        let log = InstallLog::open_in_memory().unwrap();
        let rows = log
            .activity_timeline(None, None, TimeBucketGranularity::Day)
            .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn activity_timeline_day_buckets_per_calendar_day() {
        let log = seed_timeline_log();
        let buckets = log
            .activity_timeline(None, None, TimeBucketGranularity::Day)
            .unwrap();
        // Three distinct UTC days: 2023-11-14, 2023-11-15, 2023-12-04.
        assert_eq!(buckets.len(), 3);
        // ASC order — oldest first.
        assert!(buckets[0].bucket_start_unix < buckets[1].bucket_start_unix);
        assert!(buckets[1].bucket_start_unix < buckets[2].bucket_start_unix);
        // First bucket: two installs on 2023-11-14.
        assert_eq!(buckets[0].bucket_start_unix, 1_699_920_000);
        assert_eq!(buckets[0].installs, 2);
        assert_eq!(buckets[0].total, 2);
        // Second bucket: one update + one failure on 2023-11-15.
        assert_eq!(buckets[1].bucket_start_unix, 1_700_006_400);
        assert_eq!(buckets[1].updates, 1);
        assert_eq!(buckets[1].failures, 1);
        assert_eq!(buckets[1].total, 2);
        // Third bucket: one uninstall on 2023-12-04.
        assert_eq!(buckets[2].bucket_start_unix, 1_701_648_000);
        assert_eq!(buckets[2].uninstalls, 1);
        assert_eq!(buckets[2].total, 1);
    }

    #[test]
    fn activity_timeline_week_buckets_collapse_same_week() {
        let log = seed_timeline_log();
        let buckets = log
            .activity_timeline(None, None, TimeBucketGranularity::Week)
            .unwrap();
        // 2023-11-14 and 2023-11-15 are both in the same ISO-week
        // (week starting Mon 2023-11-13). 2023-12-04 is in the
        // week starting Mon 2023-12-04. So two buckets.
        assert_eq!(buckets.len(), 2);
        // First bucket: week of 2023-11-13 — collapses day 1 + day 2
        // events. 2 installs + 1 update + 1 failure = 4.
        assert_eq!(buckets[0].bucket_start_unix, 1_699_833_600);
        assert_eq!(buckets[0].installs, 2);
        assert_eq!(buckets[0].updates, 1);
        assert_eq!(buckets[0].failures, 1);
        assert_eq!(buckets[0].total, 4);
        // Second bucket: week of 2023-12-04 — single uninstall.
        assert_eq!(buckets[1].bucket_start_unix, 1_701_648_000);
        assert_eq!(buckets[1].uninstalls, 1);
        assert_eq!(buckets[1].total, 1);
    }

    #[test]
    fn activity_timeline_month_buckets_collapse_same_month() {
        let log = seed_timeline_log();
        let buckets = log
            .activity_timeline(None, None, TimeBucketGranularity::Month)
            .unwrap();
        // 2023-11 collapses days 1+2; 2023-12 has the uninstall.
        // Two buckets.
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].bucket_start_unix, 1_698_796_800); // 2023-11-01
        assert_eq!(buckets[0].installs, 2);
        assert_eq!(buckets[0].updates, 1);
        assert_eq!(buckets[0].failures, 1);
        assert_eq!(buckets[0].total, 4);
        assert_eq!(buckets[1].bucket_start_unix, 1_701_388_800); // 2023-12-01
        assert_eq!(buckets[1].uninstalls, 1);
        assert_eq!(buckets[1].total, 1);
    }

    #[test]
    fn activity_timeline_window_filters_since() {
        let log = seed_timeline_log();
        // since = 2023-11-15T00:00:00Z = 1_700_006_400 — drops the
        // day-1 installs, keeps the day-2 events + the December
        // uninstall.
        let buckets = log
            .activity_timeline(Some(1_700_006_400), None, TimeBucketGranularity::Day)
            .unwrap();
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].installs, 0);
        assert_eq!(buckets[0].updates, 1);
        assert_eq!(buckets[0].failures, 1);
        assert_eq!(buckets[1].uninstalls, 1);
    }

    #[test]
    fn activity_timeline_window_filters_until() {
        let log = seed_timeline_log();
        // until = 2023-11-30T23:59:59Z = 1_701_388_799 — keeps
        // November, drops December's uninstall.
        let buckets = log
            .activity_timeline(None, Some(1_701_388_799), TimeBucketGranularity::Day)
            .unwrap();
        assert_eq!(buckets.len(), 2);
        assert!(buckets.iter().all(|b| b.uninstalls == 0));
    }

    #[test]
    fn activity_timeline_window_empty_returns_empty() {
        let log = seed_timeline_log();
        let buckets = log
            .activity_timeline(Some(1_800_000_000), None, TimeBucketGranularity::Day)
            .unwrap();
        assert!(buckets.is_empty());
    }

    #[test]
    fn activity_timeline_bucket_total_equals_sum_of_buckets_invariant() {
        // Conservation invariant: each bucket's `total` equals
        // installs + updates + uninstalls + failures.
        let log = seed_timeline_log();
        for g in [
            TimeBucketGranularity::Day,
            TimeBucketGranularity::Week,
            TimeBucketGranularity::Month,
        ] {
            let buckets = log.activity_timeline(None, None, g).unwrap();
            for b in &buckets {
                assert_eq!(
                    b.total,
                    b.installs + b.updates + b.uninstalls + b.failures,
                    "{:?} bucket at {} total mismatch",
                    g,
                    b.bucket_start_unix
                );
            }
        }
    }

    #[test]
    fn activity_timeline_sparse_only_non_empty_buckets() {
        // Two events months apart — the gap months/weeks/days between
        // them are NOT in the output. The UI densifies; the
        // primitive stays sparse.
        let mut log = InstallLog::open_in_memory().unwrap();
        insert_at_action(&mut log, "p", "1", InstallAction::Install, 1_700_000_000);
        insert_at_action(&mut log, "p", "1", InstallAction::Update, 1_710_000_000);
        let buckets = log
            .activity_timeline(None, None, TimeBucketGranularity::Day)
            .unwrap();
        // Exactly two buckets — no zero-filled gap.
        assert_eq!(buckets.len(), 2);
        assert!(buckets.iter().all(|b| b.total > 0));
    }

    #[test]
    fn activity_timeline_ascending_order_invariant() {
        // Ten events on five distinct days in random insertion
        // order — output must still be ASC by bucket_start_unix.
        let mut log = InstallLog::open_in_memory().unwrap();
        // Five day-floors spaced one day apart starting 2024-01-01:
        // 2024-01-05 first, then 03, 01, 04, 02 — out of order.
        let day = 86_400;
        let base = 1_704_067_200; // 2024-01-01T00:00:00Z
        for offset in [4, 2, 0, 3, 1] {
            insert_at_action(
                &mut log,
                "p",
                "1",
                InstallAction::Install,
                base + offset * day + 3_600, // mid-day
            );
        }
        let buckets = log
            .activity_timeline(None, None, TimeBucketGranularity::Day)
            .unwrap();
        assert_eq!(buckets.len(), 5);
        for i in 1..buckets.len() {
            assert!(
                buckets[i].bucket_start_unix > buckets[i - 1].bucket_start_unix,
                "buckets out of order at index {i}"
            );
        }
    }

    // ── Slice 109 — bucket_drilldown ─────────────────────────────────

    #[test]
    fn bucket_drilldown_returns_only_plugins_active_in_bucket() {
        // seed_timeline_log puts 2 installs on 2023-11-14 (p.a + p.b),
        // 1 update + 1 failure on 2023-11-15 (p.a + p.c), and 1
        // uninstall on 2023-12-04 (p.b). Drilling into the 11-14
        // daily bucket should surface ONLY p.a and p.b.
        let log = seed_timeline_log();
        let bucket_start = 1_699_920_000; // 2023-11-14T00:00:00Z
        let rows = log
            .bucket_drilldown(bucket_start, TimeBucketGranularity::Day, 25)
            .unwrap();
        assert_eq!(rows.len(), 2);
        let ids: Vec<&str> = rows.iter().map(|r| r.plugin_id.as_str()).collect();
        assert!(ids.contains(&"p.a"));
        assert!(ids.contains(&"p.b"));
        assert!(!ids.contains(&"p.c"), "p.c shouldn't drill into day 1");
    }

    #[test]
    fn bucket_drilldown_day_grain_excludes_next_day_events() {
        // The day-2 update (2023-11-15) must not bleed into the day-1
        // bucket — pins the bucket_window inclusive-second boundary.
        let log = seed_timeline_log();
        let bucket_start = 1_699_920_000; // 2023-11-14T00:00:00Z
        let rows = log
            .bucket_drilldown(bucket_start, TimeBucketGranularity::Day, 25)
            .unwrap();
        // p.a has 1 install on day 1; the update on day 2 is excluded.
        let pa = rows.iter().find(|r| r.plugin_id == "p.a").unwrap();
        assert_eq!(pa.installs, 1);
        assert_eq!(pa.updates, 0);
        assert_eq!(pa.total, 1);
    }

    #[test]
    fn bucket_drilldown_week_grain_collapses_both_days() {
        // The 2023-11-13 week bucket holds day 1 + day 2 events:
        // p.a 1 install + 1 update, p.b 1 install, p.c 1 failure.
        let log = seed_timeline_log();
        let week_start = 1_699_833_600; // 2023-11-13T00:00:00Z Monday
        let rows = log
            .bucket_drilldown(week_start, TimeBucketGranularity::Week, 25)
            .unwrap();
        assert_eq!(rows.len(), 3);
        let pa = rows.iter().find(|r| r.plugin_id == "p.a").unwrap();
        assert_eq!(pa.installs, 1);
        assert_eq!(pa.updates, 1);
        assert_eq!(pa.total, 2);
        // The December uninstall (p.b) is in a later week, NOT here.
        let pb = rows.iter().find(|r| r.plugin_id == "p.b").unwrap();
        assert_eq!(pb.installs, 1);
        assert_eq!(pb.uninstalls, 0);
        let pc = rows.iter().find(|r| r.plugin_id == "p.c").unwrap();
        assert_eq!(pc.failures, 1);
        assert_eq!(pc.total, 1);
    }

    #[test]
    fn bucket_drilldown_month_grain_separates_november_december() {
        let log = seed_timeline_log();
        // 2023-11 bucket: everything except the December uninstall.
        let nov_rows = log
            .bucket_drilldown(1_698_796_800, TimeBucketGranularity::Month, 25)
            .unwrap();
        assert_eq!(nov_rows.len(), 3);
        let pb = nov_rows.iter().find(|r| r.plugin_id == "p.b").unwrap();
        assert_eq!(pb.uninstalls, 0, "Dec uninstall mustn't bleed into Nov");
        // 2023-12 bucket: just the uninstall.
        let dec_rows = log
            .bucket_drilldown(1_701_388_800, TimeBucketGranularity::Month, 25)
            .unwrap();
        assert_eq!(dec_rows.len(), 1);
        assert_eq!(dec_rows[0].plugin_id, "p.b");
        assert_eq!(dec_rows[0].uninstalls, 1);
    }

    #[test]
    fn bucket_drilldown_total_matches_activity_timeline_bucket_total() {
        // CONSERVATION INVARIANT: for every bucket the
        // activity-timeline aggregate emits, summing the per-plugin
        // totals from bucket_drilldown for the same bucket reproduces
        // bucket.total — the two surfaces are independent
        // aggregations of the same underlying events and they
        // CAN'T diverge.
        let log = seed_timeline_log();
        for g in [
            TimeBucketGranularity::Day,
            TimeBucketGranularity::Week,
            TimeBucketGranularity::Month,
        ] {
            let buckets = log.activity_timeline(None, None, g).unwrap();
            for b in &buckets {
                let rows = log.bucket_drilldown(b.bucket_start_unix, g, 1000).unwrap();
                let sum: i64 = rows.iter().map(|r| r.total).sum();
                assert_eq!(
                    sum, b.total,
                    "bucket {:?} grain {:?}: drilldown sum {} != bucket total {}",
                    b.bucket_start_unix, g, sum, b.total
                );
            }
        }
    }

    #[test]
    fn bucket_drilldown_orders_desc_by_total_with_id_tiebreak() {
        // Pin the same sort contract plugin_histogram observes:
        // primary DESC by total, secondary ASC by plugin_id. Build a
        // bucket where two plugins tie at total=2 to exercise the
        // tie-break.
        let mut log = InstallLog::open_in_memory().unwrap();
        // 2023-11-14T00:00:00Z — base of the day-1 bucket.
        let base = 1_699_920_000;
        // Plugin a — 2 installs
        insert_at_action(&mut log, "p.a", "1", InstallAction::Install, base + 100);
        insert_at_action(&mut log, "p.a", "1", InstallAction::Install, base + 200);
        // Plugin b — 2 installs (ties with a; b > a so a comes first)
        insert_at_action(&mut log, "p.b", "1", InstallAction::Install, base + 300);
        insert_at_action(&mut log, "p.b", "1", InstallAction::Install, base + 400);
        // Plugin c — 3 installs (highest, ranks first)
        for _ in 0..3 {
            insert_at_action(&mut log, "p.c", "1", InstallAction::Install, base + 500);
        }
        let rows = log
            .bucket_drilldown(base, TimeBucketGranularity::Day, 25)
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].plugin_id, "p.c"); // highest total
        assert_eq!(rows[1].plugin_id, "p.a"); // ties at 2, a < b
        assert_eq!(rows[2].plugin_id, "p.b");
    }

    #[test]
    fn bucket_drilldown_empty_bucket_returns_empty() {
        let log = seed_timeline_log();
        // 2023-12-25 — a day with no events in the seed fixture.
        let rows = log
            .bucket_drilldown(1_703_462_400, TimeBucketGranularity::Day, 25)
            .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn bucket_drilldown_limit_caps_results() {
        let mut log = InstallLog::open_in_memory().unwrap();
        // 2023-11-14T00:00:00Z — day-1 base.
        let base = 1_699_920_000;
        // 7 distinct plugins with one install each, all on day 1.
        for i in 0..7 {
            let id = format!("p.{i:02}");
            insert_at_action(&mut log, &id, "1", InstallAction::Install, base + i * 100);
        }
        let rows = log
            .bucket_drilldown(base, TimeBucketGranularity::Day, 3)
            .unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn bucket_drilldown_negative_limit_clamps_to_zero() {
        let log = seed_timeline_log();
        let rows = log
            .bucket_drilldown(1_699_920_000, TimeBucketGranularity::Day, -5)
            .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn activity_bucket_serde_round_trip() {
        let bucket = ActivityBucket {
            bucket_start_unix: 1_700_000_000,
            installs: 3,
            updates: 2,
            uninstalls: 1,
            failures: 4,
            total: 10,
        };
        let json = serde_json::to_string(&bucket).unwrap();
        assert!(json.contains("\"bucket_start_unix\":1700000000"));
        assert!(json.contains("\"installs\":3"));
        assert!(json.contains("\"total\":10"));
        let back: ActivityBucket = serde_json::from_str(&json).unwrap();
        assert_eq!(back, bucket);
    }

    // ─── Slice 98: histogram CSV export ──────────────────────────────

    fn hist_row(
        plugin_id: &str,
        installs: i64,
        updates: i64,
        uninstalls: i64,
        failures: i64,
        last_occurred_at: i64,
    ) -> PluginHistogramRow {
        PluginHistogramRow {
            plugin_id: plugin_id.into(),
            installs,
            updates,
            uninstalls,
            failures,
            total: installs + updates + uninstalls + failures,
            last_occurred_at,
        }
    }

    #[test]
    fn histogram_csv_header_inclusion_is_caller_controlled() {
        let rows = vec![hist_row("com.x", 1, 0, 0, 0, 1_700_000_000)];
        let with_header = plugin_histogram_to_csv(&rows, true);
        assert!(with_header.starts_with(PLUGIN_HISTOGRAM_CSV_HEADER));
        let lines: Vec<_> = with_header.lines().collect();
        assert_eq!(lines.len(), 2, "header + one data row");
        let bare = plugin_histogram_to_csv(&rows, false);
        assert!(!bare.starts_with("plugin_id,"));
        assert_eq!(bare.lines().count(), 1);
    }

    #[test]
    fn histogram_csv_empty_with_header_is_header_only() {
        let csv = plugin_histogram_to_csv(&[], true);
        assert_eq!(csv, format!("{}\n", PLUGIN_HISTOGRAM_CSV_HEADER));
        let bare = plugin_histogram_to_csv(&[], false);
        assert!(bare.is_empty());
    }

    #[test]
    fn histogram_csv_header_column_count_matches_row() {
        // Defensive: header and every row carry the same number of
        // comma-separated fields. Protects future column additions
        // from drifting one side without the other.
        let rows = vec![hist_row("com.x", 1, 2, 3, 4, 1_700_000_000)];
        let csv = plugin_histogram_to_csv(&rows, true);
        let lines: Vec<_> = csv.lines().collect();
        let header_cols = lines[0].split(',').count();
        let row_cols = lines[1].split(',').count();
        assert_eq!(
            header_cols, row_cols,
            "header has {header_cols} cols, row has {row_cols}"
        );
        assert_eq!(header_cols, 8, "expected 8 columns");
    }

    #[test]
    fn histogram_csv_columns_in_documented_order() {
        // Pin the column order so a refactor that reorders the
        // header constant has to update this test in lockstep with
        // the doc comment.
        let row = hist_row("com.acme.ocr", 3, 1, 0, 2, 1_700_000_000);
        let csv = plugin_histogram_to_csv(&[row], false);
        // Single row → single line. The eight cells are:
        //   plugin_id, installs, updates, uninstalls, failures, total,
        //   last_occurred_at_unix, last_occurred_at_iso
        let line = csv.lines().next().unwrap();
        let cells: Vec<&str> = line.split(',').collect();
        assert_eq!(cells[0], "com.acme.ocr");
        assert_eq!(cells[1], "3");
        assert_eq!(cells[2], "1");
        assert_eq!(cells[3], "0");
        assert_eq!(cells[4], "2");
        assert_eq!(cells[5], "6");
        assert_eq!(cells[6], "1700000000");
        // ISO column is the canonical UTC stamp.
        assert_eq!(cells[7], "2023-11-14T22:13:20Z");
    }

    #[test]
    fn histogram_csv_iso_column_matches_install_log_format() {
        // Both CSV exports must agree on the same ISO format so a
        // paralegal joining the two on timestamp gets identical
        // strings, not nearly-identical ones.
        let row = hist_row("com.x", 1, 0, 0, 0, 1_700_000_000);
        let csv = plugin_histogram_to_csv(&[row], false);
        // The ISO substring matches the install-log shape exactly.
        assert!(csv.contains("2023-11-14T22:13:20Z"));
    }

    #[test]
    fn histogram_csv_preserves_input_order() {
        // The CSV does NOT re-sort; the caller's row order is the
        // emitted order. The server emits sorted-by-total-DESC; the
        // UI may have re-sorted client-side via sortHistogramRows.
        // Either way, the exporter ships exactly what it gets.
        let rows = vec![
            hist_row("zzz", 10, 0, 0, 0, 100),
            hist_row("aaa", 5, 0, 0, 0, 200),
            hist_row("mmm", 1, 0, 0, 0, 300),
        ];
        let csv = plugin_histogram_to_csv(&rows, false);
        let ids: Vec<&str> = csv.lines().map(|l| l.split(',').next().unwrap()).collect();
        assert_eq!(ids, vec!["zzz", "aaa", "mmm"]);
    }

    #[test]
    fn histogram_csv_escapes_plugin_id_with_comma() {
        // Reverse-DNS ids don't carry commas today, but the column
        // is a free-form string at the schema level; escape defensively.
        let row = hist_row("com.acme,inc", 1, 0, 0, 0, 1_700_000_000);
        let csv = plugin_histogram_to_csv(&[row], false);
        assert!(csv.contains("\"com.acme,inc\""));
    }

    #[test]
    fn histogram_csv_escapes_plugin_id_with_quotes() {
        // Embedded quotes are doubled per RFC 4180.
        let row = hist_row("com.x\"quoted\"", 1, 0, 0, 0, 1_700_000_000);
        let csv = plugin_histogram_to_csv(&[row], false);
        assert!(csv.contains("\"com.x\"\"quoted\"\"\""));
    }

    #[test]
    fn histogram_csv_zero_timestamp_renders_zero_not_empty() {
        // Numeric column carries the integer 0, not an empty cell.
        // Distinguishes "real zero timestamp" from "missing data" —
        // matches the schema's NOT NULL contract for aggregate rows.
        let row = hist_row("com.x", 1, 0, 0, 0, 0);
        let csv = plugin_histogram_to_csv(&[row], false);
        let cells: Vec<&str> = csv.lines().next().unwrap().split(',').collect();
        assert_eq!(cells[6], "0", "unix column is integer 0 not empty");
        // ISO of 0 == 1970-01-01T00:00:00Z (epoch start, valid).
        assert_eq!(cells[7], "1970-01-01T00:00:00Z");
    }

    #[test]
    fn histogram_csv_emits_one_row_per_input() {
        // The CSV is a 1:1 emit; row count out matches row count in.
        // Mirrors the drilldown CSV's row-count invariant (slice 88).
        let rows: Vec<PluginHistogramRow> = (0..7)
            .map(|i| hist_row(&format!("com.p{i}"), 1, 0, 0, 0, 1_700_000_000))
            .collect();
        let csv = plugin_histogram_to_csv(&rows, false);
        assert_eq!(csv.lines().count(), 7);
    }

    #[test]
    fn histogram_csv_total_field_renders_separately_not_recomputed() {
        // The serialiser writes the row's `total` field verbatim
        // rather than re-summing the four bucket columns. Important:
        // a future axis (e.g. "skipped") gets added to PluginHistogramRow
        // BEFORE the CSV serialiser learns about it — the verbatim
        // write means the totals stay correct in the lag window.
        let mut row = hist_row("com.x", 1, 1, 1, 1, 1_700_000_000);
        // Deliberately mismatch the total field to verify the writer
        // doesn't re-derive it from the buckets.
        row.total = 99;
        let csv = plugin_histogram_to_csv(&[row], false);
        let cells: Vec<&str> = csv.lines().next().unwrap().split(',').collect();
        assert_eq!(cells[5], "99");
    }

    #[test]
    fn histogram_csv_no_none_or_null_strings_anywhere() {
        // Defensive: no "None" / "null" tokens leak from any column.
        // The histogram row has no Option<_> columns so this is a
        // contract check (catches a future Option addition that
        // forgets to escape).
        let rows = vec![hist_row("com.x", 0, 0, 0, 0, 0)];
        let csv = plugin_histogram_to_csv(&rows, true);
        assert!(!csv.contains("None"));
        assert!(!csv.to_lowercase().contains("null"));
    }

    // ─── Slice 104: activity timeline CSV export ─────────────────────

    fn act_bucket(
        bucket_start_unix: i64,
        installs: i64,
        updates: i64,
        uninstalls: i64,
        failures: i64,
    ) -> ActivityBucket {
        ActivityBucket {
            bucket_start_unix,
            installs,
            updates,
            uninstalls,
            failures,
            total: installs + updates + uninstalls + failures,
        }
    }

    #[test]
    fn activity_timeline_csv_header_inclusion_is_caller_controlled() {
        let buckets = vec![act_bucket(1_700_000_000, 1, 0, 0, 0)];
        let with_header = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, true);
        assert!(with_header.starts_with(ACTIVITY_TIMELINE_CSV_HEADER));
        let lines: Vec<_> = with_header.lines().collect();
        assert_eq!(lines.len(), 2, "header + one data row");
        let bare = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, false);
        assert!(!bare.starts_with("granularity,"));
        assert_eq!(bare.lines().count(), 1);
    }

    #[test]
    fn activity_timeline_csv_empty_with_header_is_header_only() {
        let csv = activity_timeline_to_csv(&[], TimeBucketGranularity::Day, true);
        assert_eq!(csv, format!("{}\n", ACTIVITY_TIMELINE_CSV_HEADER));
        let bare = activity_timeline_to_csv(&[], TimeBucketGranularity::Day, false);
        assert!(bare.is_empty());
    }

    #[test]
    fn activity_timeline_csv_header_column_count_matches_row() {
        // Defensive: header and every row carry the same number of
        // comma-separated fields. Protects future column additions
        // from drifting one side without the other.
        let buckets = vec![act_bucket(1_700_000_000, 1, 2, 3, 4)];
        let csv = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Week, true);
        let lines: Vec<_> = csv.lines().collect();
        let header_cols = lines[0].split(',').count();
        let row_cols = lines[1].split(',').count();
        assert_eq!(
            header_cols, row_cols,
            "header has {header_cols} cols, row has {row_cols}"
        );
        assert_eq!(header_cols, 8, "expected 8 columns");
    }

    #[test]
    fn activity_timeline_csv_columns_in_documented_order() {
        // Pin the column order so a refactor that reorders the header
        // constant has to update this test in lockstep with the doc
        // comment. 1_700_000_000 = 2023-11-14T22:13:20Z.
        let bucket = act_bucket(1_700_000_000, 3, 1, 0, 2);
        let csv = activity_timeline_to_csv(&[bucket], TimeBucketGranularity::Week, false);
        let line = csv.lines().next().unwrap();
        let cells: Vec<&str> = line.split(',').collect();
        // granularity, bucket_start_unix, bucket_start_iso, installs,
        // updates, uninstalls, failures, total
        assert_eq!(cells[0], "week");
        assert_eq!(cells[1], "1700000000");
        assert_eq!(cells[2], "2023-11-14T22:13:20Z");
        assert_eq!(cells[3], "3");
        assert_eq!(cells[4], "1");
        assert_eq!(cells[5], "0");
        assert_eq!(cells[6], "2");
        assert_eq!(cells[7], "6");
    }

    #[test]
    fn activity_timeline_csv_granularity_tag_on_every_row() {
        // The granularity column lives on every row (not in a comment
        // header) so a downstream pipeline can concatenate
        // day/week/month exports without losing the discriminator.
        let buckets = vec![
            act_bucket(1_700_000_000, 1, 0, 0, 0),
            act_bucket(1_700_086_400, 0, 1, 0, 0),
            act_bucket(1_700_172_800, 0, 0, 0, 1),
        ];
        let csv = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, false);
        for line in csv.lines() {
            let first_cell = line.split(',').next().unwrap();
            assert_eq!(first_cell, "day", "expected every row to start with 'day'");
        }
    }

    #[test]
    fn activity_timeline_csv_iso_matches_install_log_format_byte_for_byte() {
        // Downstream join compatibility — pipe an activity-timeline
        // CSV next to an install-log CSV and rely on the timestamp
        // strings rendering identically when they refer to the same
        // unix-seconds value.
        let unix = 1_700_000_000;
        let timeline_csv = activity_timeline_to_csv(
            &[act_bucket(unix, 1, 0, 0, 0)],
            TimeBucketGranularity::Day,
            false,
        );
        let timeline_iso = timeline_csv.split(',').nth(2).unwrap();
        // Synthesise an InstallEvent to render through install_log_to_csv.
        let install_csv = install_log_to_csv(
            &[InstallEvent {
                id: 1,
                plugin_id: "com.x".into(),
                version: "1".into(),
                action: InstallAction::Install,
                occurred_at: unix,
                source: None,
                bytes_written: None,
                files_extracted: None,
                replaced_existing: None,
                prior_version: None,
                error_msg: None,
            }],
            false,
        );
        // install-log iso is at column index 5 (id, plugin_id,
        // version, action, occurred_at_unix, occurred_at_iso).
        let install_iso = install_csv.split(',').nth(5).unwrap();
        assert_eq!(timeline_iso, install_iso);
    }

    #[test]
    fn activity_timeline_csv_preserves_input_order() {
        // The primitive emits ASC by bucket_start_unix (the server
        // contract); the exporter ships the caller's order verbatim
        // so a caller who pre-densified the timeline (zero-fill gap
        // buckets, dense ASC) gets that order written through.
        let buckets = vec![
            act_bucket(1_700_172_800, 0, 0, 0, 1), // day 3 (third)
            act_bucket(1_700_000_000, 1, 0, 0, 0), // day 1 (first)
            act_bucket(1_700_086_400, 0, 1, 0, 0), // day 2 (second)
        ];
        let csv = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, false);
        let starts: Vec<&str> = csv.lines().map(|l| l.split(',').nth(1).unwrap()).collect();
        // Out-of-order input ships verbatim.
        assert_eq!(starts, vec!["1700172800", "1700000000", "1700086400"]);
    }

    #[test]
    fn activity_timeline_csv_zero_timestamp_renders_zero_not_empty() {
        // Defensive: a pathological zero bucket_start renders as the
        // integer 0 (NOT empty), same as the histogram CSV's zero-
        // timestamp behaviour. NOT NULL contract at the column level.
        let buckets = vec![act_bucket(0, 1, 0, 0, 0)];
        let csv = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, false);
        let line = csv.lines().next().unwrap();
        let cells: Vec<&str> = line.split(',').collect();
        assert_eq!(cells[1], "0", "bucket_start_unix should render as '0'");
        // Don't assert specific ISO — chrono's epoch render is
        // implementation-defined. The contract is "non-empty if the
        // unix value renders" which the iso8601_utc helper already
        // covers — confirm via the smoke check below.
        assert!(
            !cells[2].is_empty(),
            "bucket_start_iso should be non-empty for unix 0"
        );
    }

    #[test]
    fn activity_timeline_csv_total_field_written_verbatim() {
        // The total field is written verbatim (NOT re-summed from
        // the four bucket columns) — same defence-in-depth as the
        // histogram CSV. A caller-supplied mismatched total surfaces
        // in the export rather than being silently corrected.
        let mut bucket = act_bucket(1_700_000_000, 1, 1, 1, 1);
        bucket.total = 999;
        let csv = activity_timeline_to_csv(&[bucket], TimeBucketGranularity::Day, false);
        let line = csv.lines().next().unwrap();
        let last_cell = line.split(',').last().unwrap();
        assert_eq!(last_cell, "999", "total written verbatim, not re-summed");
    }

    #[test]
    fn activity_timeline_csv_no_none_or_null_strings_anywhere() {
        // Defensive: no "None" / "null" tokens leak from any column.
        // ActivityBucket has no Option<_> columns today so this is a
        // contract check (catches a future Option addition that
        // forgets to escape).
        let buckets = vec![act_bucket(0, 0, 0, 0, 0)];
        let csv = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, true);
        assert!(!csv.contains("None"));
        assert!(!csv.to_lowercase().contains("null"));
    }

    #[test]
    fn activity_timeline_csv_granularity_tag_distinguishes_export_pairs() {
        // The granularity column distinguishes a Day-export from a
        // Week-export from a Month-export at the row level, so a
        // downstream concatenation of day.csv + week.csv + month.csv
        // remains self-describing. Three single-row exports differ
        // ONLY in the granularity column (modulo whatever cells the
        // input buckets happen to populate — held constant here).
        let buckets = vec![act_bucket(1_700_000_000, 1, 0, 0, 0)];
        for (g, expected) in [
            (TimeBucketGranularity::Day, "day"),
            (TimeBucketGranularity::Week, "week"),
            (TimeBucketGranularity::Month, "month"),
        ] {
            let csv = activity_timeline_to_csv(&buckets, g, false);
            let first_cell = csv.lines().next().unwrap().split(',').next().unwrap();
            assert_eq!(first_cell, expected);
        }
    }

    #[test]
    fn activity_timeline_csv_one_row_per_input_invariant() {
        // The exporter ships exactly one CSV data row per input bucket
        // — no truncation, no compression. Important when the caller
        // wants to render a stable count in the UI toast ("Exported
        // 30 buckets") without re-reading the file.
        for n in [0, 1, 5, 30] {
            let buckets: Vec<ActivityBucket> = (0..n)
                .map(|i| act_bucket(1_700_000_000 + i * 86_400, 1, 0, 0, 0))
                .collect();
            let csv = activity_timeline_to_csv(&buckets, TimeBucketGranularity::Day, false);
            assert_eq!(csv.lines().count(), n as usize, "n={n}");
        }
    }

    // ─── Slice 110: bucket drilldown CSV export ──────────────────────

    #[test]
    fn bucket_drilldown_csv_header_inclusion_is_caller_controlled() {
        let rows = vec![hist_row("com.x", 1, 0, 0, 0, 1_700_000_000)];
        let with_header =
            bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, true);
        assert!(with_header.starts_with(BUCKET_DRILLDOWN_CSV_HEADER));
        let lines: Vec<_> = with_header.lines().collect();
        assert_eq!(lines.len(), 2, "header + one data row");
        let bare = bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, false);
        assert!(!bare.starts_with("granularity,"));
        assert_eq!(bare.lines().count(), 1);
    }

    #[test]
    fn bucket_drilldown_csv_empty_with_header_is_header_only() {
        let csv = bucket_drilldown_to_csv(&[], 1_700_000_000, TimeBucketGranularity::Day, true);
        assert_eq!(csv, format!("{}\n", BUCKET_DRILLDOWN_CSV_HEADER));
        let bare = bucket_drilldown_to_csv(&[], 1_700_000_000, TimeBucketGranularity::Day, false);
        assert_eq!(bare, "");
    }

    #[test]
    fn bucket_drilldown_csv_header_column_count_matches_row() {
        // The header column count must match the data-row column count
        // exactly — any future column addition that lands on one but
        // not the other corrupts every downstream parser.
        let header_cols = BUCKET_DRILLDOWN_CSV_HEADER.split(',').count();
        assert_eq!(header_cols, 11);
        let rows = vec![hist_row("com.x", 1, 2, 3, 4, 1_700_000_000)];
        let csv = bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, false);
        let first = csv.lines().next().unwrap();
        assert_eq!(first.split(',').count(), header_cols);
    }

    #[test]
    fn bucket_drilldown_csv_columns_in_documented_order() {
        // Pin the column order so a future column shuffle that breaks
        // downstream parsers surfaces here. Bucket coords first, then
        // plugin_id, then per-action counts, then total, then last_at
        // (unix + ISO).
        let row = hist_row("com.acme.ocr", 3, 1, 0, 2, 1_700_000_000);
        let csv = bucket_drilldown_to_csv(&[row], 1_700_086_400, TimeBucketGranularity::Day, false);
        let line = csv.lines().next().unwrap();
        let cells: Vec<&str> = line.split(',').collect();
        assert_eq!(cells[0], "day"); // granularity
        assert_eq!(cells[1], "1700086400"); // bucket_start_unix
        assert_eq!(cells[2], "2023-11-15T22:13:20Z"); // bucket_start_iso
        assert_eq!(cells[3], "com.acme.ocr"); // plugin_id
        assert_eq!(cells[4], "3"); // installs
        assert_eq!(cells[5], "1"); // updates
        assert_eq!(cells[6], "0"); // uninstalls
        assert_eq!(cells[7], "2"); // failures
        assert_eq!(cells[8], "6"); // total
        assert_eq!(cells[9], "1700000000"); // last_occurred_at_unix
        assert_eq!(cells[10], "2023-11-14T22:13:20Z"); // last_occurred_at_iso
    }

    #[test]
    fn bucket_drilldown_csv_granularity_tag_on_every_row() {
        // Each row carries the same granularity tag — even if the
        // export is concatenated downstream with another export, the
        // discriminator stays on every line.
        let rows = vec![
            hist_row("p.a", 1, 0, 0, 0, 1_700_000_000),
            hist_row("p.b", 0, 1, 0, 0, 1_700_001_000),
            hist_row("p.c", 0, 0, 1, 0, 1_700_002_000),
        ];
        let csv = bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Week, false);
        for line in csv.lines() {
            let first = line.split(',').next().unwrap();
            assert_eq!(first, "week", "granularity tag missing on row: {line}");
        }
    }

    #[test]
    fn bucket_drilldown_csv_iso_matches_install_log_format_byte_for_byte() {
        // The bucket_start_iso column MUST share the exact ISO format
        // of the install-log + histogram + activity-timeline CSVs so
        // an auditor joining the four exports keys on identical
        // strings without timezone-format normalisation.
        let ts = 1_700_086_400;
        // Install-log CSV's ISO column for the same timestamp:
        let event = InstallEvent {
            id: 1,
            plugin_id: "p".into(),
            version: "1".into(),
            action: InstallAction::Install,
            occurred_at: ts,
            source: Some("m".into()),
            bytes_written: Some(0),
            files_extracted: Some(0),
            replaced_existing: Some(false),
            prior_version: None,
            error_msg: None,
        };
        let log_csv = install_log_to_csv(&[event], false);
        let log_iso = log_csv.lines().next().unwrap().split(',').nth(5).unwrap();
        // Bucket drilldown CSV's bucket_start_iso (column 2) for the
        // same timestamp:
        let row = hist_row("p", 1, 0, 0, 0, ts);
        let bd_csv = bucket_drilldown_to_csv(&[row], ts, TimeBucketGranularity::Day, false);
        let bd_iso = bd_csv.lines().next().unwrap().split(',').nth(2).unwrap();
        assert_eq!(
            bd_iso, log_iso,
            "bucket_start_iso must match install-log ISO"
        );
    }

    #[test]
    fn bucket_drilldown_csv_preserves_input_order() {
        // The exporter ships rows verbatim — the bucket_drilldown
        // reader handles sort, the exporter mustn't second-guess it.
        let rows = vec![
            hist_row("p.z", 5, 0, 0, 0, 1_700_000_000),
            hist_row("p.a", 1, 0, 0, 0, 1_700_000_000),
            hist_row("p.m", 3, 0, 0, 0, 1_700_000_000),
        ];
        let csv = bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, false);
        let ids: Vec<&str> = csv.lines().map(|l| l.split(',').nth(3).unwrap()).collect();
        assert_eq!(ids, vec!["p.z", "p.a", "p.m"]);
    }

    #[test]
    fn bucket_drilldown_csv_zero_timestamp_renders_zero_not_empty() {
        // A bucket pinned at unix=0 renders "0" not empty — same
        // NOT-NULL contract as the histogram CSV's last_occurred_at_unix.
        let row = hist_row("p", 1, 0, 0, 0, 0);
        let csv = bucket_drilldown_to_csv(&[row], 0, TimeBucketGranularity::Day, false);
        let cells: Vec<&str> = csv.lines().next().unwrap().split(',').collect();
        assert_eq!(cells[1], "0"); // bucket_start_unix
        assert_eq!(cells[9], "0"); // last_occurred_at_unix
    }

    #[test]
    fn bucket_drilldown_csv_total_field_written_verbatim() {
        // Defence-in-depth: total is the caller's value, NOT a re-sum
        // of installs/updates/uninstalls/failures. A test row with a
        // deliberately-wrong total catches a future regression where
        // the serialiser silently recomputes (which would mask a
        // PluginHistogramRow axis addition bug).
        let row = PluginHistogramRow {
            plugin_id: "p".into(),
            installs: 1,
            updates: 2,
            uninstalls: 3,
            failures: 4,
            total: 99, // deliberately wrong; 1+2+3+4=10
            last_occurred_at: 1_700_000_000,
        };
        let csv = bucket_drilldown_to_csv(&[row], 1_700_000_000, TimeBucketGranularity::Day, false);
        let cells: Vec<&str> = csv.lines().next().unwrap().split(',').collect();
        assert_eq!(cells[8], "99");
    }

    #[test]
    fn bucket_drilldown_csv_no_none_or_null_strings_anywhere() {
        // Catches a future Option<_> addition to PluginHistogramRow
        // that slips through as the literal "None" or "null" string.
        let rows = vec![hist_row("p", 1, 0, 0, 0, 1_700_000_000)];
        let csv = bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, false);
        assert!(!csv.contains("None"));
        assert!(!csv.contains("null"));
    }

    #[test]
    fn bucket_drilldown_csv_one_row_per_input_invariant() {
        // The exporter ships exactly one CSV data row per input row
        // — no truncation, no compression. Pins the same stable-count
        // invariant the histogram + activity-timeline CSVs hold.
        for n in [0i64, 1, 5, 30] {
            let rows: Vec<PluginHistogramRow> = (0..n)
                .map(|i| hist_row(&format!("p.{i:02}"), 1, 0, 0, 0, 1_700_000_000))
                .collect();
            let csv =
                bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, false);
            assert_eq!(csv.lines().count(), n as usize, "n={n}");
        }
    }

    #[test]
    fn bucket_drilldown_csv_granularity_tag_distinguishes_export_pairs() {
        // Two drilldown exports for the SAME bucket-start but different
        // granularities must differ ONLY in the granularity column —
        // a downstream pipeline can join the two by stripping column
        // 0 + comparing the rest. (In practice the rows would differ
        // because a week bucket covers more events than a day, but
        // the structural invariant holds on identical input.)
        let rows = vec![hist_row("p", 1, 0, 0, 0, 1_700_000_000)];
        let day = bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Day, false);
        let week =
            bucket_drilldown_to_csv(&rows, 1_700_000_000, TimeBucketGranularity::Week, false);
        let day_rest: String = day
            .lines()
            .next()
            .unwrap()
            .split_once(',')
            .unwrap()
            .1
            .into();
        let week_rest: String = week
            .lines()
            .next()
            .unwrap()
            .split_once(',')
            .unwrap()
            .1
            .into();
        assert_eq!(day_rest, week_rest, "non-granularity columns must match");
        assert!(day.starts_with("day,"));
        assert!(week.starts_with("week,"));
    }

    #[test]
    fn bucket_drilldown_csv_escapes_plugin_id_with_comma() {
        // The plugin_id is the only column that can plausibly contain
        // an RFC-4180 trip character. Pin the escape behaviour so a
        // future relaxation of the reverse-DNS id format doesn't
        // silently corrupt downstream parsers.
        let row = hist_row("weird,id", 1, 0, 0, 0, 1_700_000_000);
        let csv = bucket_drilldown_to_csv(&[row], 1_700_000_000, TimeBucketGranularity::Day, false);
        assert!(csv.contains("\"weird,id\""));
    }

    // ─── Slice 99: histogram JSON envelope ───────────────────────────

    #[test]
    fn histogram_json_envelope_carries_schema_v1() {
        let rows = vec![hist_row("com.x", 1, 0, 0, 0, 1_700_000_000)];
        let env = plugin_histogram_to_json_with_now(&rows, None, None, 1, 1_710_000_000);
        assert_eq!(env.schema_version, PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.schema_version, 1);
    }

    #[test]
    fn histogram_json_envelope_records_row_count_matching_rows_len() {
        let rows: Vec<PluginHistogramRow> = (0..4)
            .map(|i| hist_row(&format!("com.p{i}"), 1, 0, 0, 0, 1_700_000_000))
            .collect();
        let env = plugin_histogram_to_json_with_now(&rows, None, None, 4, 1_710_000_000);
        assert_eq!(env.row_count, 4);
        assert_eq!(env.row_count, env.rows.len());
        let empty = plugin_histogram_to_json_with_now(&[], None, None, 0, 1_710_000_000);
        assert_eq!(empty.row_count, 0);
        assert_eq!(empty.rows.len(), 0);
    }

    #[test]
    fn histogram_json_envelope_carries_grand_total_from_caller() {
        // The envelope ships the caller-supplied grand_total verbatim
        // rather than re-summing (the server already pre-summed; a
        // re-sum here would let row-truncation differ silently from
        // the actual corpus total).
        let rows = vec![
            hist_row("com.a", 3, 0, 0, 0, 1_700_000_000),
            hist_row("com.b", 5, 0, 0, 0, 1_700_000_000),
        ];
        // Deliberately mismatch grand_total vs the sum of rows.total
        // to verify the envelope ships the caller's value verbatim.
        let env = plugin_histogram_to_json_with_now(&rows, None, None, 999, 1_710_000_000);
        assert_eq!(env.grand_total, 999);
    }

    #[test]
    fn histogram_json_envelope_generated_at_iso_format() {
        let env = plugin_histogram_to_json_with_now(&[], None, None, 0, 1_710_000_000);
        // Same ISO format as install-log envelope so a downstream
        // tool reading either file sees identical timestamp shape.
        assert_eq!(env.generated_at_iso, "2024-03-09T16:00:00Z");
    }

    #[test]
    fn histogram_json_envelope_no_window_bounds_means_no_iso_either() {
        let env = plugin_histogram_to_json_with_now(&[], None, None, 0, 1_710_000_000);
        assert!(env.since_unix.is_none());
        assert!(env.since_iso.is_none());
        assert!(env.until_unix.is_none());
        assert!(env.until_iso.is_none());
    }

    #[test]
    fn histogram_json_envelope_window_bounds_round_trip_to_iso() {
        let env = plugin_histogram_to_json_with_now(
            &[],
            Some(1_700_000_000),
            Some(1_710_000_000),
            0,
            1_710_000_000,
        );
        assert_eq!(env.since_unix, Some(1_700_000_000));
        assert_eq!(env.since_iso.as_deref(), Some("2023-11-14T22:13:20Z"));
        assert_eq!(env.until_unix, Some(1_710_000_000));
        assert_eq!(env.until_iso.as_deref(), Some("2024-03-09T16:00:00Z"));
    }

    #[test]
    fn histogram_json_envelope_only_since_bound_other_none() {
        let env =
            plugin_histogram_to_json_with_now(&[], Some(1_700_000_000), None, 0, 1_710_000_000);
        assert_eq!(env.since_unix, Some(1_700_000_000));
        assert!(env.since_iso.is_some());
        assert!(env.until_unix.is_none());
        assert!(env.until_iso.is_none());
    }

    #[test]
    fn histogram_json_envelope_preserves_input_row_order() {
        // The envelope ships the caller's order verbatim — the server
        // emits sorted-by-total-DESC, the UI may re-sort by another
        // axis, the exporter doesn't re-sort either way.
        let rows = vec![
            hist_row("zzz", 10, 0, 0, 0, 100),
            hist_row("aaa", 5, 0, 0, 0, 200),
            hist_row("mmm", 1, 0, 0, 0, 300),
        ];
        let env = plugin_histogram_to_json_with_now(&rows, None, None, 16, 1_710_000_000);
        let ids: Vec<&str> = env.rows.iter().map(|r| r.plugin_id.as_str()).collect();
        assert_eq!(ids, vec!["zzz", "aaa", "mmm"]);
    }

    #[test]
    fn histogram_json_envelope_rows_are_clones_not_references() {
        // The envelope owns its row data (Vec<PluginHistogramRow>).
        // Validates the to_vec() in the builder by mutating the caller's
        // slice after envelope construction — the envelope shouldn't
        // observe the mutation.
        let mut rows = vec![hist_row("com.x", 1, 0, 0, 0, 1_700_000_000)];
        let env = plugin_histogram_to_json_with_now(&rows, None, None, 1, 1_710_000_000);
        rows[0].plugin_id = "mutated".into();
        assert_eq!(env.rows[0].plugin_id, "com.x");
    }

    #[test]
    fn histogram_json_envelope_serde_round_trip() {
        // serde stability — the envelope serialises and deserialises
        // to the same value, with the same field set we documented.
        let rows = vec![
            hist_row("com.acme.ocr", 3, 1, 0, 2, 1_700_000_000),
            hist_row("org.studio.batch", 5, 0, 1, 0, 1_700_086_400),
        ];
        let env =
            plugin_histogram_to_json_with_now(&rows, Some(1_700_000_000), None, 12, 1_710_000_000);
        let s = serde_json::to_string(&env).unwrap();
        // All required fields present.
        assert!(s.contains("\"schema_version\":1"));
        assert!(s.contains("\"row_count\":2"));
        assert!(s.contains("\"grand_total\":12"));
        assert!(s.contains("\"generated_at_iso\":\"2024-03-09T16:00:00Z\""));
        assert!(s.contains("\"since_unix\":1700000000"));
        assert!(s.contains("\"since_iso\":\"2023-11-14T22:13:20Z\""));
        assert!(s.contains("\"until_unix\":null"));
        assert!(s.contains("\"until_iso\":null"));
        // Roundtrip back.
        let back: PluginHistogramExportEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn histogram_json_envelope_pretty_print_is_valid_json() {
        // The Tauri command writes serde_json::to_string_pretty —
        // confirm the envelope serialises cleanly in that form too
        // (catches any non-serialisable field added in the future).
        let rows = vec![hist_row("com.x", 1, 0, 0, 0, 1_700_000_000)];
        let env = plugin_histogram_to_json_with_now(&rows, None, None, 1, 1_710_000_000);
        let pretty = serde_json::to_string_pretty(&env).unwrap();
        // Pretty form contains newlines + indentation.
        assert!(pretty.contains('\n'));
        // Round-trip from pretty form.
        let back: PluginHistogramExportEnvelope = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn histogram_json_envelope_empty_input_renders_cleanly() {
        // Zero rows, no window. The envelope still has all its fields;
        // a downstream consumer can recognise "Slab audit export" by
        // schema_version even when there's nothing to read.
        let env = plugin_histogram_to_json_with_now(&[], None, None, 0, 1_710_000_000);
        assert_eq!(env.schema_version, PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.row_count, 0);
        assert_eq!(env.grand_total, 0);
        assert!(env.rows.is_empty());
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"rows\":[]"));
    }

    #[test]
    fn histogram_json_envelope_parallel_versioned_with_install_log() {
        // Both start at v1; their values are equal today. The two
        // envelopes are parallel-versioned (independent bumps as their
        // bodies diverge), so this equality is "true today" not "true
        // forever" — bumping one must NOT silently bump the other.
        assert_eq!(
            PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION, INSTALL_LOG_EXPORT_SCHEMA_VERSION,
            "both start at v1 — bump independently when shapes diverge",
        );
    }

    // ─── Slice 105: activity timeline JSON envelope ──────────────────

    #[test]
    fn activity_timeline_json_envelope_carries_schema_v1() {
        let buckets = vec![act_bucket(1_700_000_000, 1, 0, 0, 0)];
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Day,
            None,
            None,
            1,
            1_710_000_000,
        );
        assert_eq!(env.schema_version, ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.schema_version, 1);
    }

    #[test]
    fn activity_timeline_json_envelope_records_granularity() {
        // The granularity field discriminates the bucket-width
        // semantics — a downstream consumer can read it without
        // inferring from bucket gaps. Test all three values
        // round-trip exactly.
        for g in [
            TimeBucketGranularity::Day,
            TimeBucketGranularity::Week,
            TimeBucketGranularity::Month,
        ] {
            let env = activity_timeline_to_json_with_now(&[], g, None, None, 0, 1_710_000_000);
            assert_eq!(env.granularity, g);
        }
    }

    #[test]
    fn activity_timeline_json_envelope_bucket_count_mirrors_buckets_len() {
        let buckets: Vec<ActivityBucket> = (0..4)
            .map(|i| act_bucket(1_700_000_000 + i * 86_400, 1, 0, 0, 0))
            .collect();
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Day,
            None,
            None,
            4,
            1_710_000_000,
        );
        assert_eq!(env.bucket_count, 4);
        assert_eq!(env.bucket_count, env.buckets.len());
        let empty =
            activity_timeline_to_json_with_now(&[], TimeBucketGranularity::Day, None, None, 0, 0);
        assert_eq!(empty.bucket_count, 0);
        assert_eq!(empty.buckets.len(), 0);
    }

    #[test]
    fn activity_timeline_json_envelope_grand_total_verbatim_from_caller() {
        // grand_total ships from the caller VERBATIM (not re-summed
        // here) — matches the histogram envelope's defensive posture.
        // A caller-supplied mismatched value surfaces in the export
        // rather than being silently corrected.
        let buckets = vec![
            act_bucket(1_700_000_000, 3, 0, 0, 0),
            act_bucket(1_700_086_400, 5, 0, 0, 0),
        ];
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Day,
            None,
            None,
            999, // deliberate mismatch
            1_710_000_000,
        );
        assert_eq!(env.grand_total, 999);
    }

    #[test]
    fn activity_timeline_json_envelope_generated_at_iso_format_matches_install_log() {
        // Downstream join: the ISO format byte-for-byte matches the
        // install-log envelope's generated_at_iso so two exports
        // produced at the same moment carry identical strings.
        let now = 1_710_000_000;
        let timeline_env =
            activity_timeline_to_json_with_now(&[], TimeBucketGranularity::Day, None, None, 0, now);
        let install_env = install_log_to_json_with_now(&[], None, None, now);
        assert_eq!(timeline_env.generated_at_iso, install_env.generated_at_iso);
    }

    #[test]
    fn activity_timeline_json_envelope_window_bounds_round_trip_to_iso() {
        // Both window bounds populated → both iso sides populated;
        // both unset → both iso sides null.
        let env = activity_timeline_to_json_with_now(
            &[],
            TimeBucketGranularity::Day,
            Some(1_700_000_000),
            Some(1_710_000_000),
            0,
            1_710_000_000,
        );
        assert_eq!(env.since_iso.as_deref(), Some("2023-11-14T22:13:20Z"));
        assert_eq!(env.until_iso.as_deref(), Some("2024-03-09T16:00:00Z"));
        let no_window =
            activity_timeline_to_json_with_now(&[], TimeBucketGranularity::Day, None, None, 0, 0);
        assert!(no_window.since_iso.is_none());
        assert!(no_window.until_iso.is_none());
    }

    #[test]
    fn activity_timeline_json_envelope_only_since_has_only_since_iso() {
        let env = activity_timeline_to_json_with_now(
            &[],
            TimeBucketGranularity::Day,
            Some(1_700_000_000),
            None,
            0,
            1_710_000_000,
        );
        assert!(env.since_iso.is_some());
        assert!(env.until_iso.is_none());
    }

    #[test]
    fn activity_timeline_json_envelope_preserves_input_bucket_order() {
        // The envelope ships the caller's bucket order verbatim. The
        // server emits ASC by bucket_start_unix; the UI may densify
        // (zero-fill gap buckets) before export. Either way the
        // envelope ships exactly what it gets.
        let buckets = vec![
            act_bucket(1_700_172_800, 0, 0, 0, 1),
            act_bucket(1_700_000_000, 1, 0, 0, 0),
            act_bucket(1_700_086_400, 0, 1, 0, 0),
        ];
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Day,
            None,
            None,
            2,
            1_710_000_000,
        );
        let starts: Vec<i64> = env.buckets.iter().map(|b| b.bucket_start_unix).collect();
        // Out-of-order input ships out-of-order — caller's
        // responsibility to sort before export if order matters.
        assert_eq!(starts, vec![1_700_172_800, 1_700_000_000, 1_700_086_400]);
    }

    #[test]
    fn activity_timeline_json_envelope_buckets_are_clones_not_references() {
        // The envelope owns its bucket data (Vec<ActivityBucket>).
        // Validates the to_vec() in the builder by mutating the
        // caller's slice after construction.
        let mut buckets = vec![act_bucket(1_700_000_000, 1, 0, 0, 0)];
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Day,
            None,
            None,
            1,
            1_710_000_000,
        );
        buckets[0].installs = 999;
        assert_eq!(env.buckets[0].installs, 1);
    }

    #[test]
    fn activity_timeline_json_envelope_serde_round_trip() {
        // serde stability — the envelope serialises and deserialises
        // to the same value with the same field set we documented.
        let buckets = vec![
            act_bucket(1_700_000_000, 3, 1, 0, 2),
            act_bucket(1_700_086_400, 5, 0, 1, 0),
        ];
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Week,
            Some(1_700_000_000),
            None,
            12,
            1_710_000_000,
        );
        let s = serde_json::to_string(&env).unwrap();
        // All required fields present.
        assert!(s.contains("\"schema_version\":1"));
        assert!(s.contains("\"granularity\":\"week\""));
        assert!(s.contains("\"bucket_count\":2"));
        assert!(s.contains("\"grand_total\":12"));
        assert!(s.contains("\"generated_at_iso\":\"2024-03-09T16:00:00Z\""));
        assert!(s.contains("\"since_unix\":1700000000"));
        assert!(s.contains("\"since_iso\":\"2023-11-14T22:13:20Z\""));
        assert!(s.contains("\"until_unix\":null"));
        assert!(s.contains("\"until_iso\":null"));
        // Roundtrip back.
        let back: ActivityTimelineExportEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn activity_timeline_json_envelope_pretty_print_is_valid_json() {
        // The Tauri command writes serde_json::to_string_pretty —
        // confirm the envelope serialises cleanly in that form too.
        let buckets = vec![act_bucket(1_700_000_000, 1, 0, 0, 0)];
        let env = activity_timeline_to_json_with_now(
            &buckets,
            TimeBucketGranularity::Day,
            None,
            None,
            1,
            1_710_000_000,
        );
        let pretty = serde_json::to_string_pretty(&env).unwrap();
        assert!(pretty.contains('\n'));
        let back: ActivityTimelineExportEnvelope = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn activity_timeline_json_envelope_empty_input_renders_cleanly() {
        // Zero buckets, no window — the envelope still has all its
        // fields. A downstream consumer can recognise "Slab audit
        // export" by schema_version + granularity even with nothing
        // to read.
        let env = activity_timeline_to_json_with_now(
            &[],
            TimeBucketGranularity::Month,
            None,
            None,
            0,
            1_710_000_000,
        );
        assert_eq!(env.schema_version, ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.granularity, TimeBucketGranularity::Month);
        assert_eq!(env.bucket_count, 0);
        assert_eq!(env.grand_total, 0);
        assert!(env.buckets.is_empty());
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"buckets\":[]"));
    }

    #[test]
    fn activity_timeline_json_envelope_parallel_versioned_with_other_envelopes() {
        // All four start at v1 today. The envelopes are parallel-
        // versioned (independent bumps as their bodies diverge), so
        // these equalities are "true today" not "true forever" —
        // bumping any one must NOT silently bump the others.
        assert_eq!(
            ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION, INSTALL_LOG_EXPORT_SCHEMA_VERSION,
            "timeline + install-log: bump independently when shapes diverge",
        );
        assert_eq!(
            ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION, PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION,
            "timeline + histogram: bump independently when shapes diverge",
        );
    }

    #[test]
    fn activity_timeline_json_envelope_granularity_round_trips_serde() {
        // The granularity field uses the same serde rename_all =
        // "lowercase" — confirm it survives a roundtrip in all three
        // values without manual remapping.
        for (g, tag) in [
            (TimeBucketGranularity::Day, "day"),
            (TimeBucketGranularity::Week, "week"),
            (TimeBucketGranularity::Month, "month"),
        ] {
            let env = activity_timeline_to_json_with_now(&[], g, None, None, 0, 0);
            let s = serde_json::to_string(&env).unwrap();
            assert!(s.contains(&format!("\"granularity\":\"{tag}\"")));
            let back: ActivityTimelineExportEnvelope = serde_json::from_str(&s).unwrap();
            assert_eq!(back.granularity, g);
        }
    }

    // ─── Slice 111: bucket drilldown JSON envelope ───────────────────

    #[test]
    fn bucket_drilldown_json_envelope_carries_schema_v1() {
        let rows = vec![hist_row("com.x", 1, 0, 0, 0, 1_700_000_000)];
        let env = bucket_drilldown_to_json_with_now(
            &rows,
            1_700_000_000,
            TimeBucketGranularity::Day,
            1,
            1_710_000_000,
        );
        assert_eq!(env.schema_version, BUCKET_DRILLDOWN_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.schema_version, 1);
    }

    #[test]
    fn bucket_drilldown_json_envelope_records_bucket_coords() {
        // The envelope's three bucket-coordinate fields must round-trip
        // verbatim from the caller — they're the primary key tying the
        // export back to the activity-timeline aggregate that produced it.
        let env = bucket_drilldown_to_json_with_now(
            &[],
            1_700_086_400,
            TimeBucketGranularity::Week,
            0,
            1_710_000_000,
        );
        assert_eq!(env.granularity, TimeBucketGranularity::Week);
        assert_eq!(env.bucket_start_unix, 1_700_086_400);
        assert_eq!(env.bucket_start_iso, "2023-11-15T22:13:20Z");
    }

    #[test]
    fn bucket_drilldown_json_envelope_row_count_mirrors_rows_len() {
        // row_count is a parallel-key to rows.len() — a downstream
        // consumer reads one int instead of parsing the array.
        for n in [0, 1, 5, 30] {
            let rows: Vec<PluginHistogramRow> = (0..n)
                .map(|i| hist_row(&format!("p.{i:02}"), 1, 0, 0, 0, 1_700_000_000))
                .collect();
            let env = bucket_drilldown_to_json_with_now(
                &rows,
                1_700_000_000,
                TimeBucketGranularity::Day,
                n as i64,
                1_710_000_000,
            );
            assert_eq!(env.row_count, n);
            assert_eq!(env.row_count, env.rows.len());
        }
    }

    #[test]
    fn bucket_drilldown_json_envelope_grand_total_verbatim_from_caller() {
        // grand_total is NOT re-summed from the row totals — a
        // deliberately-wrong caller value rides through verbatim so a
        // future PluginHistogramRow axis addition can't silently
        // diverge from a stable on-disk envelope. Same defence-in-
        // depth posture as the histogram + timeline envelopes.
        let rows = vec![
            hist_row("p.a", 1, 0, 0, 0, 1_700_000_000),
            hist_row("p.b", 2, 0, 0, 0, 1_700_000_000),
        ];
        // Sum of row.total = 1 + 2 = 3, but caller supplies 999.
        let env = bucket_drilldown_to_json_with_now(
            &rows,
            1_700_000_000,
            TimeBucketGranularity::Day,
            999,
            1_710_000_000,
        );
        assert_eq!(env.grand_total, 999);
    }

    #[test]
    fn bucket_drilldown_json_envelope_generated_at_iso_matches_install_log() {
        // The generated_at_iso stamp must share the EXACT format the
        // install-log envelope produces — a downstream consumer
        // joining the four envelope kinds (install-log + histogram +
        // timeline + drilldown) by export time keys on identical
        // strings without timezone-format normalisation.
        let env_drill = bucket_drilldown_to_json_with_now(
            &[],
            1_700_000_000,
            TimeBucketGranularity::Day,
            0,
            1_710_000_000,
        );
        let env_log = install_log_to_json_with_now(&[], None, None, 1_710_000_000);
        assert_eq!(env_drill.generated_at_iso, env_log.generated_at_iso);
    }

    #[test]
    fn bucket_drilldown_json_envelope_preserves_input_row_order() {
        // Rows ship verbatim — bucket_drilldown emits DESC-by-total
        // with id-tiebreak; the envelope mustn't re-sort.
        let rows = vec![
            hist_row("p.z", 5, 0, 0, 0, 1_700_000_000),
            hist_row("p.a", 1, 0, 0, 0, 1_700_000_000),
            hist_row("p.m", 3, 0, 0, 0, 1_700_000_000),
        ];
        let env = bucket_drilldown_to_json_with_now(
            &rows,
            1_700_000_000,
            TimeBucketGranularity::Day,
            9,
            1_710_000_000,
        );
        let ids: Vec<&str> = env.rows.iter().map(|r| r.plugin_id.as_str()).collect();
        assert_eq!(ids, vec!["p.z", "p.a", "p.m"]);
    }

    #[test]
    fn bucket_drilldown_json_envelope_rows_are_owned_clones() {
        // The envelope owns its rows — a caller mutating the source
        // slice after envelope construction mustn't change the
        // envelope. Pin this by building, then mutating, then
        // re-asserting. (Vec<PluginHistogramRow> is owned by the
        // envelope; this pins the contract for future readers.)
        let mut source = vec![hist_row("p", 1, 0, 0, 0, 1_700_000_000)];
        let env = bucket_drilldown_to_json_with_now(
            &source,
            1_700_000_000,
            TimeBucketGranularity::Day,
            1,
            1_710_000_000,
        );
        source.clear();
        assert_eq!(env.rows.len(), 1);
        assert_eq!(env.rows[0].plugin_id, "p");
    }

    #[test]
    fn bucket_drilldown_json_envelope_serde_round_trip() {
        // Full envelope serde round-trip — confirms every field
        // survives serialisation and deserialisation byte-for-byte.
        let rows = vec![hist_row("com.x", 3, 1, 0, 2, 1_700_000_000)];
        let env = bucket_drilldown_to_json_with_now(
            &rows,
            1_700_086_400,
            TimeBucketGranularity::Week,
            6,
            1_710_000_000,
        );
        let s = serde_json::to_string(&env).unwrap();
        let back: BucketDrilldownExportEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn bucket_drilldown_json_envelope_pretty_print_is_valid_json() {
        // Tauri layer uses to_string_pretty — confirm the pretty
        // form is still valid JSON that round-trips back to the
        // same envelope.
        let rows = vec![hist_row("p", 1, 0, 0, 0, 1_700_000_000)];
        let env = bucket_drilldown_to_json_with_now(
            &rows,
            1_700_000_000,
            TimeBucketGranularity::Day,
            1,
            1_710_000_000,
        );
        let pretty = serde_json::to_string_pretty(&env).unwrap();
        assert!(pretty.contains('\n'), "pretty form is multi-line");
        let back: BucketDrilldownExportEnvelope = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn bucket_drilldown_json_envelope_empty_input_renders_cleanly() {
        let env = bucket_drilldown_to_json_with_now(
            &[],
            1_700_000_000,
            TimeBucketGranularity::Day,
            0,
            1_710_000_000,
        );
        assert_eq!(env.row_count, 0);
        assert_eq!(env.grand_total, 0);
        assert!(env.rows.is_empty());
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"rows\":[]"));
    }

    #[test]
    fn bucket_drilldown_json_envelope_parallel_versioned_with_other_envelopes() {
        // All four envelope schema versions start at v1 today but are
        // PARALLEL-versioned — a future shape change in one bumps
        // that one only. Pin the v1==v1 equality so a careless joint
        // bump surfaces here.
        assert_eq!(
            BUCKET_DRILLDOWN_EXPORT_SCHEMA_VERSION, INSTALL_LOG_EXPORT_SCHEMA_VERSION,
            "drilldown + install-log: bump independently when shapes diverge",
        );
        assert_eq!(
            BUCKET_DRILLDOWN_EXPORT_SCHEMA_VERSION, PLUGIN_HISTOGRAM_EXPORT_SCHEMA_VERSION,
            "drilldown + histogram: bump independently when shapes diverge",
        );
        assert_eq!(
            BUCKET_DRILLDOWN_EXPORT_SCHEMA_VERSION, ACTIVITY_TIMELINE_EXPORT_SCHEMA_VERSION,
            "drilldown + timeline: bump independently when shapes diverge",
        );
    }

    #[test]
    fn bucket_drilldown_json_envelope_granularity_serde_round_trips_for_all_three() {
        // Pin the lowercase-tag serde contract for every granularity
        // value so a future granularity addition (e.g. "year") that
        // forgets to bump the enum surfaces here.
        for (g, tag) in [
            (TimeBucketGranularity::Day, "day"),
            (TimeBucketGranularity::Week, "week"),
            (TimeBucketGranularity::Month, "month"),
        ] {
            let env = bucket_drilldown_to_json_with_now(&[], 1_700_000_000, g, 0, 0);
            let s = serde_json::to_string(&env).unwrap();
            assert!(s.contains(&format!("\"granularity\":\"{tag}\"")));
            let back: BucketDrilldownExportEnvelope = serde_json::from_str(&s).unwrap();
            assert_eq!(back.granularity, g);
        }
    }

    #[test]
    fn bucket_drilldown_json_envelope_bucket_start_iso_matches_csv_byte_for_byte() {
        // The bucket_start_iso field must produce the SAME ISO string
        // as the CSV exporter's column 2 for the same bucket_start —
        // pinned so a paralegal joining the CSV and JSON exports on
        // the bucket coord finds identical strings.
        let ts = 1_700_086_400;
        let row = hist_row("p", 1, 0, 0, 0, ts);
        let env = bucket_drilldown_to_json_with_now(
            &[row.clone()],
            ts,
            TimeBucketGranularity::Day,
            1,
            1_710_000_000,
        );
        let csv = bucket_drilldown_to_csv(&[row], ts, TimeBucketGranularity::Day, false);
        let csv_iso = csv.lines().next().unwrap().split(',').nth(2).unwrap();
        assert_eq!(env.bucket_start_iso, csv_iso);
    }
}
