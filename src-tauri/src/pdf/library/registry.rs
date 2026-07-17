// Library registry — sqlite-backed CRUD on the four library tables.
//
// Schema is versioned via `PRAGMA user_version`. `init_schema` is
// idempotent: re-running it against an existing DB just bumps any
// pending migrations forward. Tests use in-memory DBs.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 15;

/// Initial / unknown OCR classification — written for legacy rows that
/// predate Slice 2 (auto-OCR queue) and for documents the scanner has
/// not yet inspected.
pub const OCR_STATE_UNKNOWN: &str = "unknown";
/// Document is text-native — `scan_audit` says no OCR needed.
pub const OCR_STATE_TEXT_NATIVE: &str = "text_native";
/// Document is image-only — `scan_audit::Recommendation::OcrAll`.
pub const OCR_STATE_SCANNED: &str = "scanned";
/// Document has a mix of scanned and text pages — `Recommendation::OcrSome`.
pub const OCR_STATE_MIXED: &str = "mixed";
/// Queue is actively OCRing this document.
pub const OCR_STATE_PENDING: &str = "ocr_pending";
/// OCR finished — `ocr_output_path` points at the searchable PDF.
pub const OCR_STATE_DONE: &str = "ocr_done";
/// OCR was attempted and failed (e.g. tesseract missing, corrupt PDF).
pub const OCR_STATE_FAILED: &str = "ocr_failed";

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
    /// One of the `OCR_STATE_*` constants. Defaults to `unknown` for
    /// legacy rows or until the scanner runs `scan_audit` on the file.
    #[serde(default = "default_ocr_state")]
    pub ocr_state: String,
    /// When `ocr_state == "ocr_done"`, the path of the generated
    /// searchable PDF (typically `<basename>.ocr.pdf` next to the
    /// original). NULL otherwise.
    #[serde(default)]
    pub ocr_output_path: Option<String>,
    /// When `ocr_state == "ocr_failed"`, the captured error message from
    /// the queue worker (e.g. "tesseract not found on PATH", "page 14
    /// rasterised at 0 bytes"). Cleared back to NULL on a successful
    /// re-OCR. v3.52.0 Atlas OCR-Queue.
    #[serde(default)]
    pub ocr_error: Option<String>,
    /// Per-doc freeform notes shown in the Doc-Inspector drawer. Trimmed,
    /// `None` when unset (no override). Cap is [`MAX_DOC_NOTES_LEN`] Unicode
    /// scalars so a runaway paste can't bloat the row. v3.55.0 Atlas
    /// Doc-Inspector.
    #[serde(default)]
    pub notes: Option<String>,
    /// Whether the user has starred this document. Surfaced on the doc-card
    /// as a ★ glyph and filterable via [`LibraryFilter::starred_only`] or
    /// the `Starred` clause variant. Defaults to `false` so every pre-v14
    /// row silently reads as unstarred. v3.55.0 Atlas Doc-Inspector.
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub tags: Vec<TagRecord>,
}

fn default_ocr_state() -> String {
    OCR_STATE_UNKNOWN.to_string()
}

/// One row of `library_tags`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagRecord {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    /// Optional freeform note about the tag. Trimmed, never empty (empty
    /// trims clear the column). Surfaced as the rail tooltip + the doc-card
    /// chip tooltip so people can capture *why* a tag exists ("invoices
    /// pending bookkeeper review"). v3.51.0 Atlas Tag-Descriptions.
    pub description: Option<String>,
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
            conn.execute_batch("PRAGMA user_version = 1;")?;
        }
        if version < 2 {
            // Slice 2 — auto-OCR queue. Add per-doc state machine + the
            // output path the queue writes when OCR succeeds.
            conn.execute_batch(
                r#"
                ALTER TABLE library_documents
                  ADD COLUMN ocr_state TEXT NOT NULL DEFAULT 'unknown';
                ALTER TABLE library_documents
                  ADD COLUMN ocr_output_path TEXT;
                CREATE INDEX IF NOT EXISTS idx_documents_ocr_state
                  ON library_documents(ocr_state);
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 2;")?;
        }
        if version < 3 {
            // Slice for Atlas (v2.2.0) — FTS5 cross-document search.
            // Delegated to the `fts` submodule so the migration sits next
            // to the index code that owns it. This also bumps
            // user_version to SCHEMA_VERSION.
            super::fts::migrate_v3(conn)?;
        }
        if version < 4 {
            // v3.32.0 "Atlas" — Collections + Smart Collections.
            // Manual `library_collections` group docs; smart variants
            // store a saved `LibraryFilter` JSON that runs live.
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_collections (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    icon TEXT,
                    color TEXT,
                    created_at INTEGER NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS library_collection_docs (
                    collection_id INTEGER NOT NULL
                        REFERENCES library_collections(id) ON DELETE CASCADE,
                    doc_id INTEGER NOT NULL
                        REFERENCES library_documents(id) ON DELETE CASCADE,
                    added_at INTEGER NOT NULL,
                    PRIMARY KEY (collection_id, doc_id)
                );
                CREATE INDEX IF NOT EXISTS idx_collection_docs_doc
                    ON library_collection_docs(doc_id);
                CREATE TABLE IF NOT EXISTS library_smart_collections (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    icon TEXT,
                    color TEXT,
                    query_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 0
                );
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 4;")?;
        }
        if version < 5 {
            // v3.36.0 "Atlas Personal Presets" — user-saved smart-collection
            // recipes. Stored as opaque filter_json so the entire FilterGroup
            // tree survives schema changes to the query language.
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_personal_presets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    icon TEXT,
                    color TEXT,
                    description TEXT,
                    filter_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 0
                );
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 5;")?;
        }
        if version < 6 {
            // v3.37.0 "Atlas Smart Folders Hub" — persisted display order
            // and pin state across built-in + personal preset ids.
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_smart_folder_order (
                    entry_kind TEXT NOT NULL,
                    entry_id   TEXT NOT NULL,
                    sort_order INTEGER NOT NULL,
                    pinned     INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (entry_kind, entry_id)
                );
                CREATE INDEX IF NOT EXISTS idx_smart_folder_order
                    ON library_smart_folder_order(sort_order);
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 6;")?;
        }
        if version < 7 {
            // v3.38.0 "Atlas Suggest" — rolling log of recent library
            // searches + dismissed suggestion clusters. The suggestion
            // engine reads from these tables to propose personal Smart
            // Folders ("you searched 'invoice' 8 times — save a folder?").
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_search_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    query TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    result_count INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_search_log_ts
                    ON library_search_log(ts DESC);
                CREATE TABLE IF NOT EXISTS library_suggestion_dismissed (
                    cluster_hash TEXT PRIMARY KEY,
                    ts INTEGER NOT NULL
                );
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 7;")?;
        }
        if version < 8 {
            // v3.39.0 "Atlas Tag-Suggest" — per-doc dismissals for the
            // heuristic tag suggester. When the user clicks ✗ on a
            // suggested tag chip, we record (doc_id, tag_name) here so
            // that suggestion never resurfaces for that document.
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_tag_suggestion_dismissed (
                    doc_id INTEGER NOT NULL
                        REFERENCES library_documents(id) ON DELETE CASCADE,
                    tag_name TEXT NOT NULL,
                    dismissed_at INTEGER NOT NULL,
                    PRIMARY KEY (doc_id, tag_name)
                );
                CREATE INDEX IF NOT EXISTS idx_tag_sugg_dismissed_doc
                    ON library_tag_suggestion_dismissed(doc_id);
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 8;")?;
        }
        if version < 9 {
            // v3.44.0 "Atlas Recent-Tags" — stamp each (doc, tag) link with
            // the time the tag was applied so the UI can surface the most
            // recently used tags as quick chips. Legacy links predate this
            // column and carry a NULL applied_at (they sort last in the
            // recently-used list but are not lost). The (tag_id, applied_at)
            // index serves the `recently_used_tags` GROUP BY + MAX().
            conn.execute_batch(
                r#"
                ALTER TABLE library_doc_tags ADD COLUMN applied_at INTEGER;
                CREATE INDEX IF NOT EXISTS idx_doc_tags_tag_applied
                    ON library_doc_tags(tag_id, applied_at);
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 9;")?;
        }
        if version < 10 {
            // v3.50.0 "Atlas Saved Views" — one-click restorable rail
            // filters. A saved view is a name + the full LibraryFilter
            // serialized as JSON (folder + tag selection + match mode +
            // untagged toggle + sort, whatever the user had pinned).
            // Distinct from `library_personal_presets` (which materialize
            // into smart_collections) and from `library_smart_collections`
            // (which own a doc list) — a view just RE-RUNS the live filter.
            // Stored as opaque filter_json so the entire LibraryFilter
            // tree survives schema changes to the query language, exactly
            // like personal_presets does.
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS library_saved_views (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    filter_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 0
                );
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 10;")?;
        }
        if version < 11 {
            // v3.51.0 "Atlas Tag-Descriptions" — an optional freeform note per
            // tag, surfaced as the rail tooltip + every doc-card chip tooltip
            // so the *why* of a tag travels with it. Nullable so every pre-v11
            // tag silently picks up `NULL` (no rewrite, no defaulting); the
            // setter rejects oversized text and trims empty to NULL so the
            // column only ever holds "real" notes.
            conn.execute_batch("ALTER TABLE library_tags ADD COLUMN description TEXT;")?;
            conn.execute_batch("PRAGMA user_version = 11;")?;
        }
        if version < 12 {
            // v3.52.0 "Atlas OCR-Queue" — persist the OCR worker's error
            // message alongside `ocr_state = 'ocr_failed'` so the failure
            // inbox can show *why* a doc failed (tesseract missing, page
            // unrasterisable, source vanished) and the user can decide to
            // re-queue, edit the source, or remove the row. Nullable —
            // every pre-v12 row silently picks up NULL (no rewrite); set
            // back to NULL on a successful re-OCR so the column only ever
            // holds the most recent failure.
            conn.execute_batch("ALTER TABLE library_documents ADD COLUMN ocr_error TEXT;")?;
            conn.execute_batch("PRAGMA user_version = 12;")?;
        }
        if version < 13 {
            // v3.55.0 "Atlas Doc-Inspector" — per-doc freeform notes for the
            // inspector drawer. Nullable so every pre-v13 row silently picks
            // up `NULL` (no rewrite, no defaulting); the setter trims input
            // and empty-after-trim clears the column back to NULL so it only
            // ever holds "real" notes. Capped at MAX_DOC_NOTES_LEN at the
            // application layer; the column is plain TEXT because SQLite
            // doesn't enforce length and we want the validation message —
            // not a constraint-violation rusqlite blob — on the wire.
            conn.execute_batch("ALTER TABLE library_documents ADD COLUMN notes TEXT;")?;
            conn.execute_batch("PRAGMA user_version = 13;")?;
        }
        if version < 14 {
            // v3.55.0 "Atlas Doc-Inspector" — per-doc star/favorite flag.
            // Stored as INTEGER 0/1 (SQLite has no BOOLEAN type; INT is the
            // canonical encoding). DEFAULT 0 so every pre-v14 row reads as
            // "not starred" without a rewrite. NOT NULL because a tri-state
            // would just complicate the filter SQL — unset == 0 is enough.
            // Indexed because the LibraryFilter `starred_only` flag and the
            // sidebar "Starred" quick-filter both filter on this column;
            // a partial index (`WHERE starred = 1`) is cheap because only
            // a small fraction of the library is ever starred.
            conn.execute_batch(
                r#"
                ALTER TABLE library_documents
                  ADD COLUMN starred INTEGER NOT NULL DEFAULT 0;
                CREATE INDEX IF NOT EXISTS idx_documents_starred
                  ON library_documents(starred) WHERE starred = 1;
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 14;")?;
        }
        if version < 15 {
            // v3.56.0 "Atlas Saved-Views-Polish" — pin flag for saved views.
            // DEFAULT 0 so every pre-v15 row reads as unpinned without a
            // rewrite. NOT NULL — a tri-state pin would just complicate the
            // ORDER BY without adding anything (an unset pin and a false
            // pin are the same UI outcome). Partial index keyed on
            // `WHERE pinned = 1` because only a small fraction of saved
            // views are ever pinned — the list still ORDER BYs pinned DESC
            // first, then sort_order ASC, then name ASC for the rail's
            // "pinned-first" surfacing.
            conn.execute_batch(
                r#"
                ALTER TABLE library_saved_views
                  ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
                CREATE INDEX IF NOT EXISTS idx_saved_views_pinned
                  ON library_saved_views(pinned) WHERE pinned = 1;
                "#,
            )?;
            conn.execute_batch("PRAGMA user_version = 15;")?;
            debug_assert_eq!(SCHEMA_VERSION, 15);
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
    ///
    /// `initial_ocr_state` is only honored on first insert (or when the
    /// existing row still has `ocr_state = 'unknown'` — i.e. legacy /
    /// pre-Slice-2 rows that the scanner has now classified). Once an
    /// `ocr_state` has been set explicitly we refuse to overwrite it
    /// here — use `set_doc_ocr_state` for state-machine transitions
    /// (queue marks pending/done/failed).
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_document(
        &mut self,
        folder_id: Option<i64>,
        path: &str,
        title: Option<&str>,
        hash: &str,
        size_bytes: i64,
        mtime_ns: i64,
        pages: Option<i64>,
        initial_ocr_state: Option<&str>,
    ) -> Result<DocumentRecord, LibraryError> {
        let now = now_unix();
        let existing = self.find_document_by_path(path)?;
        match existing {
            Some(ref e) => {
                self.conn.execute(
                    "UPDATE library_documents
                     SET folder_id = ?1, title = COALESCE(?2, title), hash = ?3,
                         size_bytes = ?4, mtime_ns = ?5, pages = COALESCE(?6, pages),
                         last_seen_at = ?7
                     WHERE path = ?8",
                    params![folder_id, title, hash, size_bytes, mtime_ns, pages, now, path,],
                )?;
                // Only upgrade ocr_state when it's still the default
                // 'unknown' — never clobber a real classification or a
                // queue state (`ocr_pending` / `ocr_done` / `ocr_failed`).
                if e.ocr_state == OCR_STATE_UNKNOWN {
                    if let Some(state) = initial_ocr_state {
                        self.conn.execute(
                            "UPDATE library_documents SET ocr_state = ?1 WHERE path = ?2",
                            params![state, path],
                        )?;
                    }
                }
            }
            None => {
                let state = initial_ocr_state.unwrap_or(OCR_STATE_UNKNOWN);
                self.conn.execute(
                    "INSERT INTO library_documents
                     (folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at, ocr_state)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9)",
                    params![
                        folder_id, path, title, hash, size_bytes, mtime_ns, pages, now, state,
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

    /// Set the OCR state machine value for a document. Used by the
    /// queue worker to mark pending → done / failed transitions.
    pub fn set_doc_ocr_state(&mut self, doc_id: i64, state: &str) -> Result<(), LibraryError> {
        self.conn.execute(
            "UPDATE library_documents SET ocr_state = ?1 WHERE id = ?2",
            params![state, doc_id],
        )?;
        Ok(())
    }

    /// Record where the OCR queue wrote the searchable PDF when an OCR
    /// pass succeeded. Pass `None` to clear (e.g. on a re-queue).
    pub fn set_doc_ocr_output_path(
        &mut self,
        doc_id: i64,
        output_path: Option<&str>,
    ) -> Result<(), LibraryError> {
        self.conn.execute(
            "UPDATE library_documents SET ocr_output_path = ?1 WHERE id = ?2",
            params![output_path, doc_id],
        )?;
        Ok(())
    }

    /// Record the OCR worker's error message for a document. Pass `None`
    /// (or an all-whitespace string) to clear the column. Trims input so a
    /// blank explanation never sits in the DB. v3.52.0 Atlas OCR-Queue.
    pub fn set_doc_ocr_error(
        &mut self,
        doc_id: i64,
        error: Option<&str>,
    ) -> Result<(), LibraryError> {
        let trimmed = error.map(str::trim).filter(|s| !s.is_empty());
        self.conn.execute(
            "UPDATE library_documents SET ocr_error = ?1 WHERE id = ?2",
            params![trimmed, doc_id],
        )?;
        Ok(())
    }

    /// Override a library document's displayed `title`. The persisted
    /// title is what the LibraryPanel card and the search rail show; if
    /// it is `NULL`, the UI falls back to the file's basename. This is
    /// the user's lever to give a meaningless filename (`scan_001.pdf`)
    /// a real human label without renaming the file on disk.
    ///
    /// `title` is trimmed. Pass `None` — or any string that trims to
    /// empty — to clear the override back to `NULL` so the basename
    /// fallback resumes. The trimmed value is capped at
    /// [`MAX_DOC_TITLE_LEN`] Unicode scalars (cap measured in `chars`
    /// so emoji and CJK get a sane budget). Errors on unknown id or
    /// oversized text; a rejected update leaves the row's prior title
    /// untouched because the length check runs before the UPDATE.
    /// Returns the refreshed [`DocumentRecord`] with tags re-loaded so
    /// the UI can splice it back into the doc grid without a full
    /// list_documents round-trip. v3.55.0 Atlas Doc-Inspector.
    pub fn set_doc_title(
        &mut self,
        doc_id: i64,
        title: Option<&str>,
    ) -> Result<DocumentRecord, LibraryError> {
        let normalized: Option<String> = match title {
            Some(t) => {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    None
                } else if !valid_doc_title(trimmed) {
                    return Err(LibraryError::Other(format!(
                        "document title too long (max {MAX_DOC_TITLE_LEN} chars)"
                    )));
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };
        let changed = self.conn.execute(
            "UPDATE library_documents SET title = ?1 WHERE id = ?2",
            params![normalized, doc_id],
        )?;
        if changed == 0 {
            return Err(LibraryError::Other(format!("document {doc_id} not found")));
        }
        // Re-read so we return a normalized row, then eager-load tags so
        // the UI doesn't need a second round-trip to repaint the card.
        let mut doc = self
            .conn
            .query_row(
                "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes, starred
                 FROM library_documents WHERE id = ?1",
                params![doc_id],
                document_from_row,
            )
            .optional()?
            .ok_or_else(|| LibraryError::Other(format!("document {doc_id} not found")))?;
        doc.tags = self.tags_for_document(doc.id)?;
        Ok(doc)
    }

    /// Set (or clear) the freeform `notes` on a library document. Pass `None`
    /// — or any string that trims to empty — to clear the column back to
    /// NULL so the inspector renders the empty-state placeholder. The
    /// persisted text is **always trimmed**, capped at [`MAX_DOC_NOTES_LEN`]
    /// Unicode scalars (counted in `chars` so emoji and CJK get a sane
    /// budget). Errors on unknown id or oversized text; the length check
    /// runs BEFORE the UPDATE so a rejected setter leaves the row's prior
    /// notes untouched. Returns the refreshed [`DocumentRecord`] with tags
    /// eager-loaded so the Doc-Inspector drawer can repaint without an
    /// extra list_documents round-trip. v3.55.0 Atlas Doc-Inspector.
    pub fn set_doc_notes(
        &mut self,
        doc_id: i64,
        notes: Option<&str>,
    ) -> Result<DocumentRecord, LibraryError> {
        let normalized: Option<String> = match notes {
            Some(n) => {
                let trimmed = n.trim();
                if trimmed.is_empty() {
                    None
                } else if !valid_doc_notes(trimmed) {
                    return Err(LibraryError::Other(format!(
                        "document notes too long (max {MAX_DOC_NOTES_LEN} chars)"
                    )));
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };
        let changed = self.conn.execute(
            "UPDATE library_documents SET notes = ?1 WHERE id = ?2",
            params![normalized, doc_id],
        )?;
        if changed == 0 {
            return Err(LibraryError::Other(format!("document {doc_id} not found")));
        }
        let mut doc = self
            .conn
            .query_row(
                "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes, starred
                 FROM library_documents WHERE id = ?1",
                params![doc_id],
                document_from_row,
            )
            .optional()?
            .ok_or_else(|| LibraryError::Other(format!("document {doc_id} not found")))?;
        doc.tags = self.tags_for_document(doc.id)?;
        Ok(doc)
    }

    /// Set the `starred` flag on a library document. Idempotent: setting an
    /// already-`true` flag to `true` is a no-op UPDATE that returns the row
    /// unchanged (count == 1 still — SQLite reports rows matched, not rows
    /// whose value changed). Errors on unknown id. Returns the refreshed
    /// [`DocumentRecord`] with tags eager-loaded so the UI can splice the
    /// card without a list_documents round-trip. v3.55.0 Atlas Doc-Inspector.
    pub fn set_doc_starred(
        &mut self,
        doc_id: i64,
        starred: bool,
    ) -> Result<DocumentRecord, LibraryError> {
        let val: i64 = if starred { 1 } else { 0 };
        let changed = self.conn.execute(
            "UPDATE library_documents SET starred = ?1 WHERE id = ?2",
            params![val, doc_id],
        )?;
        if changed == 0 {
            return Err(LibraryError::Other(format!("document {doc_id} not found")));
        }
        let mut doc = self
            .conn
            .query_row(
                "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes, starred
                 FROM library_documents WHERE id = ?1",
                params![doc_id],
                document_from_row,
            )
            .optional()?
            .ok_or_else(|| LibraryError::Other(format!("document {doc_id} not found")))?;
        doc.tags = self.tags_for_document(doc.id)?;
        Ok(doc)
    }

    /// Remove a document row by id. ON DELETE CASCADE on
    /// `library_doc_tags` will drop its tag links too.
    pub fn remove_document(&mut self, doc_id: i64) -> Result<(), LibraryError> {
        self.conn.execute(
            "DELETE FROM library_documents WHERE id = ?1",
            params![doc_id],
        )?;
        Ok(())
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
                "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at, ocr_state, ocr_output_path, ocr_error, notes, starred
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
            "SELECT t.id, t.name, t.color, t.description FROM library_tags t
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
            description: None,
        })
    }

    pub fn find_tag_by_name(&self, name: &str) -> Result<Option<TagRecord>, LibraryError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, color, description FROM library_tags WHERE name = ?1",
                params![name],
                tag_from_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Look up a tag by its primary key. Returns `None` if no such row.
    pub fn find_tag_by_id(&self, tag_id: i64) -> Result<Option<TagRecord>, LibraryError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, color, description FROM library_tags WHERE id = ?1",
                params![tag_id],
                tag_from_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Update an existing tag's color. Pass `None` to clear it back to the
    /// default (deterministic) rendering. Returns the updated row.
    ///
    /// Errors if the tag id does not exist or `color` is not a recognized CSS
    /// color (so the column only ever holds values the UI can safely drop into
    /// a CSS property). v3.42.0 Atlas Tag-Color editing.
    pub fn set_tag_color(
        &mut self,
        tag_id: i64,
        color: Option<&str>,
    ) -> Result<TagRecord, LibraryError> {
        let normalized = match color {
            Some(c) => {
                let trimmed = c.trim();
                if !valid_tag_color(trimmed) {
                    return Err(LibraryError::Other(format!("invalid tag color: {c:?}")));
                }
                Some(trimmed)
            }
            None => None,
        };
        let changed = self.conn.execute(
            "UPDATE library_tags SET color = ?1 WHERE id = ?2",
            params![normalized, tag_id],
        )?;
        if changed == 0 {
            return Err(LibraryError::Other(format!("tag {tag_id} not found")));
        }
        self.find_tag_by_id(tag_id)?
            .ok_or_else(|| LibraryError::Other(format!("tag {tag_id} not found")))
    }

    /// Set (or clear) the freeform `description` on a tag. Pass `None` — or any
    /// string that trims to empty — to clear the column back to `NULL`. Returns
    /// the updated row so the UI can swap it in without a full refetch.
    ///
    /// The persisted text is **always trimmed**, so trailing whitespace never
    /// makes it into a tooltip. We cap the length at `MAX_TAG_DESCRIPTION_LEN`
    /// chars (measured in Unicode scalars, not bytes, so emoji and CJK get a
    /// sane budget) so a runaway paste can't bloat the DB or the rail tooltip.
    /// Errors on unknown id or oversized text; a rejected update leaves the
    /// row's old description untouched (the length check runs before the
    /// UPDATE). v3.51.0 Atlas Tag-Descriptions.
    pub fn set_tag_description(
        &mut self,
        tag_id: i64,
        description: Option<&str>,
    ) -> Result<TagRecord, LibraryError> {
        let normalized: Option<String> = match description {
            Some(d) => {
                let trimmed = d.trim();
                if trimmed.is_empty() {
                    None
                } else if !valid_tag_description(trimmed) {
                    return Err(LibraryError::Other(format!(
                        "tag description too long (max {MAX_TAG_DESCRIPTION_LEN} chars)"
                    )));
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };
        let changed = self.conn.execute(
            "UPDATE library_tags SET description = ?1 WHERE id = ?2",
            params![normalized, tag_id],
        )?;
        if changed == 0 {
            return Err(LibraryError::Other(format!("tag {tag_id} not found")));
        }
        self.find_tag_by_id(tag_id)?
            .ok_or_else(|| LibraryError::Other(format!("tag {tag_id} not found")))
    }

    /// Rename a tag everywhere it is used. Because `library_doc_tags` links
    /// documents to tags by `tag_id` (never by name), the single `UPDATE` here
    /// is automatically reflected on every document the tag is attached to and
    /// in any live tag co-occurrence the suggester computes. Returns the
    /// updated row so the UI can swap it in without a full refetch.
    ///
    /// `new_name` is trimmed. Errors if the tag id does not exist, the trimmed
    /// name is empty, or a *different* tag already carries that exact name
    /// (the `name` column is UNIQUE; we reject the collision rather than
    /// silently merging the two tags). Renaming a tag to its own current name
    /// is a no-op that returns the unchanged row. A pure case change
    /// (`research` -> `Research`) is a valid distinct rename because the UNIQUE
    /// constraint and lookups use SQLite's case-sensitive BINARY collation.
    /// v3.43.0 Atlas Tag-Rename.
    pub fn rename_tag(&mut self, tag_id: i64, new_name: &str) -> Result<TagRecord, LibraryError> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(LibraryError::Other("tag name cannot be empty".into()));
        }
        let current = self
            .find_tag_by_id(tag_id)?
            .ok_or_else(|| LibraryError::Other(format!("tag {tag_id} not found")))?;
        // Renaming to the same name is a harmless no-op.
        if current.name == trimmed {
            return Ok(current);
        }
        // Reject a collision with a *different* tag (UNIQUE name column).
        if let Some(existing) = self.find_tag_by_name(trimmed)? {
            if existing.id != tag_id {
                return Err(LibraryError::Other(format!(
                    "a tag named {trimmed:?} already exists"
                )));
            }
        }
        self.conn.execute(
            "UPDATE library_tags SET name = ?1 WHERE id = ?2",
            params![trimmed, tag_id],
        )?;
        self.find_tag_by_id(tag_id)?
            .ok_or_else(|| LibraryError::Other(format!("tag {tag_id} not found")))
    }

    /// Fold the `source_id` tag into `target_id`, then delete the source tag.
    /// Every document that wore the source tag ends up wearing the target tag,
    /// and the source row is removed — the deliberate "actually, these are the
    /// same tag" path that `rename_tag` refuses (a rename onto an existing name
    /// is rejected rather than silently merging). Returns the surviving target
    /// row so the UI can keep it selected.
    ///
    /// `applied_at` is coalesced so the recently-used order survives the merge:
    /// for a document that carried *both* tags, the surviving target link keeps
    /// the NULL-aware **newest** of the two stamps (a real timestamp always
    /// beats a legacy NULL). For a document that carried only the source tag,
    /// its link is re-pointed to the target and keeps its original stamp.
    ///
    /// Errors if either id is unknown or `source_id == target_id` (merging a
    /// tag into itself is meaningless). Validation happens before any mutation,
    /// so a rejected merge leaves every row untouched. The whole fold runs in
    /// one transaction. v3.45.0 Atlas Tag-Merge.
    pub fn merge_tags(
        &mut self,
        source_id: i64,
        target_id: i64,
    ) -> Result<TagRecord, LibraryError> {
        if source_id == target_id {
            return Err(LibraryError::Other("cannot merge a tag into itself".into()));
        }
        // Validate both ends up front so a bad id never half-applies a merge.
        self.find_tag_by_id(source_id)?
            .ok_or_else(|| LibraryError::Other(format!("tag {source_id} not found")))?;
        let target = self
            .find_tag_by_id(target_id)?
            .ok_or_else(|| LibraryError::Other(format!("tag {target_id} not found")))?;

        let tx = self.conn.transaction()?;
        {
            // 1. For documents carrying BOTH tags, lift the target link's
            //    applied_at to the NULL-aware max of the two so the merged tag
            //    keeps the more recent "used at". `max(coalesce(a,b),
            //    coalesce(b,a))` returns the larger when both are real, the
            //    real one when exactly one is NULL, and NULL only when both
            //    are NULL — exactly the ordering recently_used_tags expects.
            tx.execute(
                "UPDATE library_doc_tags
                 SET applied_at = (
                     SELECT max(coalesce(s.applied_at, library_doc_tags.applied_at),
                                coalesce(library_doc_tags.applied_at, s.applied_at))
                     FROM library_doc_tags s
                     WHERE s.doc_id = library_doc_tags.doc_id AND s.tag_id = ?1
                 )
                 WHERE tag_id = ?2
                   AND EXISTS (
                     SELECT 1 FROM library_doc_tags s
                     WHERE s.doc_id = library_doc_tags.doc_id AND s.tag_id = ?1
                   )",
                params![source_id, target_id],
            )?;
            // 2. Re-point source links onto the target. Links for documents
            //    that already carry the target collide on the (doc_id, tag_id)
            //    primary key and are skipped by OR IGNORE — those duplicates
            //    were already accounted for in step 1.
            tx.execute(
                "UPDATE OR IGNORE library_doc_tags SET tag_id = ?2 WHERE tag_id = ?1",
                params![source_id, target_id],
            )?;
            // 3. Delete the leftover (both-tag) source links OR IGNORE skipped,
            //    then drop the now-orphaned source tag row.
            tx.execute(
                "DELETE FROM library_doc_tags WHERE tag_id = ?1",
                params![source_id],
            )?;
            tx.execute("DELETE FROM library_tags WHERE id = ?1", params![source_id])?;
        }
        tx.commit()?;
        Ok(target)
    }

    pub fn list_tags(&self) -> Result<Vec<TagRecord>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, color, description FROM library_tags ORDER BY name ASC")?;
        let rows = stmt
            .query_map([], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Remove a tag by id. ON DELETE CASCADE on `library_doc_tags`
    /// will detach it from all documents in the same transaction.
    pub fn remove_tag(&mut self, tag_id: i64) -> Result<(), LibraryError> {
        self.conn
            .execute("DELETE FROM library_tags WHERE id = ?1", params![tag_id])?;
        Ok(())
    }

    /// Replace this document's tag set with exactly `tag_ids`. Missing
    /// tag ids that exist in the table become attached; ids not in
    /// the slice get detached.
    ///
    /// This is a *diff*, not a wipe-and-reinsert: links that survive the
    /// update keep their original `applied_at`, only newly-attached links
    /// are stamped with the current time. That preserves a truthful
    /// "recently used" ordering (re-saving an unchanged tag set must not
    /// shuffle every tag to the top of `recently_used_tags`). Duplicate ids
    /// in `tag_ids` are coalesced.
    pub fn set_doc_tags(&mut self, doc_id: i64, tag_ids: &[i64]) -> Result<(), LibraryError> {
        let now = now_unix();
        let desired: std::collections::HashSet<i64> = tag_ids.iter().copied().collect();
        let tx = self.conn.transaction()?;
        {
            // Current links for this doc.
            let mut current = std::collections::HashSet::new();
            {
                let mut stmt =
                    tx.prepare("SELECT tag_id FROM library_doc_tags WHERE doc_id = ?1")?;
                let mut rows = stmt.query(params![doc_id])?;
                while let Some(row) = rows.next()? {
                    current.insert(row.get::<_, i64>(0)?);
                }
            }
            // Detach the links the caller dropped.
            {
                let mut del =
                    tx.prepare("DELETE FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2")?;
                for tid in current.difference(&desired) {
                    del.execute(params![doc_id, tid])?;
                }
            }
            // Attach the new links, stamped now. INSERT OR IGNORE keeps the
            // call idempotent even if the same id appears twice.
            {
                let mut ins = tx.prepare(
                    "INSERT OR IGNORE INTO library_doc_tags (doc_id, tag_id, applied_at)
                     VALUES (?1, ?2, ?3)",
                )?;
                for tid in desired.difference(&current) {
                    ins.execute(params![doc_id, tid, now])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The `limit` most recently *applied* tags, newest first, each tag
    /// listed once by its newest application time. Tags never applied to a
    /// document are excluded (there is nothing recent about them). Legacy
    /// links written before the `applied_at` column existed carry NULL and
    /// sort after every timestamped link, with the link rowid as a stable
    /// final tie-break so ordering is deterministic. Powers the
    /// "Recently used" quick-chips when tagging a document.
    /// v3.44.0 Atlas Recent-Tags.
    pub fn recently_used_tags(&self, limit: usize) -> Result<Vec<TagRecord>, LibraryError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, t.color, t.description
             FROM library_tags t
             INNER JOIN library_doc_tags dt ON dt.tag_id = t.id
             GROUP BY t.id
             ORDER BY MAX(dt.applied_at) IS NULL,
                      MAX(dt.applied_at) DESC,
                      MAX(dt.rowid) DESC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Document count per tag, as `(tag_id, count)` pairs ordered by tag id.
    /// Every tag in the library appears exactly once; a tag attached to zero
    /// documents reports a count of 0 because the LEFT JOIN keeps it (an
    /// INNER JOIN would silently drop unused tags, which is exactly the
    /// residue the rail needs to surface). One GROUP BY round-trip, never a
    /// per-tag query. Powers the muted usage count beside each tag in the rail
    /// and the "sort by most used" ordering. v3.46.0 Atlas Tag-Usage-Counts.
    pub fn tag_usage_counts(&self) -> Result<Vec<(i64, i64)>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, COUNT(dt.doc_id)
             FROM library_tags t
             LEFT JOIN library_doc_tags dt ON dt.tag_id = t.id
             GROUP BY t.id
             ORDER BY t.id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete every tag attached to zero documents in one statement, returning
    /// the number of tag rows removed. This is the cleanup for the residue that
    /// merges and bulk-removes leave behind: a tag whose last document link was
    /// detached still lingers in the rail with a usage count of 0 (the very
    /// thing `tag_usage_counts`' LEFT JOIN surfaces) until something prunes it.
    ///
    /// `NOT EXISTS` against `library_doc_tags` keeps an unused tag and drops it;
    /// a tag carrying even one link is untouched, so documents never lose a tag
    /// they actually wear. One DELETE, never a per-tag query. A library with no
    /// unused tags is a no-op that returns 0. v3.47.0 Atlas Tag-Cleanup.
    pub fn delete_unused_tags(&mut self) -> Result<usize, LibraryError> {
        let removed = self.conn.execute(
            "DELETE FROM library_tags
             WHERE NOT EXISTS (
                 SELECT 1 FROM library_doc_tags dt WHERE dt.tag_id = library_tags.id
             )",
            [],
        )?;
        Ok(removed)
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
        ocr_state: row.get(10)?,
        ocr_output_path: row.get(11)?,
        ocr_error: row.get(12)?,
        notes: row.get(13)?,
        starred: row.get::<_, i64>(14)? != 0,
        tags: Vec::new(),
    })
}

fn tag_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRecord> {
    Ok(TagRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        description: row.get(3)?,
    })
}

/// Maximum length of a tag description, in Unicode scalar values. Picked
/// large enough that a real sentence or two fits ("invoices pending the
/// bookkeeper's monthly review — auto-applied by the email rule") but small
/// enough that the rail tooltip stays glanceable and the DB column stays
/// cheap. Measured in chars (not bytes) so emoji and CJK get a sane budget.
/// v3.51.0 Atlas Tag-Descriptions.
pub(crate) const MAX_TAG_DESCRIPTION_LEN: usize = 500;

/// Whether `d` (assumed already trimmed) fits the persisted-description budget.
pub(crate) fn valid_tag_description(d: &str) -> bool {
    d.chars().count() <= MAX_TAG_DESCRIPTION_LEN
}

/// Maximum length of an override title on `library_documents.title`, in
/// Unicode scalar values. Big enough for a real document name (most paper
/// titles fit comfortably under 200 chars; book chapters and legal exhibits
/// occasionally hit 400) but capped so a runaway paste can't bloat the DB or
/// the doc-card layout. Counted in chars (not bytes) so emoji and CJK get a
/// sane budget. v3.55.0 Atlas Doc-Inspector.
pub(crate) const MAX_DOC_TITLE_LEN: usize = 500;

/// Whether `t` (assumed already trimmed) fits the persisted-title budget.
pub(crate) fn valid_doc_title(t: &str) -> bool {
    t.chars().count() <= MAX_DOC_TITLE_LEN
}

/// Maximum length of the freeform `notes` field on a document row, in Unicode
/// scalar values. Sized for a paragraph or two of provenance context ("Got
/// this from opposing counsel; redaction missing on page 14, see follow-up
/// 2025-09-12") without letting a runaway paste balloon the row. Counted in
/// chars (not bytes) so emoji and CJK get a sane budget.
/// v3.55.0 Atlas Doc-Inspector.
pub(crate) const MAX_DOC_NOTES_LEN: usize = 4000;

/// Whether `n` (assumed already trimmed) fits the persisted-notes budget.
pub(crate) fn valid_doc_notes(n: &str) -> bool {
    n.chars().count() <= MAX_DOC_NOTES_LEN
}

/// Whether `c` is a CSS color value we're willing to persist on a tag row.
///
/// We accept exactly the two shapes the app produces — `#rgb` / `#rgba` /
/// `#rrggbb` / `#rrggbbaa` hex (the swatch palette) and the functional
/// `hsl()/hsla()/rgb()/rgba()` notations (`pastel_for` emits `hsl(...)`).
/// The functional body is restricted to digits, dots, `%`, commas and spaces
/// so a stored value can never carry CSS that breaks out of the property it's
/// dropped into (no `;`, `{`, `url(`, quotes, angle brackets, etc.).
pub(crate) fn valid_tag_color(c: &str) -> bool {
    let c = c.trim();
    if c.is_empty() {
        return false;
    }
    if let Some(hex) = c.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    let lower = c.to_ascii_lowercase();
    for fname in ["hsla(", "rgba(", "hsl(", "rgb("] {
        if let Some(rest) = lower.strip_prefix(fname) {
            let Some(body) = rest.strip_suffix(')') else {
                return false;
            };
            return !body.is_empty()
                && body
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'%' | b',' | b' '));
        }
    }
    false
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
    fn schema_v7_has_search_log_and_dismissed_tables() {
        let db = db();
        let log_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='library_search_log'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(log_count, 1, "library_search_log table missing");

        let dismissed_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='library_suggestion_dismissed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dismissed_count, 1, "library_suggestion_dismissed missing");

        // Schema always migrates to the latest version; the dedicated
        // version test pins the exact number. Here we only assert the v7
        // tables landed.
    }

    #[test]
    fn schema_v8_has_tag_suggestion_dismissed_table() {
        let db = db();
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='library_tag_suggestion_dismissed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "library_tag_suggestion_dismissed table missing");
        // v3.39.0 landed this table at schema v8; the dedicated version test
        // (`schema_version_is_set`) pins the current number — this one only
        // asserts the table exists and migrations ran past v8.
        assert!(db.schema_version().unwrap() >= 8);
    }

    #[test]
    fn schema_v9_has_applied_at_on_doc_tags() {
        let db = db();
        // The migration adds an `applied_at` column to library_doc_tags.
        let has_col: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library_doc_tags') \
                 WHERE name = 'applied_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_col, 1, "library_doc_tags.applied_at column missing");
        assert!(db.schema_version().unwrap() >= 9);
    }

    #[test]
    fn schema_v10_has_saved_views_table() {
        // v3.50.0 Atlas Saved Views — new table for one-click restorable
        // rail filters. Independent of the smart_collections / personal_presets
        // surfaces (a view re-runs a filter; it doesn't own a doc list and
        // doesn't materialize into one). Assert with `>=` not `==` so the
        // next migration doesn't trip an unrelated equality check — the
        // trap that bit the v3.39 -> bulk-tag tick.
        let db = db();
        let table_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='library_saved_views'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 1, "library_saved_views table missing");
        // The four columns the saved_views module expects.
        for col in ["id", "name", "filter_json", "created_at", "sort_order"] {
            let has_col: i64 = db
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('library_saved_views') \
                     WHERE name = ?1",
                    rusqlite::params![col],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(has_col, 1, "library_saved_views.{col} column missing");
        }
        assert!(db.schema_version().unwrap() >= 10);
    }

    #[test]
    fn schema_v11_has_description_on_tags() {
        // v3.51.0 Atlas Tag-Descriptions — adds a nullable `description`
        // column to library_tags so each tag can carry an optional
        // freeform note. Assert with `>=` not `==` so the next migration
        // doesn't trip an unrelated equality check (the trap that bit
        // the v3.39 -> bulk-tag tick).
        let db = db();
        let has_col: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library_tags') \
                 WHERE name = 'description'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_col, 1, "library_tags.description column missing");
        // Pre-v11 tags pick up NULL automatically (no default rewriting).
        assert!(db.schema_version().unwrap() >= 11);
    }

    #[test]
    fn schema_v12_has_ocr_error_on_documents() {
        // v3.52.0 Atlas OCR-Queue — adds a nullable `ocr_error` column
        // to library_documents so the most recent failure reason travels
        // with each row. `>=` not `==` so the next migration doesn't
        // accidentally fail this assert (same equality-trap discipline
        // as the v11 description test).
        let db = db();
        let has_col: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library_documents') \
                 WHERE name = 'ocr_error'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_col, 1, "library_documents.ocr_error column missing");
        assert!(db.schema_version().unwrap() >= 12);
    }

    #[test]
    fn schema_v13_has_notes_on_documents() {
        // v3.55.0 Atlas Doc-Inspector — adds a nullable `notes` column to
        // library_documents so the inspector drawer's freeform note travels
        // with each row. `>=` not `==` so the next migration doesn't
        // accidentally fail this assert.
        let db = db();
        let has_col: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library_documents') \
                 WHERE name = 'notes'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_col, 1, "library_documents.notes column missing");
        assert!(db.schema_version().unwrap() >= 13);
    }

    #[test]
    fn schema_v14_has_starred_on_documents() {
        // v3.55.0 Atlas Doc-Inspector — adds an INTEGER NOT NULL DEFAULT 0
        // `starred` column and a partial index on rows where starred = 1.
        // `>=` not `==` so the next migration doesn't accidentally fail.
        let db = db();
        let has_col: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library_documents') \
                 WHERE name = 'starred'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_col, 1, "library_documents.starred column missing");
        // Partial index lands too — confirms the migration ran in full.
        let has_idx: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'index' AND name = 'idx_documents_starred'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_idx, 1, "idx_documents_starred missing");
        assert!(db.schema_version().unwrap() >= 14);
    }

    #[test]
    fn schema_v15_has_pinned_on_saved_views() {
        // v3.56.0 Atlas Saved-Views-Polish — adds an INTEGER NOT NULL
        // DEFAULT 0 `pinned` column to library_saved_views + the partial
        // index on rows where pinned = 1. `>=` not `==` so the next
        // migration doesn't accidentally fail.
        let db = db();
        let has_col: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library_saved_views') \
                 WHERE name = 'pinned'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_col, 1, "library_saved_views.pinned column missing");
        let has_idx: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'index' AND name = 'idx_saved_views_pinned'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_idx, 1, "idx_saved_views_pinned missing");
        assert!(db.schema_version().unwrap() >= 15);
    }

    #[test]
    fn set_doc_ocr_error_round_trips() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        // Fresh row has NULL ocr_error.
        assert!(d.ocr_error.is_none());

        // Setting a real reason persists.
        db.set_doc_ocr_error(d.id, Some("tesseract not on PATH"))
            .unwrap();
        let d2 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d2.ocr_error.as_deref(), Some("tesseract not on PATH"));

        // Setting None clears it.
        db.set_doc_ocr_error(d.id, None).unwrap();
        let d3 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert!(d3.ocr_error.is_none());

        // Trimmed-empty equivalent to None (no whitespace-only reasons).
        db.set_doc_ocr_error(d.id, Some("real")).unwrap();
        db.set_doc_ocr_error(d.id, Some("   \n\t  ")).unwrap();
        let d4 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert!(
            d4.ocr_error.is_none(),
            "whitespace-only reason should clear the column, got {:?}",
            d4.ocr_error
        );

        // Trims leading/trailing whitespace on real reasons.
        db.set_doc_ocr_error(d.id, Some("  rasterise failed  "))
            .unwrap();
        let d5 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d5.ocr_error.as_deref(), Some("rasterise failed"));
    }

    #[test]
    fn set_doc_ocr_error_preserves_other_doc_columns() {
        // Guard against column drift: writing the error must not regress
        // ocr_state / ocr_output_path / title.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(
                Some(f.id),
                "/tmp/keep.pdf",
                Some("My Doc"),
                "h",
                1,
                1,
                Some(7),
                Some(OCR_STATE_DONE),
            )
            .unwrap();
        db.set_doc_ocr_output_path(d.id, Some("/tmp/keep.ocr.pdf"))
            .unwrap();
        db.set_doc_ocr_error(d.id, Some("transient blip")).unwrap();
        let after = db.find_document_by_path("/tmp/keep.pdf").unwrap().unwrap();
        assert_eq!(after.title.as_deref(), Some("My Doc"));
        assert_eq!(after.ocr_state, OCR_STATE_DONE);
        assert_eq!(after.ocr_output_path.as_deref(), Some("/tmp/keep.ocr.pdf"));
        assert_eq!(after.pages, Some(7));
        assert_eq!(after.ocr_error.as_deref(), Some("transient blip"));
    }

    #[test]
    fn upsert_existing_doc_preserves_ocr_error() {
        // The hot re-upsert path mustn't smash a persisted error string.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.set_doc_ocr_error(d.id, Some("first failure")).unwrap();
        // Re-upsert (scanner re-spotting the same file) must not drop the column.
        let _ = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let after = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(after.ocr_error.as_deref(), Some("first failure"));
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
                None,
            )
            .unwrap();
        assert!(d.id > 0);
        assert_eq!(d.path, "/tmp/a.pdf");
        assert_eq!(d.hash, "deadbeef");
        assert_eq!(d.size_bytes, 100);
        assert_eq!(d.mtime_ns, 42);
        assert_eq!(d.pages, Some(3));
        assert_eq!(d.title.as_deref(), Some("Title"));
        assert_eq!(d.ocr_state, OCR_STATE_UNKNOWN);
        assert!(d.ocr_output_path.is_none());
    }

    #[test]
    fn upsert_document_updates_existing_by_path() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d1 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "v1", 100, 1, Some(3), None)
            .unwrap();
        let d2 = db
            .upsert_document(
                Some(f.id),
                "/tmp/a.pdf",
                Some("New"),
                "v2",
                200,
                2,
                Some(5),
                None,
            )
            .unwrap();
        assert_eq!(d1.id, d2.id, "upsert should keep the same id");
        assert_eq!(d2.hash, "v2");
        assert_eq!(d2.size_bytes, 200);
        assert_eq!(d2.pages, Some(5));
        assert_eq!(d2.title.as_deref(), Some("New"));
    }

    #[test]
    fn set_doc_title_updates_and_returns_row_with_tags() {
        // v3.55.0 Atlas Doc-Inspector — verifies the setter returns a
        // refreshed DocumentRecord with tags eager-loaded so the UI can
        // splice the card without a list_documents round-trip.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/x.pdf", None, "h", 10, 1, Some(2), None)
            .unwrap();
        let t = db.add_tag("research", None).unwrap();
        db.set_doc_tags(d.id, &[t.id]).unwrap();
        let updated = db.set_doc_title(d.id, Some("Quarterly Filing")).unwrap();
        assert_eq!(updated.id, d.id);
        assert_eq!(updated.title.as_deref(), Some("Quarterly Filing"));
        // Eager-loaded tags survive the round-trip.
        assert_eq!(updated.tags.len(), 1);
        assert_eq!(updated.tags[0].name, "research");
        // Persisted: a fresh path lookup also sees the new title.
        let fresh = db.find_document_by_path("/tmp/x.pdf").unwrap().unwrap();
        assert_eq!(fresh.title.as_deref(), Some("Quarterly Filing"));
        // Other columns are untouched by a pure title update.
        assert_eq!(fresh.hash, "h");
        assert_eq!(fresh.size_bytes, 10);
        assert_eq!(fresh.pages, Some(2));
    }

    #[test]
    fn set_doc_title_trims_whitespace() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/y.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let updated = db
            .set_doc_title(d.id, Some("   Important Report  \n"))
            .unwrap();
        assert_eq!(updated.title.as_deref(), Some("Important Report"));
    }

    #[test]
    fn set_doc_title_empty_or_none_clears_to_null() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(
                Some(f.id),
                "/tmp/z.pdf",
                Some("Seed Title"),
                "h",
                1,
                1,
                None,
                None,
            )
            .unwrap();
        // Empty / whitespace strings collapse to NULL so the basename
        // fallback resumes — never persist "real but empty" trash.
        for clearer in ["", "   ", "\n\t  "] {
            db.set_doc_title(d.id, Some("Seed Title")).unwrap();
            let cleared = db.set_doc_title(d.id, Some(clearer)).unwrap();
            assert!(cleared.title.is_none(), "{clearer:?} should clear");
        }
        // None explicitly clears too.
        db.set_doc_title(d.id, Some("Seed Title")).unwrap();
        let cleared = db.set_doc_title(d.id, None).unwrap();
        assert!(cleared.title.is_none());
    }

    #[test]
    fn set_doc_title_rejects_oversized_text_and_preserves_prior() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(
                Some(f.id),
                "/tmp/big.pdf",
                Some("Original"),
                "h",
                1,
                1,
                None,
                None,
            )
            .unwrap();
        let oversize = "x".repeat(MAX_DOC_TITLE_LEN + 1);
        assert!(
            db.set_doc_title(d.id, Some(&oversize)).is_err(),
            "{} chars should be rejected",
            MAX_DOC_TITLE_LEN + 1
        );
        // Length check runs BEFORE the UPDATE so the prior title survives.
        let fresh = db.find_document_by_path("/tmp/big.pdf").unwrap().unwrap();
        assert_eq!(fresh.title.as_deref(), Some("Original"));
        // Exact-cap input is accepted.
        let exact = "y".repeat(MAX_DOC_TITLE_LEN);
        let accepted = db.set_doc_title(d.id, Some(&exact)).unwrap();
        assert_eq!(accepted.title.as_deref(), Some(exact.as_str()));
    }

    #[test]
    fn set_doc_title_unknown_id_errors() {
        let mut db = db();
        assert!(
            db.set_doc_title(424242, Some("nope")).is_err(),
            "unknown id should error"
        );
    }

    #[test]
    fn set_doc_notes_updates_and_returns_row_with_tags() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/n.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t = db.add_tag("urgent", None).unwrap();
        db.set_doc_tags(d.id, &[t.id]).unwrap();
        // Default value is None — the column is nullable and pre-v13 rows
        // silently picked up NULL.
        assert!(d.notes.is_none());
        let updated = db
            .set_doc_notes(d.id, Some("Got this from opposing counsel"))
            .unwrap();
        assert_eq!(
            updated.notes.as_deref(),
            Some("Got this from opposing counsel")
        );
        // Tags survive the round-trip (eager-loaded).
        assert_eq!(updated.tags.len(), 1);
        assert_eq!(updated.tags[0].name, "urgent");
        // Persisted: a fresh path lookup also sees the new notes.
        let fresh = db.find_document_by_path("/tmp/n.pdf").unwrap().unwrap();
        assert_eq!(
            fresh.notes.as_deref(),
            Some("Got this from opposing counsel")
        );
        // Other columns are untouched by a pure notes update.
        assert_eq!(fresh.hash, "h");
    }

    #[test]
    fn set_doc_notes_trims_whitespace() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/n.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let updated = db
            .set_doc_notes(d.id, Some("   Inbound from email rule.\n  "))
            .unwrap();
        assert_eq!(updated.notes.as_deref(), Some("Inbound from email rule."));
    }

    #[test]
    fn set_doc_notes_empty_or_none_clears_to_null() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/n.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        for clearer in ["", "   ", "\n\t  "] {
            db.set_doc_notes(d.id, Some("seed note")).unwrap();
            let cleared = db.set_doc_notes(d.id, Some(clearer)).unwrap();
            assert!(cleared.notes.is_none(), "{clearer:?} should clear");
        }
        db.set_doc_notes(d.id, Some("seed note")).unwrap();
        let cleared = db.set_doc_notes(d.id, None).unwrap();
        assert!(cleared.notes.is_none());
    }

    #[test]
    fn set_doc_notes_rejects_oversized_text_and_preserves_prior() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/n.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.set_doc_notes(d.id, Some("Original note.")).unwrap();
        let oversize = "x".repeat(MAX_DOC_NOTES_LEN + 1);
        assert!(
            db.set_doc_notes(d.id, Some(&oversize)).is_err(),
            "{} chars should be rejected",
            MAX_DOC_NOTES_LEN + 1
        );
        // Length check runs BEFORE the UPDATE so the prior notes survive.
        let fresh = db.find_document_by_path("/tmp/n.pdf").unwrap().unwrap();
        assert_eq!(fresh.notes.as_deref(), Some("Original note."));
        // Exact-cap input is accepted (boundary test).
        let exact = "y".repeat(MAX_DOC_NOTES_LEN);
        let accepted = db.set_doc_notes(d.id, Some(&exact)).unwrap();
        assert_eq!(accepted.notes.as_deref(), Some(exact.as_str()));
    }

    #[test]
    fn set_doc_notes_unknown_id_errors() {
        let mut db = db();
        assert!(
            db.set_doc_notes(424242, Some("nope")).is_err(),
            "unknown id should error"
        );
    }

    #[test]
    fn set_doc_starred_toggles_and_returns_row_with_tags() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/s.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t = db.add_tag("priority", None).unwrap();
        db.set_doc_tags(d.id, &[t.id]).unwrap();
        // Default value is false — pre-v14 rows pick up `starred = 0`.
        assert!(!d.starred);
        let starred = db.set_doc_starred(d.id, true).unwrap();
        assert!(starred.starred);
        // Tags survive the round-trip.
        assert_eq!(starred.tags.len(), 1);
        assert_eq!(starred.tags[0].name, "priority");
        // Persisted: a fresh path lookup also sees the new flag.
        let fresh = db.find_document_by_path("/tmp/s.pdf").unwrap().unwrap();
        assert!(fresh.starred);
        // Other columns are untouched by a pure star update.
        assert_eq!(fresh.hash, "h");
        // Toggle back off works too.
        let cleared = db.set_doc_starred(d.id, false).unwrap();
        assert!(!cleared.starred);
        let fresh = db.find_document_by_path("/tmp/s.pdf").unwrap().unwrap();
        assert!(!fresh.starred);
    }

    #[test]
    fn set_doc_starred_is_idempotent() {
        // Setting an already-starred doc to starred again is a no-op UPDATE
        // that still returns the row (SQLite reports rows matched, not rows
        // whose value changed).
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/i.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.set_doc_starred(d.id, true).unwrap();
        let again = db.set_doc_starred(d.id, true).unwrap();
        assert!(again.starred);
        // Same for unstarred -> unstarred.
        db.set_doc_starred(d.id, false).unwrap();
        let again = db.set_doc_starred(d.id, false).unwrap();
        assert!(!again.starred);
    }

    #[test]
    fn set_doc_starred_unknown_id_errors() {
        let mut db = db();
        assert!(
            db.set_doc_starred(424242, true).is_err(),
            "unknown id should error"
        );
    }

    #[test]
    fn upsert_existing_doc_preserves_starred() {
        // The scanner's re-scan pass calls upsert_document on every existing
        // path; a star set by the user must NOT get wiped by a scan.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/p.pdf", None, "v1", 100, 1, Some(3), None)
            .unwrap();
        db.set_doc_starred(d.id, true).unwrap();
        // Re-upsert with new hash / size (typical post-edit scan).
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/p.pdf", None, "v2", 200, 2, Some(5), None)
            .unwrap();
        assert!(d2.starred, "starred flag should survive a re-upsert");
    }

    #[test]
    fn cascade_delete_folder_drops_documents() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        db.upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.remove_folder(f.id).unwrap();
        assert!(db.find_document_by_path("/tmp/a.pdf").unwrap().is_none());
    }

    #[test]
    fn touch_document_updates_last_seen() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
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
    fn set_tag_color_updates_and_returns_row() {
        let mut db = db();
        let t = db.add_tag("invoice", Some("#ff7a59")).unwrap();
        let updated = db.set_tag_color(t.id, Some("#6ab7ff")).unwrap();
        assert_eq!(updated.id, t.id);
        assert_eq!(updated.name, "invoice");
        assert_eq!(updated.color.as_deref(), Some("#6ab7ff"));
        // Persisted: a fresh read sees the new color.
        assert_eq!(
            db.find_tag_by_id(t.id).unwrap().unwrap().color.as_deref(),
            Some("#6ab7ff")
        );
    }

    #[test]
    fn set_tag_color_trims_whitespace() {
        let mut db = db();
        let t = db.add_tag("draft", None).unwrap();
        let updated = db.set_tag_color(t.id, Some("  #7ee787  ")).unwrap();
        assert_eq!(updated.color.as_deref(), Some("#7ee787"));
    }

    #[test]
    fn set_tag_color_none_clears() {
        let mut db = db();
        let t = db.add_tag("done", Some("#f5c518")).unwrap();
        let cleared = db.set_tag_color(t.id, None).unwrap();
        assert_eq!(cleared.color, None);
    }

    #[test]
    fn set_tag_color_accepts_hsl_from_pastel_for() {
        let mut db = db();
        let t = db.add_tag("research", None).unwrap();
        // pastel_for emits e.g. "hsl(123, 60%, 80%)" — must round-trip.
        let updated = db.set_tag_color(t.id, Some("hsl(123, 60%, 80%)")).unwrap();
        assert_eq!(updated.color.as_deref(), Some("hsl(123, 60%, 80%)"));
    }

    #[test]
    fn set_tag_color_rejects_invalid_color() {
        let mut db = db();
        let t = db.add_tag("contract", Some("#abc")).unwrap();
        for bad in [
            "red",                      // bare keyword, not in our accepted set
            "#12",                      // wrong hex length
            "#gggggg",                  // non-hex digits
            "url(x)",                   // function we don't allow
            "hsl(120, 60%, 80%); evil", // CSS injection attempt
            "hsl(120, 60%, 80%",        // missing close paren
            "",                         // empty
        ] {
            assert!(
                db.set_tag_color(t.id, Some(bad)).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
        // The row's original color is untouched after a rejected update.
        assert_eq!(
            db.find_tag_by_id(t.id).unwrap().unwrap().color.as_deref(),
            Some("#abc")
        );
    }

    #[test]
    fn set_tag_color_unknown_id_errors() {
        let mut db = db();
        assert!(db.set_tag_color(9999, Some("#ff7a59")).is_err());
    }

    // ---- set_tag_description (v3.51.0 Atlas Tag-Descriptions) ----

    #[test]
    fn add_tag_starts_with_no_description() {
        let mut db = db();
        let t = db.add_tag("invoice", None).unwrap();
        // Fresh tags carry NULL — descriptions are an explicit opt-in.
        assert_eq!(t.description, None);
        assert_eq!(db.find_tag_by_id(t.id).unwrap().unwrap().description, None);
    }

    #[test]
    fn set_tag_description_updates_and_returns_row() {
        let mut db = db();
        let t = db.add_tag("invoice", None).unwrap();
        let updated = db
            .set_tag_description(t.id, Some("Bills waiting on bookkeeper"))
            .unwrap();
        assert_eq!(updated.id, t.id);
        assert_eq!(updated.name, "invoice");
        assert_eq!(
            updated.description.as_deref(),
            Some("Bills waiting on bookkeeper")
        );
        // Persisted: a fresh read sees the new description.
        let fresh = db.find_tag_by_id(t.id).unwrap().unwrap();
        assert_eq!(
            fresh.description.as_deref(),
            Some("Bills waiting on bookkeeper")
        );
        // Color and name are untouched by a description update.
        assert_eq!(fresh.color, None);
        assert_eq!(fresh.name, "invoice");
    }

    #[test]
    fn set_tag_description_trims_whitespace() {
        let mut db = db();
        let t = db.add_tag("draft", None).unwrap();
        let updated = db
            .set_tag_description(t.id, Some("   First draft only.  \n"))
            .unwrap();
        // Both ends trimmed; persisted text never carries trailing whitespace.
        assert_eq!(updated.description.as_deref(), Some("First draft only."));
    }

    #[test]
    fn set_tag_description_empty_trims_to_null() {
        let mut db = db();
        let t = db.add_tag("note", None).unwrap();
        // Seed a description.
        db.set_tag_description(t.id, Some("something")).unwrap();
        // An empty or whitespace-only string is equivalent to passing None —
        // it clears the column back to NULL rather than persisting "".
        for clearer in ["", "   ", "\n\t  "] {
            db.set_tag_description(t.id, Some("something")).unwrap();
            let cleared = db.set_tag_description(t.id, Some(clearer)).unwrap();
            assert_eq!(cleared.description, None, "{clearer:?} should clear");
        }
    }

    #[test]
    fn set_tag_description_none_clears() {
        let mut db = db();
        let t = db.add_tag("done", None).unwrap();
        db.set_tag_description(t.id, Some("ready to archive"))
            .unwrap();
        let cleared = db.set_tag_description(t.id, None).unwrap();
        assert_eq!(cleared.description, None);
    }

    #[test]
    fn set_tag_description_accepts_max_length() {
        let mut db = db();
        let t = db.add_tag("research", None).unwrap();
        let exact = "x".repeat(MAX_TAG_DESCRIPTION_LEN);
        let updated = db.set_tag_description(t.id, Some(&exact)).unwrap();
        assert_eq!(updated.description.as_deref(), Some(exact.as_str()));
    }

    #[test]
    fn set_tag_description_rejects_oversized_text() {
        let mut db = db();
        let t = db.add_tag("contract", None).unwrap();
        db.set_tag_description(t.id, Some("original")).unwrap();
        let oversize = "x".repeat(MAX_TAG_DESCRIPTION_LEN + 1);
        assert!(
            db.set_tag_description(t.id, Some(&oversize)).is_err(),
            "{} chars should be rejected",
            MAX_TAG_DESCRIPTION_LEN + 1
        );
        // The row's original description is untouched after a rejected update.
        assert_eq!(
            db.find_tag_by_id(t.id)
                .unwrap()
                .unwrap()
                .description
                .as_deref(),
            Some("original")
        );
    }

    #[test]
    fn set_tag_description_counts_chars_not_bytes() {
        // The length cap is in Unicode scalars, not bytes — emoji and CJK
        // should each count as ONE toward the budget, not 3-4 bytes. A
        // string of MAX scalars (where each is multi-byte) must still fit.
        let mut db = db();
        let t = db.add_tag("emoji", None).unwrap();
        let multibyte = "漢".repeat(MAX_TAG_DESCRIPTION_LEN); // 3 bytes each
        assert!(multibyte.len() > MAX_TAG_DESCRIPTION_LEN);
        let updated = db.set_tag_description(t.id, Some(&multibyte)).unwrap();
        assert_eq!(updated.description.as_deref(), Some(multibyte.as_str()));
    }

    #[test]
    fn set_tag_description_unknown_id_errors() {
        let mut db = db();
        assert!(db.set_tag_description(9999, Some("missing tag")).is_err());
    }

    #[test]
    fn set_tag_description_persists_across_list_and_recently_used() {
        // The widened SELECT in list_tags and recently_used_tags must
        // surface the description so the rail tooltip and the recent-chip
        // tooltip both pick it up. This catches a column drift if the
        // SQL widening regresses.
        let mut db = db();
        let folder = db.add_folder("/tmp/desc").unwrap();
        let doc = db
            .upsert_document(
                Some(folder.id),
                "/tmp/desc/p.pdf",
                Some("p"),
                "hash1",
                1,
                1,
                None,
                None,
            )
            .unwrap();
        let t = db.add_tag("priority", None).unwrap();
        db.set_doc_tags(doc.id, &[t.id]).unwrap();
        db.set_tag_description(t.id, Some("URGENT")).unwrap();

        // list_tags carries it.
        let listed = db.list_tags().unwrap();
        let from_list = listed.iter().find(|x| x.id == t.id).unwrap();
        assert_eq!(from_list.description.as_deref(), Some("URGENT"));

        // recently_used_tags carries it.
        let recent = db.recently_used_tags(5).unwrap();
        let from_recent = recent.iter().find(|x| x.id == t.id).unwrap();
        assert_eq!(from_recent.description.as_deref(), Some("URGENT"));

        // tags_for_document carries it.
        let on_doc = db.tags_for_document(doc.id).unwrap();
        assert_eq!(on_doc[0].description.as_deref(), Some("URGENT"));
    }

    #[test]
    fn rename_tag_preserves_description() {
        let mut db = db();
        let t = db.add_tag("reserch", None).unwrap();
        db.set_tag_description(t.id, Some("Papers I'm reading"))
            .unwrap();
        let renamed = db.rename_tag(t.id, "research").unwrap();
        // Renaming is a pure UPDATE on (name); description must survive it.
        assert_eq!(renamed.description.as_deref(), Some("Papers I'm reading"));
    }

    #[test]
    fn set_tag_color_preserves_description() {
        let mut db = db();
        let t = db.add_tag("priority", None).unwrap();
        db.set_tag_description(t.id, Some("Drop everything"))
            .unwrap();
        let updated = db.set_tag_color(t.id, Some("#ff7a59")).unwrap();
        // Setting a color must NOT clobber the description column.
        assert_eq!(updated.description.as_deref(), Some("Drop everything"));
    }

    #[test]
    fn rename_tag_updates_and_returns_row() {
        let mut db = db();
        let t = db.add_tag("reserch", Some("#6ab7ff")).unwrap();
        let renamed = db.rename_tag(t.id, "research").unwrap();
        assert_eq!(renamed.id, t.id, "id is preserved across a rename");
        assert_eq!(renamed.name, "research");
        // Color is untouched by a rename.
        assert_eq!(renamed.color.as_deref(), Some("#6ab7ff"));
        // Persisted: a fresh lookup by the new name resolves to the same row,
        // and the old name no longer exists.
        assert_eq!(db.find_tag_by_name("research").unwrap().unwrap().id, t.id);
        assert!(db.find_tag_by_name("reserch").unwrap().is_none());
    }

    #[test]
    fn rename_tag_carries_document_links() {
        // The whole point of "rename everywhere": documents wear the tag via
        // tag_id, so a rename must leave every attachment intact.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t = db.add_tag("draught", None).unwrap();
        db.set_doc_tags(d.id, &[t.id]).unwrap();

        let renamed = db.rename_tag(t.id, "draft").unwrap();
        assert_eq!(renamed.name, "draft");

        let attached = db.tags_for_document(d.id).unwrap();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0].id, t.id);
        assert_eq!(attached[0].name, "draft", "the doc sees the new name");
    }

    #[test]
    fn rename_tag_trims_whitespace() {
        let mut db = db();
        let t = db.add_tag("temp", None).unwrap();
        let renamed = db.rename_tag(t.id, "  invoices  ").unwrap();
        assert_eq!(renamed.name, "invoices");
    }

    #[test]
    fn rename_tag_to_same_name_is_noop() {
        let mut db = db();
        let t = db.add_tag("research", Some("#abc")).unwrap();
        let renamed = db.rename_tag(t.id, "research").unwrap();
        assert_eq!(renamed.id, t.id);
        assert_eq!(renamed.name, "research");
        assert_eq!(renamed.color.as_deref(), Some("#abc"));
        // Trimming still applies to the no-op path.
        let trimmed = db.rename_tag(t.id, "  research  ").unwrap();
        assert_eq!(trimmed.name, "research");
    }

    #[test]
    fn rename_tag_allows_pure_case_change() {
        // BINARY collation makes "research" and "Research" distinct names, so a
        // case-only rename is a real rename, not a self-collision.
        let mut db = db();
        let t = db.add_tag("research", None).unwrap();
        let renamed = db.rename_tag(t.id, "Research").unwrap();
        assert_eq!(renamed.name, "Research");
        assert!(db.find_tag_by_name("research").unwrap().is_none());
        assert_eq!(db.find_tag_by_name("Research").unwrap().unwrap().id, t.id);
    }

    #[test]
    fn rename_tag_rejects_collision_with_other_tag() {
        let mut db = db();
        let a = db.add_tag("research", None).unwrap();
        let b = db.add_tag("draft", None).unwrap();
        // Renaming b onto a's name must fail and leave both rows untouched.
        assert!(db.rename_tag(b.id, "research").is_err());
        assert_eq!(db.find_tag_by_id(a.id).unwrap().unwrap().name, "research");
        assert_eq!(db.find_tag_by_id(b.id).unwrap().unwrap().name, "draft");
        // Still exactly two tags — no merge, no orphan.
        assert_eq!(db.list_tags().unwrap().len(), 2);
    }

    #[test]
    fn rename_tag_rejects_empty_name() {
        let mut db = db();
        let t = db.add_tag("keep", None).unwrap();
        for bad in ["", "   ", "\t\n"] {
            assert!(
                db.rename_tag(t.id, bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
        // The original name survives every rejected rename.
        assert_eq!(db.find_tag_by_id(t.id).unwrap().unwrap().name, "keep");
    }

    #[test]
    fn rename_tag_unknown_id_errors() {
        let mut db = db();
        assert!(db.rename_tag(9999, "whatever").is_err());
    }

    #[test]
    fn merge_tags_deletes_source_and_returns_target() {
        let mut db = db();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("machine-learning", Some("#6ab7ff")).unwrap();
        let survivor = db.merge_tags(source.id, target.id).unwrap();
        // The returned row is the surviving target, color intact.
        assert_eq!(survivor.id, target.id);
        assert_eq!(survivor.name, "machine-learning");
        assert_eq!(survivor.color.as_deref(), Some("#6ab7ff"));
        // The source tag row is gone; only the target remains.
        assert!(db.find_tag_by_id(source.id).unwrap().is_none());
        assert_eq!(db.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn merge_tags_repoints_source_only_docs_to_target() {
        // A document that wore ONLY the source tag must end up wearing the
        // target tag after the merge.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        db.set_doc_tags(d.id, &[source.id]).unwrap();

        db.merge_tags(source.id, target.id).unwrap();
        let attached = db.tags_for_document(d.id).unwrap();
        assert_eq!(attached.len(), 1, "doc carries exactly one tag");
        assert_eq!(attached[0].id, target.id, "and it is the target");
    }

    #[test]
    fn merge_tags_coalesces_docs_carrying_both_into_one_link() {
        // A document that wore BOTH tags must end up with a single target
        // link, never a duplicate — the (doc, tag) primary key forbids it and
        // the merge must respect that.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        let other = db.add_tag("keep", None).unwrap();
        db.set_doc_tags(d.id, &[source.id, target.id, other.id])
            .unwrap();

        db.merge_tags(source.id, target.id).unwrap();
        let attached = db.tags_for_document(d.id).unwrap();
        let ids: std::collections::HashSet<i64> = attached.iter().map(|t| t.id).collect();
        // Exactly target + the untouched third tag — no source, no duplicate.
        assert_eq!(attached.len(), 2);
        assert!(ids.contains(&target.id));
        assert!(ids.contains(&other.id));
        assert!(!ids.contains(&source.id));
    }

    #[test]
    fn merge_tags_keeps_newest_applied_at_when_doc_had_both() {
        // When a doc carried both tags with different stamps, the surviving
        // target link must keep the NEWER of the two so recent-order survives.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        db.set_doc_tags(d.id, &[source.id, target.id]).unwrap();

        // Case A: source is newer than target → target link lifts to source's.
        stamp(&db, d.id, source.id, 900);
        stamp(&db, d.id, target.id, 100);
        db.merge_tags(source.id, target.id).unwrap();
        let at: i64 = db
            .conn()
            .query_row(
                "SELECT applied_at FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2",
                params![d.id, target.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(at, 900, "merged link keeps the newer (source) stamp");
    }

    #[test]
    fn merge_tags_keeps_target_stamp_when_target_is_newer() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        db.set_doc_tags(d.id, &[source.id, target.id]).unwrap();
        stamp(&db, d.id, source.id, 100);
        stamp(&db, d.id, target.id, 900); // target newer
        db.merge_tags(source.id, target.id).unwrap();
        let at: i64 = db
            .conn()
            .query_row(
                "SELECT applied_at FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2",
                params![d.id, target.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(at, 900, "merged link keeps the newer (target) stamp");
    }

    #[test]
    fn merge_tags_real_stamp_beats_legacy_null_either_side() {
        // A real timestamp must always win over a legacy NULL, whichever side
        // carries it — this is the NULL-aware max the coalesce pair guarantees.
        for source_null in [true, false] {
            let mut db = db();
            let f = db.add_folder("/tmp").unwrap();
            let d = db
                .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
                .unwrap();
            let source = db.add_tag("ml", None).unwrap();
            let target = db.add_tag("ai", None).unwrap();
            db.set_doc_tags(d.id, &[source.id, target.id]).unwrap();
            // NULL one side, a real 500 on the other.
            let (null_tag, real_tag) = if source_null {
                (source.id, target.id)
            } else {
                (target.id, source.id)
            };
            db.conn()
                .execute(
                    "UPDATE library_doc_tags SET applied_at = NULL WHERE doc_id = ?1 AND tag_id = ?2",
                    params![d.id, null_tag],
                )
                .unwrap();
            stamp(&db, d.id, real_tag, 500);

            db.merge_tags(source.id, target.id).unwrap();
            let at: Option<i64> = db
                .conn()
                .query_row(
                    "SELECT applied_at FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2",
                    params![d.id, target.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                at,
                Some(500),
                "real stamp beats NULL (source_null={source_null})"
            );
        }
    }

    #[test]
    fn merge_tags_repointed_link_keeps_its_own_stamp() {
        // A source-only link is re-pointed, not restamped: its applied_at must
        // carry over unchanged so the merged tag's recency is truthful.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        db.set_doc_tags(d.id, &[source.id]).unwrap();
        stamp(&db, d.id, source.id, 1234);

        db.merge_tags(source.id, target.id).unwrap();
        let at: i64 = db
            .conn()
            .query_row(
                "SELECT applied_at FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2",
                params![d.id, target.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(at, 1234, "re-pointed link keeps its original stamp");
    }

    #[test]
    fn merge_tags_preserves_recently_used_order() {
        // End-to-end: after folding `ml` into `ai`, `ai` should rank by the
        // newest application it inherited, ahead of an untouched older tag.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        let older = db.add_tag("older", None).unwrap();
        db.set_doc_tags(d.id, &[source.id, target.id, older.id])
            .unwrap();
        stamp(&db, d.id, source.id, 800); // source is the most recent use
        stamp(&db, d.id, target.id, 200);
        stamp(&db, d.id, older.id, 400);

        db.merge_tags(source.id, target.id).unwrap();
        let recent = db.recently_used_tags(10).unwrap();
        let names: Vec<&str> = recent.iter().map(|t| t.name.as_str()).collect();
        // ai inherited source's 800, so it ranks above older (400).
        assert_eq!(names, vec!["ai", "older"]);
    }

    #[test]
    fn merge_tags_into_self_errors_and_changes_nothing() {
        let mut db = db();
        let t = db.add_tag("ml", None).unwrap();
        assert!(db.merge_tags(t.id, t.id).is_err());
        // The tag is untouched.
        assert!(db.find_tag_by_id(t.id).unwrap().is_some());
        assert_eq!(db.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn merge_tags_unknown_id_errors_and_leaves_rows_untouched() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let real = db.add_tag("real", None).unwrap();
        db.set_doc_tags(d.id, &[real.id]).unwrap();
        // Unknown source, and unknown target — both must error.
        assert!(db.merge_tags(9999, real.id).is_err());
        assert!(db.merge_tags(real.id, 9999).is_err());
        // The real tag and its single doc link survive every rejected merge.
        assert!(db.find_tag_by_id(real.id).unwrap().is_some());
        assert_eq!(db.tags_for_document(d.id).unwrap().len(), 1);
        assert_eq!(db.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn merge_tags_across_multiple_docs() {
        // The fold spans every document, mixing source-only, target-only and
        // both-carrying docs in one call.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let only_src = db
            .upsert_document(Some(f.id), "/tmp/s.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let only_tgt = db
            .upsert_document(Some(f.id), "/tmp/t.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let both = db
            .upsert_document(Some(f.id), "/tmp/b.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        db.set_doc_tags(only_src.id, &[source.id]).unwrap();
        db.set_doc_tags(only_tgt.id, &[target.id]).unwrap();
        db.set_doc_tags(both.id, &[source.id, target.id]).unwrap();

        db.merge_tags(source.id, target.id).unwrap();
        // Every doc wears exactly the target now.
        for doc in [only_src.id, only_tgt.id, both.id] {
            let attached = db.tags_for_document(doc).unwrap();
            assert_eq!(attached.len(), 1, "doc {doc} has one tag");
            assert_eq!(attached[0].id, target.id);
        }
        // No orphaned source links survive anywhere.
        let leftover: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM library_doc_tags WHERE tag_id = ?1",
                params![source.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0, "no source links remain after merge");
    }

    #[test]
    fn merge_tags_with_no_documents_just_drops_source() {
        // Merging two tags that aren't on any document is still valid — it just
        // deletes the now-redundant source row.
        let mut db = db();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        let survivor = db.merge_tags(source.id, target.id).unwrap();
        assert_eq!(survivor.id, target.id);
        assert!(db.find_tag_by_id(source.id).unwrap().is_none());
    }

    #[test]
    fn tag_usage_counts_counts_documents_per_tag() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d1 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h1", 1, 1, None, None)
            .unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/b.pdf", None, "h2", 1, 1, None, None)
            .unwrap();
        let d3 = db
            .upsert_document(Some(f.id), "/tmp/c.pdf", None, "h3", 1, 1, None, None)
            .unwrap();
        let popular = db.add_tag("popular", None).unwrap();
        let rare = db.add_tag("rare", None).unwrap();
        // popular on all three docs, rare on just one.
        db.set_doc_tags(d1.id, &[popular.id, rare.id]).unwrap();
        db.set_doc_tags(d2.id, &[popular.id]).unwrap();
        db.set_doc_tags(d3.id, &[popular.id]).unwrap();

        let counts: std::collections::HashMap<i64, i64> =
            db.tag_usage_counts().unwrap().into_iter().collect();
        assert_eq!(counts.get(&popular.id), Some(&3));
        assert_eq!(counts.get(&rare.id), Some(&1));
    }

    #[test]
    fn tag_usage_counts_reports_zero_for_unused_tags() {
        // A tag attached to no document must still appear, with a count of 0 —
        // a LEFT JOIN keeps it where an INNER JOIN would drop it. This is what
        // lets the rail show "0" and what the unused-tag cleanup builds on.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let used = db.add_tag("used", None).unwrap();
        let unused = db.add_tag("unused", None).unwrap();
        db.set_doc_tags(d.id, &[used.id]).unwrap();

        let counts: std::collections::HashMap<i64, i64> =
            db.tag_usage_counts().unwrap().into_iter().collect();
        // Both tags present; the never-applied one reports 0, not missing.
        assert_eq!(counts.len(), 2);
        assert_eq!(counts.get(&used.id), Some(&1));
        assert_eq!(counts.get(&unused.id), Some(&0));
    }

    #[test]
    fn tag_usage_counts_lists_every_tag_once_ordered_by_id() {
        let mut db = db();
        let a = db.add_tag("a", None).unwrap();
        let b = db.add_tag("b", None).unwrap();
        let c = db.add_tag("c", None).unwrap();
        let rows = db.tag_usage_counts().unwrap();
        let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![a.id, b.id, c.id], "one row per tag, id-ordered");
        assert!(rows.iter().all(|(_, n)| *n == 0));
    }

    #[test]
    fn tag_usage_counts_is_empty_with_no_tags() {
        let db = db();
        assert!(db.tag_usage_counts().unwrap().is_empty());
    }

    #[test]
    fn tag_usage_counts_reflects_bulk_apply_and_remove() {
        // Counts must track bulk operations, not just single set_doc_tags.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d1 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h1", 1, 1, None, None)
            .unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/b.pdf", None, "h2", 1, 1, None, None)
            .unwrap();
        let tag = db.add_tag("topic", None).unwrap();

        crate::pdf::library::bulk_tag::apply_tag_to_docs(&mut db, "topic", &[d1.id, d2.id])
            .unwrap();
        let after_apply: std::collections::HashMap<i64, i64> =
            db.tag_usage_counts().unwrap().into_iter().collect();
        assert_eq!(after_apply.get(&tag.id), Some(&2));

        crate::pdf::library::bulk_tag::remove_tag_from_docs(&mut db, tag.id, &[d1.id]).unwrap();
        let after_remove: std::collections::HashMap<i64, i64> =
            db.tag_usage_counts().unwrap().into_iter().collect();
        assert_eq!(after_remove.get(&tag.id), Some(&1));
    }

    #[test]
    fn tag_usage_counts_reflects_merge() {
        // After folding source into target, the surviving target's count is the
        // number of DISTINCT docs that wore either tag (no double-counting the
        // doc that wore both), and the gone source is no longer reported.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d1 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h1", 1, 1, None, None)
            .unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/b.pdf", None, "h2", 1, 1, None, None)
            .unwrap();
        let d3 = db
            .upsert_document(Some(f.id), "/tmp/c.pdf", None, "h3", 1, 1, None, None)
            .unwrap();
        let source = db.add_tag("ml", None).unwrap();
        let target = db.add_tag("ai", None).unwrap();
        // d1 wears both, d2 source-only, d3 target-only → union is 3 docs.
        db.set_doc_tags(d1.id, &[source.id, target.id]).unwrap();
        db.set_doc_tags(d2.id, &[source.id]).unwrap();
        db.set_doc_tags(d3.id, &[target.id]).unwrap();

        db.merge_tags(source.id, target.id).unwrap();
        let counts: std::collections::HashMap<i64, i64> =
            db.tag_usage_counts().unwrap().into_iter().collect();
        assert_eq!(counts.get(&target.id), Some(&3), "distinct union, no dup");
        assert_eq!(counts.get(&source.id), None, "source tag is gone");
    }

    #[test]
    fn delete_unused_tags_removes_only_unused() {
        // The cleanup must drop tags on zero documents and keep the rest.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let used = db.add_tag("used", None).unwrap();
        let orphan_a = db.add_tag("orphan_a", None).unwrap();
        let orphan_b = db.add_tag("orphan_b", None).unwrap();
        db.set_doc_tags(d.id, &[used.id]).unwrap();

        let removed = db.delete_unused_tags().unwrap();
        assert_eq!(removed, 2, "both orphans removed, used kept");
        // The in-use tag survives; the orphans are gone.
        assert!(db.find_tag_by_id(used.id).unwrap().is_some());
        assert!(db.find_tag_by_id(orphan_a.id).unwrap().is_none());
        assert!(db.find_tag_by_id(orphan_b.id).unwrap().is_none());
        // Only the in-use tag is left in the table.
        let remaining = db.list_tags().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, used.id);
    }

    #[test]
    fn delete_unused_tags_is_noop_when_all_used() {
        // Every tag wears at least one doc → nothing to remove, returns 0,
        // and no tag is touched.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let a = db.add_tag("a", None).unwrap();
        let b = db.add_tag("b", None).unwrap();
        db.set_doc_tags(d.id, &[a.id, b.id]).unwrap();

        let removed = db.delete_unused_tags().unwrap();
        assert_eq!(removed, 0, "all tags in use, nothing removed");
        assert_eq!(db.list_tags().unwrap().len(), 2);
    }

    #[test]
    fn delete_unused_tags_empty_library_is_zero() {
        // No tags at all is a clean no-op (not an error).
        let mut db = db();
        assert_eq!(db.delete_unused_tags().unwrap(), 0);
    }

    #[test]
    fn delete_unused_tags_cleans_merge_and_remove_residue() {
        // The real motivation: a merge leaves the source tag gone, but a
        // *bulk remove* that strips a tag off its last document leaves the tag
        // row behind with count 0. delete_unused_tags is what reclaims it.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let lonely = db.add_tag("lonely", None).unwrap();
        let keeper = db.add_tag("keeper", None).unwrap();
        db.set_doc_tags(d.id, &[lonely.id, keeper.id]).unwrap();

        // Strip "lonely" off its only document — the tag row now lingers unused.
        crate::pdf::library::bulk_tag::remove_tag_from_docs(&mut db, lonely.id, &[d.id]).unwrap();
        let before: std::collections::HashMap<i64, i64> =
            db.tag_usage_counts().unwrap().into_iter().collect();
        assert_eq!(before.get(&lonely.id), Some(&0), "lonely lingers at 0");

        let removed = db.delete_unused_tags().unwrap();
        assert_eq!(removed, 1, "the now-unused tag is reclaimed");
        assert!(db.find_tag_by_id(lonely.id).unwrap().is_none());
        assert!(db.find_tag_by_id(keeper.id).unwrap().is_some());
    }

    #[test]
    fn valid_tag_color_accepts_expected_shapes() {
        for ok in [
            "#abc",
            "#abcd",
            "#aabbcc",
            "#aabbccdd",
            "#FF7A59",
            "hsl(123, 60%, 80%)",
            "hsla(123, 60%, 80%, 0.5)",
            "rgb(255, 122, 89)",
            "rgba(255, 122, 89, 0.5)",
        ] {
            assert!(valid_tag_color(ok), "expected {ok:?} to be valid");
        }
    }

    #[test]
    fn valid_tag_color_rejects_bad_shapes() {
        for bad in [
            "",
            "   ",
            "red",
            "#12",
            "#12345",
            "#gggggg",
            "url(javascript:1)",
            "hsl(1)x",
            "hsl(1, 2, 3); background: url(x)",
            "<script>",
            "rgb(1, 2, 3) !important",
        ] {
            assert!(!valid_tag_color(bad), "expected {bad:?} to be invalid");
        }
    }

    #[test]
    fn set_doc_tags_replaces_existing() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
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

    // Force a known applied_at on a (doc, tag) link so ordering tests are
    // deterministic despite now_unix()'s 1-second resolution.
    fn stamp(db: &LibraryDb, doc_id: i64, tag_id: i64, ts: i64) {
        db.conn()
            .execute(
                "UPDATE library_doc_tags SET applied_at = ?1 WHERE doc_id = ?2 AND tag_id = ?3",
                params![ts, doc_id, tag_id],
            )
            .unwrap();
    }

    #[test]
    fn set_doc_tags_preserves_applied_at_for_surviving_links() {
        // Re-saving an unchanged tag must NOT restamp it — that would shuffle
        // a stable tag to the top of the recently-used list on every save.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t1 = db.add_tag("keep", None).unwrap();
        let t2 = db.add_tag("temp", None).unwrap();
        db.set_doc_tags(d.id, &[t1.id]).unwrap();
        stamp(&db, d.id, t1.id, 1000);

        // Add t2 later; t1's stamp must be untouched.
        db.set_doc_tags(d.id, &[t1.id, t2.id]).unwrap();
        let t1_at: i64 = db
            .conn()
            .query_row(
                "SELECT applied_at FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2",
                params![d.id, t1.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t1_at, 1000, "surviving link keeps its original applied_at");
        // t2 is freshly stamped (non-null, set by now_unix()).
        let t2_at: Option<i64> = db
            .conn()
            .query_row(
                "SELECT applied_at FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2",
                params![d.id, t2.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(t2_at.is_some(), "newly attached link is stamped");
    }

    #[test]
    fn recently_used_tags_orders_by_newest_application() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let old = db.add_tag("old", None).unwrap();
        let mid = db.add_tag("mid", None).unwrap();
        let new = db.add_tag("new", None).unwrap();
        db.set_doc_tags(d.id, &[old.id, mid.id, new.id]).unwrap();
        stamp(&db, d.id, old.id, 100);
        stamp(&db, d.id, mid.id, 200);
        stamp(&db, d.id, new.id, 300);

        let recent = db.recently_used_tags(10).unwrap();
        let names: Vec<&str> = recent.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["new", "mid", "old"], "newest applied first");
    }

    #[test]
    fn recently_used_tags_respects_limit() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let a = db.add_tag("a", None).unwrap();
        let b = db.add_tag("b", None).unwrap();
        let c = db.add_tag("c", None).unwrap();
        db.set_doc_tags(d.id, &[a.id, b.id, c.id]).unwrap();
        stamp(&db, d.id, a.id, 1);
        stamp(&db, d.id, b.id, 2);
        stamp(&db, d.id, c.id, 3);

        let recent = db.recently_used_tags(2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].name, "c");
        assert_eq!(recent[1].name, "b");
        // limit 0 returns an empty list, not every tag.
        assert!(db.recently_used_tags(0).unwrap().is_empty());
    }

    #[test]
    fn recently_used_tags_excludes_never_applied_tags() {
        // A tag that exists but was never attached to a document has nothing
        // recent about it and must not appear.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let used = db.add_tag("used", None).unwrap();
        let _unused = db.add_tag("unused", None).unwrap();
        db.set_doc_tags(d.id, &[used.id]).unwrap();

        let recent = db.recently_used_tags(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "used");
    }

    #[test]
    fn recently_used_tags_lists_each_tag_once_by_newest_use() {
        // A tag applied to several docs collapses to one chip, ranked by its
        // single newest application across all of them.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d1 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/b.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let shared = db.add_tag("shared", None).unwrap();
        let other = db.add_tag("other", None).unwrap();
        db.set_doc_tags(d1.id, &[shared.id]).unwrap();
        db.set_doc_tags(d2.id, &[shared.id, other.id]).unwrap();
        stamp(&db, d1.id, shared.id, 100);
        stamp(&db, d2.id, shared.id, 500); // shared's newest use
        stamp(&db, d2.id, other.id, 300);

        let recent = db.recently_used_tags(10).unwrap();
        let names: Vec<&str> = recent.iter().map(|t| t.name.as_str()).collect();
        // shared appears once, and ranks above other (500 > 300).
        assert_eq!(names, vec!["shared", "other"]);
    }

    #[test]
    fn recently_used_tags_sorts_legacy_null_stamps_last() {
        // Links predating the applied_at column carry NULL; they must sort
        // after every timestamped link but still be reachable.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let legacy = db.add_tag("legacy", None).unwrap();
        let fresh = db.add_tag("fresh", None).unwrap();
        db.set_doc_tags(d.id, &[legacy.id, fresh.id]).unwrap();
        // Simulate a pre-migration link: NULL stamp on legacy, real on fresh.
        db.conn()
            .execute(
                "UPDATE library_doc_tags SET applied_at = NULL WHERE tag_id = ?1",
                params![legacy.id],
            )
            .unwrap();
        stamp(&db, d.id, fresh.id, 50);

        let recent = db.recently_used_tags(10).unwrap();
        let names: Vec<&str> = recent.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["fresh", "legacy"], "NULL stamps sort last");
    }

    #[test]
    fn upsert_document_carries_tags_on_re_read() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t1 = db.add_tag("research", None).unwrap();
        db.set_doc_tags(d.id, &[t1.id]).unwrap();
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
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

    #[test]
    fn remove_document_drops_row() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.remove_document(d.id).unwrap();
        assert!(db.find_document_by_path("/tmp/a.pdf").unwrap().is_none());
    }

    #[test]
    fn remove_document_cascades_doc_tags() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t = db.add_tag("paper", None).unwrap();
        db.set_doc_tags(d.id, &[t.id]).unwrap();
        db.remove_document(d.id).unwrap();
        // Tag itself still exists, but the join row is gone — verified
        // indirectly by re-adding the doc and seeing zero attached tags.
        let d2 = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        assert_eq!(d2.tags.len(), 0);
        // And the tag itself is still listed.
        assert_eq!(db.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn remove_tag_drops_row() {
        let mut db = db();
        let t = db.add_tag("paper", None).unwrap();
        db.remove_tag(t.id).unwrap();
        assert!(db.find_tag_by_name("paper").unwrap().is_none());
    }

    #[test]
    fn remove_tag_detaches_from_documents() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        let t1 = db.add_tag("paper", None).unwrap();
        let t2 = db.add_tag("read", None).unwrap();
        db.set_doc_tags(d.id, &[t1.id, t2.id]).unwrap();
        assert_eq!(db.tags_for_document(d.id).unwrap().len(), 2);
        db.remove_tag(t1.id).unwrap();
        let remaining = db.tags_for_document(d.id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, t2.id);
    }

    // -- Slice 2: ocr_state column + transitions --

    #[test]
    fn schema_v2_is_set() {
        let db = db();
        // Atlas (v2.2.0) bumped the schema to v3. The v2 column set is a
        // strict subset, so this test now asserts the lower bound.
        assert!(db.schema_version().unwrap() >= 2);
    }

    #[test]
    fn fresh_doc_defaults_to_unknown_ocr_state() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        assert_eq!(d.ocr_state, OCR_STATE_UNKNOWN);
        assert!(d.ocr_output_path.is_none());
    }

    #[test]
    fn upsert_with_initial_state_writes_it() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(
                Some(f.id),
                "/tmp/scan.pdf",
                None,
                "h",
                1,
                1,
                None,
                Some(OCR_STATE_SCANNED),
            )
            .unwrap();
        assert_eq!(d.ocr_state, OCR_STATE_SCANNED);
    }

    #[test]
    fn set_doc_ocr_state_round_trips() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.set_doc_ocr_state(d.id, OCR_STATE_PENDING).unwrap();
        let d2 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d2.ocr_state, OCR_STATE_PENDING);
        db.set_doc_ocr_state(d.id, OCR_STATE_DONE).unwrap();
        let d3 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d3.ocr_state, OCR_STATE_DONE);
    }

    #[test]
    fn set_doc_ocr_output_path_round_trips() {
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h", 1, 1, None, None)
            .unwrap();
        db.set_doc_ocr_output_path(d.id, Some("/tmp/a.ocr.pdf"))
            .unwrap();
        let d2 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d2.ocr_output_path.as_deref(), Some("/tmp/a.ocr.pdf"));
        db.set_doc_ocr_output_path(d.id, None).unwrap();
        let d3 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert!(d3.ocr_output_path.is_none());
    }

    #[test]
    fn upsert_existing_doc_preserves_ocr_state() {
        // Once an OCR state has been set (queue ran, marked done), a
        // subsequent scanner-driven upsert MUST NOT clobber it.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let d = db
            .upsert_document(
                Some(f.id),
                "/tmp/a.pdf",
                None,
                "h1",
                1,
                1,
                None,
                Some(OCR_STATE_SCANNED),
            )
            .unwrap();
        db.set_doc_ocr_state(d.id, OCR_STATE_DONE).unwrap();
        // Scanner re-runs with the file having changed → it would pass
        // a fresh classification. Must not overwrite ocr_done.
        let _ = db
            .upsert_document(
                Some(f.id),
                "/tmp/a.pdf",
                None,
                "h2",
                2,
                2,
                None,
                Some(OCR_STATE_TEXT_NATIVE),
            )
            .unwrap();
        let d2 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d2.ocr_state, OCR_STATE_DONE);
    }

    #[test]
    fn upsert_upgrades_unknown_state_when_initial_provided() {
        // The most common path: legacy row inserted before Slice 2 with
        // ocr_state='unknown'. Next scan re-runs scan_audit and passes
        // Some(state); the upsert should accept that upgrade.
        let mut db = db();
        let f = db.add_folder("/tmp").unwrap();
        let _ = db
            .upsert_document(Some(f.id), "/tmp/a.pdf", None, "h1", 1, 1, None, None)
            .unwrap();
        let _ = db
            .upsert_document(
                Some(f.id),
                "/tmp/a.pdf",
                None,
                "h2",
                2,
                2,
                None,
                Some(OCR_STATE_SCANNED),
            )
            .unwrap();
        let d2 = db.find_document_by_path("/tmp/a.pdf").unwrap().unwrap();
        assert_eq!(d2.ocr_state, OCR_STATE_SCANNED);
    }
}
