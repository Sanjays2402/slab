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

use rusqlite::Connection;

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
/// methods land in Slice 8.2; this module ships the scaffolding +
/// schema + open helpers.
pub struct PluginStorage {
    /// The underlying sqlite connection. Marked `pub(crate)` so the
    /// Slice 8.3 unit tests can poke at it via raw SQL when asserting
    /// schema-level invariants (e.g. that `PRAGMA user_version`
    /// matches [`SCHEMA_VERSION`]).
    ///
    /// `dead_code` allow: in 8.1 nothing *reads* this field yet; the
    /// CRUD methods landing in 8.2 will. The field is the whole
    /// point of the struct, so we accept the temporary lint.
    #[allow(dead_code)]
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
}
