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
const SCHEMA_VERSION: u32 = 2;

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

    // ─── Auto-prune driver (Slice 64) ────────────────────────────────

    /// Run the retention policy if the debounce window has elapsed.
    ///
    /// - If `last_auto_prune_at` is missing or older than
    ///   `now_unix - AUTO_PRUNE_INTERVAL_SECS`, prune rows older
    ///   than `retain_days()` and stamp `last_auto_prune_at = now_unix`.
    /// - Otherwise, no-op (returns
    ///   [`AutoPruneOutcome::Skipped { next_due_unix }`]).
    ///
    /// Designed to be called once on app startup. The debounce keeps
    /// the auto-prune to roughly daily even when the user launches
    /// the app many times per day (CI-style restarts, dev iteration).
    /// `now_unix` is an explicit parameter so tests can pin it
    /// deterministically; the production wrapper uses
    /// [`auto_prune_if_due_now`].
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
        let pruned = self.prune_older_than(cutoff)?;
        self.set_last_auto_prune_at(now_unix)?;
        Ok(AutoPruneOutcome::Pruned {
            rows_removed: pruned,
            retain_days,
            cutoff_unix: cutoff,
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
    /// The prune ran. `rows_removed` is the delete count; the other
    /// fields describe what window was applied so the UI can show
    /// "Auto-pruned 23 events older than 2025-06-21 (365d)".
    Pruned {
        rows_removed: usize,
        retain_days: i64,
        cutoff_unix: i64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_pragma_pinned() {
        let log = InstallLog::open_in_memory().unwrap();
        assert_eq!(log.schema_version().unwrap(), SCHEMA_VERSION);
        // v2: install_log_settings table added (round-14 retention
        // policy storage). Bump in lockstep with init_schema arms.
        assert_eq!(SCHEMA_VERSION, 2);
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
}
