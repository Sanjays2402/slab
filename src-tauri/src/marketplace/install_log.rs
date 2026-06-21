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
}
