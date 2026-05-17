// Library registry — sqlite-backed CRUD on the four library tables.
//
// Schema is versioned via `PRAGMA user_version`. `init_schema` is
// idempotent: re-running it against an existing DB just bumps any
// pending migrations forward. Tests use in-memory DBs.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("sqlite: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("library: {0}")]
    Other(String),
}

/// One row of `library_folders`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderRecord {
    pub id: i64,
    pub path: String,
    pub added_at: i64,
    pub last_scanned_at: Option<i64>,
}

/// One row of `library_documents`, with `tags` eager-loaded by
/// `query::query_documents`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRecord {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub path: String,
    pub title: Option<String>,
    pub hash: String,
    pub size_bytes: i64,
    pub mtime_ns: i64,
    pub pages: Option<i64>,
    pub added_at: i64,
    pub last_seen_at: i64,
    #[serde(default)]
    pub tags: Vec<TagRecord>,
}

/// One row of `library_tags`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagRecord {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

/// Returns the default path Slab opens its library DB at.
/// (`~/.slab/library.sqlite`). Tests don't need this — they use
/// `LibraryDb::open_in_memory`.
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("library.sqlite")
}

/// Owning handle to the library SQLite connection. Open once per app
/// run; all CRUD calls go through this.
pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    /// Open the on-disk library DB, creating parent dirs + schema as
    /// needed.
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory DB (tests).
    pub fn open_in_memory() -> Result<Self, LibraryError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), LibraryError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_folders (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    added_at INTEGER NOT NULL,
                    last_scanned_at INTEGER
                );

                CREATE TABLE IF NOT EXISTS library_documents (
                    id INTEGER PRIMARY KEY,
                    folder_id INTEGER REFERENCES library_folders(id) ON DELETE CASCADE,
                    path TEXT NOT NULL UNIQUE,
                    title TEXT,
                    hash TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    mtime_ns INTEGER NOT NULL,
                    pages INTEGER,
                    added_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_documents_folder ON library_documents(folder_id);
                CREATE INDEX IF NOT EXISTS idx_documents_hash ON library_documents(hash);

                CREATE TABLE IF NOT EXISTS library_tags (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT
                );

                CREATE TABLE IF NOT EXISTS library_doc_tags (
                    doc_id INTEGER NOT NULL REFERENCES library_documents(id) ON DELETE CASCADE,
                    tag_id INTEGER NOT NULL REFERENCES library_tags(id) ON DELETE CASCADE,
                    PRIMARY KEY (doc_id, tag_id)
                );
                "#,
            )?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    /// Borrow the underlying connection — scanner needs it.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Borrow mutably — scanner uses a transaction.
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Currently-encoded schema version. Bumps on each migration.
    pub fn schema_version(&self) -> Result<u32, LibraryError> {
        let v: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(v)
    }

    // ---------------------------------------------------------------
    // Folders
    // ---------------------------------------------------------------

    /// Insert a folder. If it already exists by path, returns the
    /// existing row (idempotent — we don't want to error when a user
    /// re-adds an already-tracked folder).
    pub fn add_folder(&mut self, path: &str) -> Result<FolderRecord, LibraryError> {
        if let Some(existing) = self.find_folder_by_path(path)? {
            return Ok(existing);
        }
        let now = now_unix();
        self.conn.execute(
            "INSERT INTO library_folders (path, added_at) VALUES (?1, ?2)",
            params![path, now],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(FolderRecord {
            id,
            path: path.to_string(),
            added_at: now,
            last_scanned_at: None,
        })
    }

    pub fn find_folder_by_path(&self, path: &str) -> Result<Option<FolderRecord>, LibraryError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, path, added_at, last_scanned_at FROM library_folders WHERE path = ?1",
                params![path],
                folder_from_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn remove_folder(&mut self, id: i64) -> Result<(), LibraryError> {
        self.conn
            .execute("DELETE FROM library_folders WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// All folders, ordered by added_at ascending (oldest first).
    pub fn list_folders(&self) -> Result<Vec<FolderRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, added_at, last_scanned_at FROM library_folders ORDER BY added_at ASC, id ASC",
        )?;
        let rows = stmt
            .query_map([], folder_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn mark_folder_scanned(&mut self, id: i64, when: i64) -> Result<(), LibraryError> {
        self.conn.execute(
            "UPDATE library_folders SET last_scanned_at = ?1 WHERE id = ?2",
            params![when, id],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Documents
    // ---------------------------------------------------------------

    /// Insert-or-update a document keyed by `path`. When the path is
    /// already present, hash/size/mtime/pages/last_seen_at are refreshed.
    pub fn upsert_document(
        &mut self,
        folder_id: Option<i64>,
        path: &str,
        title: Option<&str>,
        hash: &str,
        size_bytes: i64,
        mtime_ns: i64,
        pages: Option<i64>,
    ) -> Result<DocumentRecord, LibraryError> {
        let now = now_unix();
        let existing = self.find_document_by_path(path)?;
        match existing {
            Some(_) => {
                self.conn.execute(
                    "UPDATE library_documents
                     SET folder_id = ?1, title = COALESCE(?2, title), hash = ?3,
                         size_bytes = ?4, mtime_ns = ?5, pages = COALESCE(?6, pages),
                         last_seen_at = ?7
                     WHERE path = ?8",
                    params![folder_id, title, hash, size_bytes, mtime_ns, pages, now, path,],
                )?;
            }
            None => {
                self.conn.execute(
                    "INSERT INTO library_documents
                     (folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        folder_id, path, title, hash, size_bytes, mtime_ns, pages, now,
                    ],
                )?;
            }
        }
        // Re-read so we get back a normalized row including any
        // pre-existing tags.
        let mut doc = self.find_document_by_path(path)?.ok_or_else(|| {
            LibraryError::Other("upsert succeeded but row not found on re-read".into())
        })?;
        doc.tags = self.tags_for_document(doc.id)?;
        Ok(doc)
    }

    /// Bump only `last_seen_at` (scanner uses this for unchanged files
    /// it spotted on the new scan).
    pub fn touch_document(&mut self, doc_id: i64) -> Result<(), LibraryError> {
        let now = now_unix();
        self.conn.execute(
            "UPDATE library_documents SET last_seen_at = ?1 WHERE id = ?2",
            params![now, doc_id],
        )?;
        Ok(())
    }

    pub fn find_document_by_path(
        &self,
        path: &str,
    ) -> Result<Option<DocumentRecord>, LibraryError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at
                 FROM library_documents WHERE path = ?1",
                params![path],
                document_from_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Tags attached to a single document, ordered by name.
    pub fn tags_for_document(&self, doc_id: i64) -> Result<Vec<TagRecord>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, t.color FROM library_tags t
             INNER JOIN library_doc_tags dt ON dt.tag_id = t.id
             WHERE dt.doc_id = ?1
             ORDER BY t.name ASC",
        )?;
        let rows = stmt
            .query_map(params![doc_id], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // ---------------------------------------------------------------
    // Tags
    // ---------------------------------------------------------------

    pub fn add_tag(&mut self, name: &str, color: Option<&str>) -> Result<TagRecord, LibraryError> {
        if let Some(existing) = self.find_tag_by_name(name)? {
            return Ok(existing);
        }
        self.conn.execute(
            "INSERT INTO library_tags (name, color) VALUES (?1, ?2)",
            params![name, color],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(TagRecord {
            id,
            name: name.to_string(),
            color: color.map(str::to_string),
        })
    }

    pub fn find_tag_by_name(&self, name: &str) -> Result<Option<TagRecord>, LibraryError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, color FROM library_tags WHERE name = ?1",
                params![name],
                tag_from_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_tags(&self) -> Result<Vec<TagRecord>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, color FROM library_tags ORDER BY name ASC")?;
        let rows = stmt
            .query_map([], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Replace this document's tag set with exactly `tag_ids`. Missing
    /// tag ids that exist in the table become attached; ids not in
    /// the slice get detached.
    pub fn set_doc_tags(&mut self, doc_id: i64, tag_ids: &[i64]) -> Result<(), LibraryError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM library_doc_tags WHERE doc_id = ?1",
            params![doc_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO library_doc_tags (doc_id, tag_id) VALUES (?1, ?2)",
            )?;
            for tid in tag_ids {
                stmt.execute(params![doc_id, tid])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn folder_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FolderRecord> {
    Ok(FolderRecord {
        id: row.get(0)?,
        path: row.get(1)?,
        added_at: row.get(2)?,
        last_scanned_at: row.get(3)?,
    })
}

fn document_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentRecord> {
    Ok(DocumentRecord {
        id: row.get(0)?,
        folder_id: row.get(1)?,
        path: row.get(2)?,
        title: row.get(3)?,
        hash: row.get(4)?,
        size_bytes: row.get(5)?,
        mtime_ns: row.get(6)?,
        pages: row.get(7)?,
        added_at: row.get(8)?,
        last_seen_at: row.get(9)?,
        tags: Vec::new(),
    })
}

fn tag_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRecord> {
    Ok(TagRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().expect("open in-memory DB")
    }

    #[test]
    fn schema_version_is_set() {
        let db = db();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn add_folder_returns_record_with_id() {
        let mut db = db();
        let f = db.add_folder("/tmp/papers").unwrap();
        assert!(f.id > 0);
        assert_eq!(f.path, "/tmp/papers");
        assert!(f.added_at > 0);
        assert!(f.last_scanned_at.is_none());
    }

    #[test]
    fn add_folder_duplicate_path_returns_existing() {
        let mut db = db();
        let a = db.add_folder("/tmp/papers").unwrap();
        let b = db.add_folder("/tmp/papers").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn list_folders_returns_ordered() {
        let mut db = db();
        let a = db.add_folder("/a").unwrap();
        let b = db.add_folder("/b").unwrap();
        let c = db.add_folder("/c").unwrap();
        let listed = db.list_folders().unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].id, a.id);
        assert_eq!(listed[1].id, b.id);
        assert_eq!(listed[2].id, c.id);
    }

    #[test]
    fn remove_folder_works() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        db.remove_folder(f.id).unwrap();
        assert!(db.find_folder_by_path("/tmp").unwrap().is_none());
    }

    #[test]
    fn mark_folder_scanned_updates_timestamp() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        db.mark_folder_scanned(f.id, 12345).unwrap();
        let f2 = db.find_folder_by_path("/tmp").unwrap().unwrap();
        assert_eq!(f2.last_scanned_at, Some(12345));
    }

    #[test]
    fn upsert_document_inserts_new() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(
                Some(f.id),
                "/tmp/a.pdf",
                Some("Title"),
                "deadbeef",
                100,
                42,
                Some(3),
            )
            .unwrap();
        assert!(d.id > 0);
        assert_eq!(d.path, "/tmp/a.pdf");
        assert_eq!(d.hash, "deadbeef");
        assert_eq!(d.size_bytes, 100);
        assert_eq!(d.mtime_ns, 42);
        assert_eq!(d.pages, Some(3));
        assert_eq!(d.title.as_deref(), Some("Title"));
    }

    #[test]
    fn upsert_document_updates_existing_by_path() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d1 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "v1", 100, 1, Some(3))
            .unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", Some("New"), "v2", 200, 2, Some(5))
            .unwrap();
        assert_eq!(d1.id, d2.id, "upsert should keep the same id");
        assert_eq!(d2.hash, "v2");
        assert_eq!(d2.size_bytes, 200);
        assert_eq!(d2.pages, Some(5));
        assert_eq!(d2.title.as_deref(), Some("New"));
    }

    #[test]
    fn cascade_delete_folder_drops_documents() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        db.upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None)
            .unwrap();
        db.remove_folder(f.id).unwrap();
        assert!(db.find_document_by_path("/tmp/a.pdf").unwrap().is_none());
    }

    #[test]
    fn touch_document_updates_last_seen() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None)
            .unwrap();
        let original = d.last_seen_at;
        std::thread::sleep(std::time::Duration::from_millis(1100));
        db.touch_document(d.id).unwrap();
        let d2 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert!(d2.last_seen_at >= original);
    }

    #[test]
    fn add_tag_unique_constraint() {
        let mut db = db();
        let a = db.add_tag("research", Some("#f00")).unwrap();
        let b = db.add_tag("research", None).unwrap();
        assert_eq!(a.id, b.id, "duplicate add_tag should return existing row");
    }

    #[test]
    fn list_tags_ordered_by_name() {
        let mut db = db();
        db.add_tag("zebra", None).unwrap();
        db.add_tag("apple", None).unwrap();
        db.add_tag("mango", None).unwrap();
        let listed = db.list_tags().unwrap();
        assert_eq!(
            listed.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["apple", "mango", "zebra"]
        );
    }

    #[test]
    fn set_doc_tags_replaces_existing() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None)
            .unwrap();
        let t1 = db.add_tag("research", None).unwrap();
        let t2 = db.add_tag("draft", None).unwrap();
        let t3 = db.add_tag("done", None).unwrap();

        db.set_doc_tags(d.id, &[t1.id, t2.id]).unwrap();
        let attached = db.tags_for_document(d.id).unwrap();
        assert_eq!(attached.len(), 2);
        assert!(attached.iter().any(|t| t.id == t1.id));
        assert!(attached.iter().any(|t| t.id == t2.id));

        // Replace — keep t1, drop t2, add t3.
        db.set_doc_tags(d.id, &[t1.id, t3.id]).unwrap();
        let attached = db.tags_for_document(d.id).unwrap();
        assert_eq!(attached.len(), 2);
        assert!(attached.iter().any(|t| t.id == t1.id));
        assert!(attached.iter().any(|t| t.id == t3.id));
        assert!(!attached.iter().any(|t| t.id == t2.id));
    }

    #[test]
    fn upsert_document_carries_tags_on_re_read() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None)
            .unwrap();
        let t1 = db.add_tag("research", None).unwrap();
        db.set_doc_tags(d.id, &[t1.id]).unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None)
            .unwrap();
        assert_eq!(d2.tags.len(), 1);
        assert_eq!(d2.tags[0].id, t1.id);
    }

    #[test]
    fn default_db_path_points_at_dot_slab() {
        let p = default_db_path();
        let s = p.to_string_lossy();
        assert!(s.ends_with("/.slab/library.sqlite") || s.ends_with("\\.slab\\library.sqlite"));
    }
}
