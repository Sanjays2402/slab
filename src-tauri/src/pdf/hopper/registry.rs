//! Hopper sqlite registry — persistent store of watched-folder configs.
//!
//! All Hopper state lives in a single sqlite DB at `~/.slab/hopper.db`
//! (or wherever the caller decides — `open()` takes a path). We use
//! `rusqlite` with the `bundled` feature so the binary is self-contained
//! across macOS / Linux / Windows.
//!
//! ## Schema
//!
//! ```sql
//! CREATE TABLE watches (
//!   id              INTEGER PRIMARY KEY,
//!   source_dir      TEXT NOT NULL,
//!   output_dir      TEXT NOT NULL,
//!   recipe_id       TEXT,
//!   rename_pattern  TEXT,
//!   ai_rename       INTEGER NOT NULL,  -- 0/1
//!   enabled         INTEGER NOT NULL,  -- 0/1
//!   created_at      TEXT NOT NULL      -- RFC3339
//! );
//! ```
//!
//! Concurrency: the registry is wrapped in `Mutex` at the service layer
//! (`HopperService`). Individual methods take `&mut self` so callers can
//! short-lock during writes.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A persisted watched-folder configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Watch {
    pub id: i64,
    pub source_dir: String,
    pub output_dir: String,
    pub recipe_id: Option<String>,
    pub rename_pattern: Option<String>,
    pub ai_rename: bool,
    pub enabled: bool,
    pub created_at: String,
}

/// Input payload for [`HopperRegistry::add`] — the registry assigns id
/// and timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchInput {
    pub source_dir: String,
    pub output_dir: String,
    pub recipe_id: Option<String>,
    pub rename_pattern: Option<String>,
    pub ai_rename: bool,
}

/// Sqlite-backed CRUD store of watches.
pub struct HopperRegistry {
    conn: Connection,
}

impl HopperRegistry {
    /// Open (or create) the registry DB at `path`. Creates parent dirs
    /// if missing. Applies the schema migration idempotently.
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
            CREATE TABLE IF NOT EXISTS watches (
                id              INTEGER PRIMARY KEY,
                source_dir      TEXT NOT NULL,
                output_dir      TEXT NOT NULL,
                recipe_id       TEXT,
                rename_pattern  TEXT,
                ai_rename       INTEGER NOT NULL,
                enabled         INTEGER NOT NULL,
                created_at      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS watches_enabled_idx ON watches(enabled);
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Insert a new watch. Returns its assigned id. New watches start
    /// `enabled = true`.
    pub fn add(&mut self, input: WatchInput) -> rusqlite::Result<i64> {
        let now = rfc3339_now();
        self.conn.execute(
            "INSERT INTO watches \
                (source_dir, output_dir, recipe_id, rename_pattern, ai_rename, enabled, created_at) \
                VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
            params![
                input.source_dir,
                input.output_dir,
                input.recipe_id,
                input.rename_pattern,
                input.ai_rename as i32,
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List every watch ordered oldest-first.
    pub fn list(&self) -> rusqlite::Result<Vec<Watch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_dir, output_dir, recipe_id, rename_pattern, \
                    ai_rename, enabled, created_at \
             FROM watches ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], row_to_watch)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Fetch a single watch by id.
    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Watch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_dir, output_dir, recipe_id, rename_pattern, \
                    ai_rename, enabled, created_at \
             FROM watches WHERE id = ?1",
        )?;
        stmt.query_row(params![id], row_to_watch).optional()
    }

    /// Delete a watch. Idempotent — removing a missing id is not an error.
    pub fn remove(&mut self, id: i64) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM watches WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Toggle the `enabled` flag.
    pub fn set_enabled(&mut self, id: i64, enabled: bool) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE watches SET enabled = ?1 WHERE id = ?2",
            params![enabled as i32, id],
        )?;
        Ok(())
    }
}

/// Default DB location: `~/.slab/hopper.db`. Falls back to a temp path
/// when no home dir can be resolved (test envs, sandboxed CI).
pub fn default_db_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        home.join(".slab").join("hopper.db")
    } else {
        std::env::temp_dir().join("slab-hopper.db")
    }
}

fn row_to_watch(row: &rusqlite::Row<'_>) -> rusqlite::Result<Watch> {
    Ok(Watch {
        id: row.get(0)?,
        source_dir: row.get(1)?,
        output_dir: row.get(2)?,
        recipe_id: row.get(3)?,
        rename_pattern: row.get(4)?,
        ai_rename: row.get::<_, i32>(5)? != 0,
        enabled: row.get::<_, i32>(6)? != 0,
        created_at: row.get(7)?,
    })
}

fn rfc3339_now() -> String {
    // Minimal RFC3339-ish stamp without pulling chrono — seconds since
    // epoch as YYYY-MM-DDTHH:MM:SSZ is overkill for our use; we just
    // store the unix seconds as a string. Display can format later.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hopper.db");
        (dir, path)
    }

    #[test]
    fn round_trip_add_and_list() {
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let id = reg
            .add(WatchInput {
                source_dir: "/tmp/in".into(),
                output_dir: "/tmp/out".into(),
                recipe_id: Some("flatten-and-bates".into()),
                rename_pattern: Some("{date}_{ai_title}".into()),
                ai_rename: true,
            })
            .unwrap();

        let all = reg.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id);
        assert_eq!(all[0].source_dir, "/tmp/in");
        assert_eq!(all[0].output_dir, "/tmp/out");
        assert_eq!(all[0].recipe_id.as_deref(), Some("flatten-and-bates"));
        assert!(all[0].ai_rename);
        assert!(all[0].enabled);
    }

    #[test]
    fn open_is_idempotent() {
        let (_g, path) = tmp_db();
        {
            let mut reg = HopperRegistry::open(&path).unwrap();
            reg.add(WatchInput {
                source_dir: "/a".into(),
                output_dir: "/b".into(),
                recipe_id: None,
                rename_pattern: None,
                ai_rename: false,
            })
            .unwrap();
        }
        // Re-open & re-apply schema — must not nuke existing rows.
        let reg = HopperRegistry::open(&path).unwrap();
        assert_eq!(reg.list().unwrap().len(), 1);
    }

    #[test]
    fn remove_is_idempotent() {
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let id = reg
            .add(WatchInput {
                source_dir: "/x".into(),
                output_dir: "/y".into(),
                recipe_id: None,
                rename_pattern: None,
                ai_rename: false,
            })
            .unwrap();
        reg.remove(id).unwrap();
        reg.remove(id).unwrap(); // second remove no-ops
        assert!(reg.list().unwrap().is_empty());
        assert!(reg.get(id).unwrap().is_none());
    }

    #[test]
    fn set_enabled_toggles() {
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let id = reg
            .add(WatchInput {
                source_dir: "/x".into(),
                output_dir: "/y".into(),
                recipe_id: None,
                rename_pattern: None,
                ai_rename: false,
            })
            .unwrap();
        assert!(reg.get(id).unwrap().unwrap().enabled);
        reg.set_enabled(id, false).unwrap();
        assert!(!reg.get(id).unwrap().unwrap().enabled);
        reg.set_enabled(id, true).unwrap();
        assert!(reg.get(id).unwrap().unwrap().enabled);
    }

    #[test]
    fn list_orders_by_id_ascending() {
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let mut ids = Vec::new();
        for label in ["a", "b", "c"] {
            ids.push(
                reg.add(WatchInput {
                    source_dir: format!("/in/{label}"),
                    output_dir: "/out".into(),
                    recipe_id: None,
                    rename_pattern: None,
                    ai_rename: false,
                })
                .unwrap(),
            );
        }
        let all = reg.list().unwrap();
        let listed: Vec<_> = all.iter().map(|w| w.id).collect();
        assert_eq!(listed, ids);
    }
}
