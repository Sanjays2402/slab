//! Workshop (v2.0.0 Slice 8) — per-plugin key/value storage.
//!
//! Plugin authors want a place to stash cache, preferences, or tiny
//! amounts of state across sessions. The rquickjs sandbox has no
//! `localStorage`, no `IndexedDB`, no `window`. Without a host
//! surface, plugins either burn `slab.fetch` quota every load
//! (re-downloading static data) or punt persistence to the user
//! ("re-enter your API key on every restart"). Both are bad.
//!
//! ## Shape
//!
//! Single process-wide sqlite database at
//! `~/.slab/plugin-storage.sqlite` with one flat `kv` table keyed by
//! `(plugin_id, key)`. Per-plugin scoping is enforced **in code**:
//! every public method takes a `plugin_id: &str` and pins it into
//! every WHERE clause, so plugin A literally cannot observe plugin
//! B's keys even via crafted SQL. There's no inter-plugin namespace
//! to traverse.
//!
//! ## Why a single DB, not one file per plugin?
//!
//! - One open [`Connection`] per process is cheaper than maintaining
//!   a pool keyed by `plugin_id` (we'd have N inactive handles in
//!   memory for N installed plugins).
//! - Sqlite's row-level locking is plenty for the read-mostly
//!   access pattern plugins exhibit (preferences, cache lookups).
//! - Backup / inspection / `rm` is trivially scoped to a single
//!   file under `~/.slab/`.
//! - Migrations live in one place when the schema evolves.
//!
//! ## Why no manifest capability gate?
//!
//! Slices 1–7 gate every `slab.*` surface against a manifest cap +
//! user grant. Storage breaks that pattern intentionally:
//!
//! 1. **Scoping is the only realistic threat model.** Capabilities
//!    exist to prevent plugin A from exfiltrating *user* data
//!    (`fs`, `net`) or *other-plugin* data (`beacon`). Per-plugin
//!    sqlite scoping already prevents the latter; the former isn't
//!    on the table because the plugin can only ever read keys it
//!    wrote itself.
//! 2. **Quotas are the actual concern.** A buggy plugin filling the
//!    disk matters more than a "denied" plugin author working around
//!    the gate by stashing state on a remote KV. We enforce hard
//!    caps in [`PluginStorage::kv_set`] — [`MAX_PLUGIN_BYTES`] /
//!    [`MAX_VALUE_BYTES`] / [`MAX_KEY_BYTES`].
//! 3. **UX cost.** A `storage = "allow"` consent prompt for every
//!    plugin that wants to cache one number forever is friction
//!    theatre. Users would rubber-stamp it 100% of the time → the
//!    prompt becomes noise that drowns out the gates that matter.
//!
//! Full reasoning in
//! `docs/plans/2026-05-19-v2.0.0-workshop-slice-8.md`.
//!
//! ## What this module is NOT
//!
//! - It is **not** a blob store. Single-value cap is 1 MiB; total
//!   per-plugin cap is 8 MiB. Plugins needing more should host
//!   their own backend.
//! - It does **not** offer transactions or batched writes (deferred
//!   to a future polish slice if anyone asks).
//! - It does **not** encrypt at rest. Storage is no more sensitive
//!   than the PDFs Slab already opens unencrypted.
//! - It does **not** expose multi-plugin keys for any reason. Even
//!   the host can't (and shouldn't) build cross-plugin queries; if
//!   that's ever needed it goes through a Tauri command that takes
//!   a `plugin_id` explicitly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::{params, Connection, OptionalExtension};

/// Schema version stamped into `PRAGMA user_version`. Bump + add a
/// migration arm in [`PluginStorage::init_schema`] when changing the
/// table shape. v1: initial `kv(plugin_id, key, value, value_size,
/// updated_at)`.
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Quota constants — public so the JS binding (Slice 8.5) and Cabinet UI
// (future) can surface them in error messages without re-declaring the
// numbers.
// ---------------------------------------------------------------------------

/// Per-plugin total-storage cap. 8 MiB is generous for cache /
/// preferences workloads while bounding worst-case disk usage at
/// `8 MiB × N installed plugins` — typically well under 100 MiB even
/// for power users.
pub const MAX_PLUGIN_BYTES: u64 = 8 * 1024 * 1024;

/// Per-value byte cap. Larger than this almost always means the
/// plugin is using storage as a blob store, which we deliberately
/// don't support. 1 MiB matches the rquickjs heap budget for a
/// single string allocation.
pub const MAX_VALUE_BYTES: usize = 1024 * 1024;

/// Key byte cap. Sqlite has no inherent limit on TEXT length, but
/// the `(plugin_id, key)` index lookup wants short keys for cache
/// efficiency, and a pathological 1 MiB key would blow the index
/// page count.
pub const MAX_KEY_BYTES: usize = 64 * 1024;

// Compile-time sanity checks on the constants. If a future "let's
// make values bigger" change breaks one of these invariants, the
// crate fails to build — better than runtime asserts in a test that
// might never be exercised on a given platform.
const _: () = assert!(
    (MAX_VALUE_BYTES as u64) <= MAX_PLUGIN_BYTES,
    "single-value cap must fit inside per-plugin cap"
);
const _: () = assert!(
    MAX_KEY_BYTES < MAX_VALUE_BYTES,
    "keys should be small relative to values"
);
const _: () = assert!(
    MAX_PLUGIN_BYTES.is_multiple_of(MAX_VALUE_BYTES as u64),
    "plugin cap should be a clean multiple of value cap (quota arithmetic invariant)"
);

// ---------------------------------------------------------------------------
// Errors.
// ---------------------------------------------------------------------------

/// Storage-layer error type. Wraps [`rusqlite::Error`] for the common
/// "underlying sqlite said no" case and carries explicit variants for
/// validation / quota failures the JS binding needs to differentiate
/// when building the rejected Promise message.
///
/// Each variant's `Display` impl produces a single-line, user-facing
/// summary; the JS binding embeds that verbatim into
/// `new Error(msg)` so plugin authors get a useful message on
/// `(await slab.storage.set(...)).catch(e => e.message)`.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Anything sqlite reported (open failed, integrity violation,
    /// busy lock, etc.). We never wrap further — let the original
    /// message propagate.
    #[error("sqlite: {0}")]
    Db(#[from] rusqlite::Error),

    /// Couldn't create the parent directory or open the file. Only
    /// happens during [`PluginStorage::open`]; in-memory opens never
    /// touch the filesystem.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Caller passed a key longer than [`MAX_KEY_BYTES`]. We measure
    /// bytes (not chars) because that's what sqlite's index sees.
    #[error("key too long ({0} bytes, max {1})")]
    KeyTooLong(usize, usize),

    /// Caller passed a value longer than [`MAX_VALUE_BYTES`].
    #[error("value too large ({0} bytes, max {1})")]
    ValueTooLarge(usize, usize),

    /// Writing the new value would push this plugin's total usage
    /// past [`MAX_PLUGIN_BYTES`]. We compute `current` as the
    /// post-eviction baseline (i.e. NOT counting the prior value at
    /// `key` when this is an update), so the numbers add up to the
    /// projected new total.
    #[error("plugin storage quota exceeded ({current} bytes + {incoming} > {limit})")]
    QuotaExceeded {
        current: u64,
        incoming: u64,
        limit: u64,
    },
}

// ---------------------------------------------------------------------------
// Paths.
// ---------------------------------------------------------------------------

/// Default DB path: `~/.slab/plugin-storage.sqlite`. Falls back to
/// `./plugin-storage.sqlite` if `$HOME` is somehow unset — matches the
/// pattern used by `ai::study_store::default_db_path()` for
/// consistency.
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("plugin-storage.sqlite")
}

// ---------------------------------------------------------------------------
// PluginStorage — owning handle around the per-process sqlite connection.
// ---------------------------------------------------------------------------

/// Owning handle around the per-process sqlite [`Connection`]. CRUD
/// methods (`kv_get` / `kv_set` / `kv_remove` / `kv_list` / `kv_clear`
/// / `kv_usage_bytes`) live below the struct; the JS binding in Slice
/// 8.5 calls them through a [`SharedPluginStorage`] handle.
pub struct PluginStorage {
    /// The underlying sqlite connection. Marked `pub(crate)` so the
    /// Slice 8.1+8.3 unit tests can poke at it via raw SQL when
    /// asserting schema-level invariants (e.g. that `PRAGMA
    /// user_version` matches [`SCHEMA_VERSION`]).
    pub(crate) conn: Connection,
}

impl PluginStorage {
    /// Open (or create) the DB at `path`, ensuring the parent
    /// directory exists, and initialise the schema.
    ///
    /// # Errors
    /// - [`StorageError::Io`] if the parent dir can't be created
    ///   (e.g. permission denied on `~/.slab`).
    /// - [`StorageError::Db`] if sqlite refuses to open or schema
    ///   migration fails.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database. Used by the Slice 8.3 unit tests
    /// (`#[cfg(test)] pub fn shared_in_memory(...)`) so each test
    /// gets full isolation without touching disk.
    ///
    /// Public (not `#[cfg(test)]`) so integration tests outside this
    /// module's tree (e.g. Slice 8.6 E2E tests living in
    /// `runtime/mod.rs`) can build a deterministic per-test store
    /// too.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Run schema migrations. Idempotent — safe to call on every
    /// open. New migration arms are added when [`SCHEMA_VERSION`]
    /// bumps.
    fn init_schema(conn: &Connection) -> Result<(), StorageError> {
        // v1 schema. `value` is BLOB so we can store arbitrary UTF-8
        // strings without sqlite trying to interpret them as TEXT
        // and stripping NULs etc. The Slice 8.2 helpers always pass
        // `String::as_bytes()` and decode via `String::from_utf8_lossy`
        // — explicit ownership of the byte boundary.
        //
        // `value_size` is denormalised (it's also `length(value)`) so
        // the quota-check query can `SUM(value_size)` without
        // touching the BLOB pages. Cheaper by a factor of "size of
        // your largest stored value".
        //
        // `updated_at` is unix seconds. We don't index it; reads
        // are by (plugin_id, key) only. Future LRU-eviction work
        // would add an index then.
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS kv (
              plugin_id   TEXT NOT NULL,
              key         TEXT NOT NULL,
              value       BLOB NOT NULL,
              value_size  INTEGER NOT NULL,
              updated_at  INTEGER NOT NULL,
              PRIMARY KEY (plugin_id, key)
            );
            CREATE INDEX IF NOT EXISTS idx_kv_plugin ON kv(plugin_id);
            "#,
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // CRUD — every method takes `plugin_id: &str` and pins it into every
    // WHERE clause. Per-plugin scoping is enforced in code here; there is
    // no other layer (no manifest gate, no row-level security, no namespace
    // mangling). If you refactor these queries, KEEP the `plugin_id` filter
    // on every read AND every write.
    // -----------------------------------------------------------------------

    /// Fetch the value for `key` in `plugin_id`'s namespace, or
    /// `None` if not set.
    ///
    /// Values were written by [`Self::kv_set`] as UTF-8 bytes; this
    /// reads them back via `String::from_utf8_lossy` so a malformed
    /// row (which shouldn't be possible through the public API)
    /// degrades to the Unicode replacement character rather than
    /// erroring out the whole lookup.
    pub fn kv_get(&self, plugin_id: &str, key: &str) -> Result<Option<String>, StorageError> {
        let row: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT value FROM kv WHERE plugin_id = ?1 AND key = ?2",
                params![plugin_id, key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(row.map(|b| String::from_utf8_lossy(&b).into_owned()))
    }

    /// Insert or replace the value for `key` in `plugin_id`'s
    /// namespace. Enforces all three quotas BEFORE writing:
    ///
    /// 1. Key length ≤ [`MAX_KEY_BYTES`]
    /// 2. Value length ≤ [`MAX_VALUE_BYTES`]
    /// 3. Projected total ≤ [`MAX_PLUGIN_BYTES`], where projected =
    ///    `current_total - prev_size + new_size`. The subtraction is
    ///    why overwriting an existing key doesn't double-count.
    ///
    /// # Errors
    /// - [`StorageError::KeyTooLong`] / [`StorageError::ValueTooLarge`]
    ///   — validation failures (no DB write occurred).
    /// - [`StorageError::QuotaExceeded`] — quota check failed (no DB
    ///   write occurred). The error fields carry the post-eviction
    ///   baseline and the incoming write size so the JS binding can
    ///   surface specific numbers to plugin authors.
    /// - [`StorageError::Db`] — sqlite rejected the write.
    pub fn kv_set(&self, plugin_id: &str, key: &str, value: &str) -> Result<(), StorageError> {
        if key.len() > MAX_KEY_BYTES {
            return Err(StorageError::KeyTooLong(key.len(), MAX_KEY_BYTES));
        }
        let value_bytes = value.as_bytes();
        if value_bytes.len() > MAX_VALUE_BYTES {
            return Err(StorageError::ValueTooLarge(
                value_bytes.len(),
                MAX_VALUE_BYTES,
            ));
        }

        // Read current total for this plugin. COALESCE handles the
        // empty-plugin case (SUM of zero rows is NULL).
        let current_total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(value_size), 0) FROM kv WHERE plugin_id = ?1",
            params![plugin_id],
            |r| r.get(0),
        )?;

        // Read prior size at THIS key, if any. Subtracted from the
        // total so an overwrite doesn't double-count toward the quota.
        let prev_size: i64 = self
            .conn
            .query_row(
                "SELECT value_size FROM kv WHERE plugin_id = ?1 AND key = ?2",
                params![plugin_id, key],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);

        // Saturating arithmetic protects against a corrupt DB where
        // prev_size somehow exceeds current_total — that would
        // underflow `u64` and let the write succeed against intent.
        let baseline = (current_total as u64).saturating_sub(prev_size as u64);
        let incoming = value_bytes.len() as u64;
        let projected = baseline.saturating_add(incoming);
        if projected > MAX_PLUGIN_BYTES {
            return Err(StorageError::QuotaExceeded {
                current: baseline,
                incoming,
                limit: MAX_PLUGIN_BYTES,
            });
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO kv(plugin_id, key, value, value_size, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(plugin_id, key) DO UPDATE SET \
               value = excluded.value, \
               value_size = excluded.value_size, \
               updated_at = excluded.updated_at",
            params![plugin_id, key, value_bytes, value_bytes.len() as i64, now],
        )?;
        Ok(())
    }

    /// Delete the row at `(plugin_id, key)`. Returns `true` iff a
    /// row was actually removed.
    pub fn kv_remove(&self, plugin_id: &str, key: &str) -> Result<bool, StorageError> {
        let n = self.conn.execute(
            "DELETE FROM kv WHERE plugin_id = ?1 AND key = ?2",
            params![plugin_id, key],
        )?;
        Ok(n > 0)
    }

    /// Every key stored for `plugin_id`, lexicographic order. The JS
    /// binding (Slice 8.5) returns this as a `Promise<string[]>`.
    pub fn kv_list(&self, plugin_id: &str) -> Result<Vec<String>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT key FROM kv WHERE plugin_id = ?1 ORDER BY key")?;
        let rows = stmt.query_map(params![plugin_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Drop every key for `plugin_id`. Returns the number of rows
    /// removed (informational — the JS binding surfaces this as the
    /// `Promise<number>` resolution value).
    pub fn kv_clear(&self, plugin_id: &str) -> Result<usize, StorageError> {
        let n = self
            .conn
            .execute("DELETE FROM kv WHERE plugin_id = ?1", params![plugin_id])?;
        Ok(n)
    }

    /// Total bytes stored for `plugin_id`. Used by quota arithmetic
    /// internally and exposed publicly for a future
    /// `slab.storage.usage()` introspection surface; tests assert
    /// quota arithmetic via this method to avoid taking a direct
    /// dependency on the schema column names.
    pub fn kv_usage_bytes(&self, plugin_id: &str) -> Result<u64, StorageError> {
        let total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(value_size), 0) FROM kv WHERE plugin_id = ?1",
            params![plugin_id],
            |r| r.get(0),
        )?;
        Ok(total.max(0) as u64)
    }
}

// ---------------------------------------------------------------------------
// Shared singleton — process-wide handle, lazily opened on first use.
// ---------------------------------------------------------------------------

/// Sharable handle around a [`PluginStorage`]. Wrapped in an `Arc<Mutex<_>>`
/// because [`Connection`] is `!Sync` — only one thread may hold the
/// lock at a time. Slice 8.5's JS bindings clone this handle into each
/// closure; lock contention is negligible because every operation is
/// microsecond-scale.
pub type SharedPluginStorage = Arc<Mutex<PluginStorage>>;

/// Process-wide singleton. Lazily opened on first call to
/// [`shared_storage`]; subsequent calls return cheap [`Arc::clone`]s.
static STORAGE: OnceLock<SharedPluginStorage> = OnceLock::new();

/// Get (lazily opening) the process-wide shared store.
///
/// The first successful call opens the on-disk DB at
/// [`default_db_path`] and stores the handle in the static cell.
/// Concurrent first-callers race; whichever loses simply opens a
/// throwaway DB then drops it when they observe the winner's value
/// via [`OnceLock::get`].
///
/// # Errors
/// Propagates whatever [`PluginStorage::open`] returned on first
/// call. Subsequent successful calls never error.
pub fn shared_storage() -> Result<SharedPluginStorage, StorageError> {
    if let Some(s) = STORAGE.get() {
        return Ok(Arc::clone(s));
    }
    let store = PluginStorage::open(&default_db_path())?;
    let arc = Arc::new(Mutex::new(store));
    // Race-safe: if another thread already populated the cell we
    // return THEIR Arc, not ours, so callers always see a single
    // shared connection. Our local `arc` falls out of scope and the
    // underlying DB handle is dropped.
    let _ = STORAGE.set(Arc::clone(&arc));
    Ok(Arc::clone(STORAGE.get().unwrap_or(&arc)))
}

/// Build a test-local shared handle over an in-memory DB. Bypasses
/// the global singleton so each test gets full isolation.
#[cfg(test)]
pub fn shared_in_memory() -> Result<SharedPluginStorage, StorageError> {
    Ok(Arc::new(Mutex::new(PluginStorage::open_in_memory()?)))
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_initialises_schema() {
        let store = PluginStorage::open_in_memory().expect("open in-memory");
        // The kv table should exist and be queryable even with zero rows.
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM kv", [], |r| r.get(0))
            .expect("kv table exists and is queryable");
        assert_eq!(count, 0);
    }

    #[test]
    fn user_version_pragma_set_to_schema_version() {
        let store = PluginStorage::open_in_memory().expect("open in-memory");
        let v: u32 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("read user_version");
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn schema_creates_plugin_id_index() {
        // The (plugin_id) index is what makes scoping queries fast.
        // Verify it's created so we never accidentally regress.
        let store = PluginStorage::open_in_memory().expect("open in-memory");
        let name: String = store
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_kv_plugin'",
                [],
                |r| r.get(0),
            )
            .expect("idx_kv_plugin exists");
        assert_eq!(name, "idx_kv_plugin");
    }

    #[test]
    fn primary_key_is_compound_plugin_id_key() {
        // Insert two rows with the same `key` but different
        // `plugin_id`. If the primary key isn't compound, the second
        // insert raises a UNIQUE constraint violation.
        let store = PluginStorage::open_in_memory().expect("open in-memory");
        store
            .conn
            .execute(
                "INSERT INTO kv(plugin_id, key, value, value_size, updated_at) \
                 VALUES ('plug-a', 'shared-key', X'01', 1, 0)",
                [],
            )
            .expect("first insert");
        store
            .conn
            .execute(
                "INSERT INTO kv(plugin_id, key, value, value_size, updated_at) \
                 VALUES ('plug-b', 'shared-key', X'02', 1, 0)",
                [],
            )
            .expect("second insert (different plugin, same key) must succeed");

        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM kv", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn init_schema_is_idempotent() {
        // Calling open() twice on the same on-disk path (simulated
        // here by running init_schema twice on the same Connection)
        // must not error or duplicate the index.
        let store = PluginStorage::open_in_memory().expect("open in-memory");
        PluginStorage::init_schema(&store.conn).expect("idempotent re-init");

        // Still exactly one index named idx_kv_plugin.
        let n: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_kv_plugin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn shared_in_memory_returns_distinct_handles_per_call() {
        // Each `shared_in_memory()` call opens a NEW in-memory DB
        // (no global state). Writing to one must not affect another.
        let a = shared_in_memory().expect("a");
        let b = shared_in_memory().expect("b");
        a.lock()
            .unwrap()
            .conn
            .execute(
                "INSERT INTO kv(plugin_id, key, value, value_size, updated_at) \
                 VALUES ('p', 'k', X'01', 1, 0)",
                [],
            )
            .expect("write to a");
        let n_a: i64 = a
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM kv", [], |r| r.get(0))
            .unwrap();
        let n_b: i64 = b
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM kv", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_a, 1, "a got the write");
        assert_eq!(n_b, 0, "b is isolated");
    }

    #[test]
    fn default_db_path_lives_under_dot_slab() {
        // Ensures the path matches the documented `~/.slab/...`
        // location so backup scripts, .gitignore patterns, and
        // Cabinet's "purge plugins" UX all agree on where the file
        // is. Path component check is OS-independent.
        let p = default_db_path();
        let s = p.to_string_lossy();
        assert!(
            s.ends_with("plugin-storage.sqlite"),
            "expected filename to be plugin-storage.sqlite, got {s}"
        );
        let has_dot_slab = p.iter().any(|c| c == std::ffi::OsStr::new(".slab"));
        assert!(has_dot_slab, "expected `.slab` component in path: {s}");
    }

    #[test]
    fn storage_error_display_includes_size_info() {
        // Slice 8.5's JS binding embeds `e.to_string()` into the
        // rejected Promise's Error message. Verify the variants the
        // binding cares about produce useful, single-line text.
        let e = StorageError::KeyTooLong(100, 64);
        assert!(format!("{e}").contains("100"));
        assert!(format!("{e}").contains("64"));

        let e = StorageError::ValueTooLarge(2_000_000, 1_048_576);
        assert!(format!("{e}").contains("2000000"));

        let e = StorageError::QuotaExceeded {
            current: 5,
            incoming: 10,
            limit: 12,
        };
        let s = format!("{e}");
        assert!(s.contains('5'));
        assert!(s.contains("10"));
        assert!(s.contains("12"));
    }

    // ---- Slice 8.3 CRUD contract tests ----

    #[test]
    fn get_returns_none_for_missing_key() {
        let s = PluginStorage::open_in_memory().unwrap();
        assert_eq!(s.kv_get("plug", "missing").unwrap(), None);
    }

    #[test]
    fn set_then_get_round_trips_value() {
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plug", "k", "hello").unwrap();
        assert_eq!(s.kv_get("plug", "k").unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn set_round_trips_unicode_and_null_bytes() {
        // BLOB-not-TEXT column means embedded NULs and arbitrary
        // UTF-8 round-trip without sqlite trying to interpret them.
        let s = PluginStorage::open_in_memory().unwrap();
        let payload = "héllo\0wörld 🍰";
        s.kv_set("plug", "weird", payload).unwrap();
        assert_eq!(s.kv_get("plug", "weird").unwrap().as_deref(), Some(payload));
    }

    #[test]
    fn set_overwrites_existing_value() {
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plug", "k", "first").unwrap();
        s.kv_set("plug", "k", "second").unwrap();
        assert_eq!(s.kv_get("plug", "k").unwrap().as_deref(), Some("second"));
        // And exactly one row, not two.
        assert_eq!(s.kv_list("plug").unwrap(), vec!["k"]);
    }

    #[test]
    fn remove_deletes_key_and_returns_true() {
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plug", "k", "v").unwrap();
        assert!(s.kv_remove("plug", "k").unwrap());
        assert_eq!(s.kv_get("plug", "k").unwrap(), None);
    }

    #[test]
    fn remove_returns_false_for_missing_key() {
        let s = PluginStorage::open_in_memory().unwrap();
        assert!(!s.kv_remove("plug", "ghost").unwrap());
    }

    #[test]
    fn list_returns_sorted_keys_for_plugin_only() {
        let s = PluginStorage::open_in_memory().unwrap();
        // Insert out-of-order to verify ORDER BY actually sorts.
        s.kv_set("plug-a", "zebra", "z").unwrap();
        s.kv_set("plug-a", "apple", "a").unwrap();
        s.kv_set("plug-a", "mango", "m").unwrap();
        // Decoy in a different namespace — must not leak into list.
        s.kv_set("plug-b", "leaked", "x").unwrap();
        assert_eq!(
            s.kv_list("plug-a").unwrap(),
            vec!["apple", "mango", "zebra"]
        );
    }

    #[test]
    fn list_is_empty_for_unknown_plugin() {
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plug-a", "k", "v").unwrap();
        let empty: Vec<String> = vec![];
        assert_eq!(s.kv_list("plug-unknown").unwrap(), empty);
    }

    #[test]
    fn clear_drops_all_keys_for_plugin() {
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plug", "a", "1").unwrap();
        s.kv_set("plug", "b", "2").unwrap();
        s.kv_set("plug", "c", "3").unwrap();
        let n = s.kv_clear("plug").unwrap();
        assert_eq!(n, 3);
        assert!(s.kv_list("plug").unwrap().is_empty());
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), 0);
    }

    #[test]
    fn clear_other_plugins_unaffected() {
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plug-a", "k", "alpha").unwrap();
        s.kv_set("plug-b", "k", "beta").unwrap();
        s.kv_clear("plug-a").unwrap();
        // Decoy plugin's data still intact.
        assert_eq!(s.kv_get("plug-b", "k").unwrap().as_deref(), Some("beta"));
    }

    #[test]
    fn set_rejects_oversized_key() {
        let s = PluginStorage::open_in_memory().unwrap();
        let huge_key = "k".repeat(MAX_KEY_BYTES + 1);
        let err = s.kv_set("plug", &huge_key, "v").unwrap_err();
        assert!(
            matches!(err, StorageError::KeyTooLong(n, lim) if n == MAX_KEY_BYTES + 1 && lim == MAX_KEY_BYTES),
            "got {err:?}"
        );
        // And no write occurred.
        assert!(s.kv_list("plug").unwrap().is_empty());
    }

    #[test]
    fn set_rejects_oversized_value() {
        let s = PluginStorage::open_in_memory().unwrap();
        let huge_value = "x".repeat(MAX_VALUE_BYTES + 1);
        let err = s.kv_set("plug", "k", &huge_value).unwrap_err();
        assert!(
            matches!(err, StorageError::ValueTooLarge(n, lim) if n == MAX_VALUE_BYTES + 1 && lim == MAX_VALUE_BYTES),
            "got {err:?}"
        );
        assert!(s.kv_list("plug").unwrap().is_empty());
    }

    #[test]
    fn set_rejects_when_quota_exceeded() {
        // Fill the plugin to exactly the cap with N × 1 MiB values,
        // then assert ONE MORE byte fails. Uses kv_usage_bytes to
        // verify the post-cap state is still at the cap (no partial
        // write).
        let s = PluginStorage::open_in_memory().unwrap();
        let big = "x".repeat(MAX_VALUE_BYTES);
        let slots = MAX_PLUGIN_BYTES as usize / MAX_VALUE_BYTES;
        for i in 0..slots {
            s.kv_set("plug", &format!("k{i}"), &big)
                .unwrap_or_else(|e| panic!("slot {i} should fit: {e:?}"));
        }
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), MAX_PLUGIN_BYTES);
        // One more byte is over the cap.
        let err = s.kv_set("plug", "overflow", "y").unwrap_err();
        match err {
            StorageError::QuotaExceeded {
                current,
                incoming,
                limit,
            } => {
                assert_eq!(current, MAX_PLUGIN_BYTES);
                assert_eq!(incoming, 1);
                assert_eq!(limit, MAX_PLUGIN_BYTES);
            }
            other => panic!("expected QuotaExceeded, got {other:?}"),
        }
        // Failed write left the cap intact.
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), MAX_PLUGIN_BYTES);
    }

    #[test]
    fn set_overwrite_does_not_double_count_quota() {
        // The crux of the quota arithmetic: an overwrite at the same
        // key should subtract the prior size before adding the new
        // size, NOT double-count. If we'd written `prev + new`, this
        // test fails because 8 MiB + 1 MiB > 8 MiB.
        let s = PluginStorage::open_in_memory().unwrap();
        let big = "x".repeat(MAX_VALUE_BYTES);
        let slots = MAX_PLUGIN_BYTES as usize / MAX_VALUE_BYTES;
        for i in 0..slots {
            s.kv_set("plug", &format!("k{i}"), &big).unwrap();
        }
        // Now overwrite k0 with another 1 MiB value. Net change is
        // 0; quota check should pass.
        s.kv_set("plug", "k0", &big)
            .expect("overwrite of same-size value must fit");
        // Sanity: usage didn't grow.
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), MAX_PLUGIN_BYTES);
    }

    #[test]
    fn scoping_isolates_plugins() {
        // The security-critical test. Two plugins write the same key;
        // each must observe only its own value. Then clear plugin A
        // and confirm plugin B is untouched.
        let s = PluginStorage::open_in_memory().unwrap();
        s.kv_set("plugin-a", "secret", "alpha").unwrap();
        s.kv_set("plugin-b", "secret", "beta").unwrap();
        assert_eq!(
            s.kv_get("plugin-a", "secret").unwrap().as_deref(),
            Some("alpha")
        );
        assert_eq!(
            s.kv_get("plugin-b", "secret").unwrap().as_deref(),
            Some("beta")
        );
        s.kv_clear("plugin-a").unwrap();
        assert_eq!(s.kv_get("plugin-a", "secret").unwrap(), None);
        assert_eq!(
            s.kv_get("plugin-b", "secret").unwrap().as_deref(),
            Some("beta")
        );
        // List on plugin-a is empty; plugin-b still sees its key.
        assert!(s.kv_list("plugin-a").unwrap().is_empty());
        assert_eq!(s.kv_list("plugin-b").unwrap(), vec!["secret"]);
    }

    #[test]
    fn usage_tracks_inserts_overwrites_and_removes() {
        // Belt-and-braces for the quota-arithmetic invariant: every
        // write path that affects usage must keep `kv_usage_bytes`
        // honest. If a future refactor breaks this, the JS binding's
        // `slab.storage.usage()` would lie to plugin authors.
        let s = PluginStorage::open_in_memory().unwrap();
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), 0);
        s.kv_set("plug", "a", "hello").unwrap(); //  5 bytes
        s.kv_set("plug", "b", "world!").unwrap(); // 6 bytes
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), 11);
        s.kv_set("plug", "a", "hi").unwrap(); // shrink 5 → 2
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), 8);
        s.kv_remove("plug", "b").unwrap();
        assert_eq!(s.kv_usage_bytes("plug").unwrap(), 2);
    }
}
