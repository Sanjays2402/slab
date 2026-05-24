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

use super::rules::Rule;

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

        // ── v3.21.0 migration: add `rules_json` (idempotent column-add).
        //
        // Sqlite has no `ADD COLUMN IF NOT EXISTS`, so probe `pragma_table_info`.
        // A NULL/empty value means "no rules" (== `[]`); the typed API in
        // [`set_rules`] / [`get_rules`] normalises both.
        let has_rules: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('watches') WHERE name = 'rules_json'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if !has_rules {
            conn.execute(
                "ALTER TABLE watches ADD COLUMN rules_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }

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

    /// Replace this watch's rule list. Pass `&[]` to clear all rules.
    ///
    /// Rules are stored as a JSON array in the `rules_json` column. The
    /// JSON shape is stable across versions (kebab-case `kind` tag) — see
    /// [`crate::pdf::hopper::rules`] for the schema.
    pub fn set_rules(&mut self, id: i64, rules: &[Rule]) -> rusqlite::Result<()> {
        let json = serde_json::to_string(rules).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?;
        self.conn.execute(
            "UPDATE watches SET rules_json = ?1 WHERE id = ?2",
            params![json, id],
        )?;
        Ok(())
    }

    /// Read this watch's rule list. Returns `Ok(vec![])` for unknown ids
    /// or empty/`[]` storage; only invalid JSON surfaces as an error.
    pub fn get_rules(&self, id: i64) -> rusqlite::Result<Vec<Rule>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT rules_json FROM watches WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        let json = json.unwrap_or_else(|| "[]".into());
        if json.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(e.to_string())),
            )
        })
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

    // ─── v3.21.0 rules_json migration + typed API ─────────────────────

    #[test]
    fn migration_adds_rules_json_column_idempotently() {
        let (_g, path) = tmp_db();
        // First open creates column.
        {
            let _ = HopperRegistry::open(&path).unwrap();
        }
        // Second open must be a no-op (would error if it re-added the col).
        {
            let _ = HopperRegistry::open(&path).unwrap();
        }
        // Verify column exists.
        let conn = rusqlite::Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('watches') WHERE name = 'rules_json'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn new_watch_starts_with_empty_rules() {
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let id = reg
            .add(WatchInput {
                source_dir: "/in".into(),
                output_dir: "/out".into(),
                recipe_id: None,
                rename_pattern: None,
                ai_rename: false,
            })
            .unwrap();
        assert!(reg.get_rules(id).unwrap().is_empty());
    }

    #[test]
    fn set_and_get_rules_round_trips() {
        use crate::pdf::hopper::rules::{Rule, RuleAction, RulePredicate};
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let id = reg
            .add(WatchInput {
                source_dir: "/in".into(),
                output_dir: "/out".into(),
                recipe_id: None,
                rename_pattern: None,
                ai_rename: false,
            })
            .unwrap();

        let rules = vec![
            Rule {
                name: "taxes".into(),
                predicate: RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
                action: RuleAction {
                    recipe_id: Some("flatten".into()),
                    output_dir: Some("/taxes".into()),
                    rename_pattern: None,
                },
            },
            Rule {
                name: "catch-all".into(),
                predicate: RulePredicate::Always,
                action: RuleAction::default(),
            },
        ];
        reg.set_rules(id, &rules).unwrap();
        let got = reg.get_rules(id).unwrap();
        assert_eq!(got, rules);
    }

    #[test]
    fn set_rules_empty_clears() {
        use crate::pdf::hopper::rules::{Rule, RulePredicate};
        let (_g, path) = tmp_db();
        let mut reg = HopperRegistry::open(&path).unwrap();
        let id = reg
            .add(WatchInput {
                source_dir: "/in".into(),
                output_dir: "/out".into(),
                recipe_id: None,
                rename_pattern: None,
                ai_rename: false,
            })
            .unwrap();
        reg.set_rules(
            id,
            &[Rule {
                name: "x".into(),
                predicate: RulePredicate::Always,
                action: Default::default(),
            }],
        )
        .unwrap();
        assert_eq!(reg.get_rules(id).unwrap().len(), 1);
        reg.set_rules(id, &[]).unwrap();
        assert!(reg.get_rules(id).unwrap().is_empty());
    }

    #[test]
    fn get_rules_survives_reopen() {
        use crate::pdf::hopper::rules::{Rule, RulePredicate};
        let (_g, path) = tmp_db();
        let id = {
            let mut reg = HopperRegistry::open(&path).unwrap();
            let id = reg
                .add(WatchInput {
                    source_dir: "/in".into(),
                    output_dir: "/out".into(),
                    recipe_id: None,
                    rename_pattern: None,
                    ai_rename: false,
                })
                .unwrap();
            reg.set_rules(
                id,
                &[Rule {
                    name: "persist-me".into(),
                    predicate: RulePredicate::Always,
                    action: Default::default(),
                }],
            )
            .unwrap();
            id
        };
        let reg = HopperRegistry::open(&path).unwrap();
        let got = reg.get_rules(id).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "persist-me");
    }

    #[test]
    fn get_rules_unknown_id_returns_empty() {
        let (_g, path) = tmp_db();
        let reg = HopperRegistry::open(&path).unwrap();
        assert!(reg.get_rules(9999).unwrap().is_empty());
    }
}
