//! FTS5-backed page-level full-text index, colocated with the library DB.
//!
//! Schema v3 adds the `library_fts` virtual table keyed on
//! `(doc_id, page_index)`. Rows are upserted by the scanner after a doc's
//! page text is extracted. An `AFTER DELETE ON library_documents` trigger
//! cascades fts rows away when documents are removed.
//!
//! Why FTS5 and not Tantivy?
//! * rusqlite already links libsqlite with the FTS5 module enabled
//!   (bundled feature), so zero new build deps.
//! * Atomic delete-cascade in the same transaction as document delete.
//! * unicode61 tokenizer handles diacritic-folded matches out of the box.
//! * bm25() ranking is built-in.

use rusqlite::{params, Connection};

use super::registry::LibraryError;

/// Idempotent migration from schema v2 → v3. Safe to call repeatedly; if
/// `user_version` is already ≥ 3, this is a noop. Adds the
/// `library_fts` virtual table and the cascade-delete trigger.
pub fn migrate_v3(conn: &Connection) -> Result<(), LibraryError> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 3 {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS library_fts USING fts5(
            doc_id UNINDEXED,
            page_index UNINDEXED,
            text,
            tokenize = 'unicode61 remove_diacritics 2'
        );

        CREATE TRIGGER IF NOT EXISTS library_fts_cascade_delete
        AFTER DELETE ON library_documents
        BEGIN
            DELETE FROM library_fts WHERE doc_id = OLD.id;
        END;

        PRAGMA user_version = 3;
        "#,
    )?;
    Ok(())
}

/// Replace all FTS rows for `doc_id`, then insert one row per non-blank
/// page in `pages`. Empty pages are skipped (no point indexing zero
/// tokens). Called by the scanner after every successful upsert.
pub fn index_doc(conn: &Connection, doc_id: i64, pages: &[String]) -> Result<(), LibraryError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM library_fts WHERE doc_id = ?1", params![doc_id])?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO library_fts (doc_id, page_index, text) VALUES (?1, ?2, ?3)",
        )?;
        for (i, page) in pages.iter().enumerate() {
            if page.trim().is_empty() {
                continue;
            }
            stmt.execute(params![doc_id, i as i64, page])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Total number of indexed pages across all documents — used by the
/// Library panel's status footer and by tests.
pub fn total_indexed_pages(conn: &Connection) -> Result<i64, LibraryError> {
    let n: i64 = conn.query_row("SELECT count(*) FROM library_fts", [], |r| r.get(0))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::registry::LibraryDb;

    fn seed_doc(conn: &Connection, id: i64, path: &str) {
        conn.execute(
            "INSERT INTO library_documents (id, folder_id, path, hash, size_bytes, mtime_ns, added_at, last_seen_at)
             VALUES (?1, NULL, ?2, 'h', 1, 0, 0, 0)",
            params![id, path],
        )
        .unwrap();
    }

    #[test]
    fn fts_table_exists_after_migration() {
        let db = LibraryDb::open_in_memory().unwrap();
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='library_fts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "library_fts virtual table must exist");
    }

    #[test]
    fn schema_version_is_at_least_three() {
        let db = LibraryDb::open_in_memory().unwrap();
        assert!(db.schema_version().unwrap() >= 3);
    }

    #[test]
    fn migrate_v3_is_idempotent() {
        let db = LibraryDb::open_in_memory().unwrap();
        // Already at v3 — calling again must be a noop.
        migrate_v3(db.conn()).unwrap();
        migrate_v3(db.conn()).unwrap();
        assert!(db.schema_version().unwrap() >= 3);
    }

    #[test]
    fn index_doc_round_trips_page_text() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed_doc(db.conn(), 42, "/x/a.pdf");
        index_doc(
            db.conn(),
            42,
            &[
                "This contract shall be governed by indemnification clause 3.4".into(),
                "Second page content with arbitration language".into(),
            ],
        )
        .unwrap();
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM library_fts WHERE doc_id = 42",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn index_doc_skips_blank_pages() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed_doc(db.conn(), 5, "/x/b.pdf");
        index_doc(
            db.conn(),
            5,
            &[
                "real text".into(),
                "   ".into(),
                "".into(),
                "more text".into(),
            ],
        )
        .unwrap();
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM library_fts WHERE doc_id = 5",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn index_doc_replaces_existing_rows_on_reindex() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed_doc(db.conn(), 9, "/x/c.pdf");
        index_doc(
            db.conn(),
            9,
            &["v1 first".into(), "v1 second".into(), "v1 third".into()],
        )
        .unwrap();
        index_doc(db.conn(), 9, &["v2 first".into()]).unwrap();
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM library_fts WHERE doc_id = 9",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            n, 1,
            "reindex must purge old rows before inserting new ones"
        );
    }

    #[test]
    fn deleting_doc_cascades_into_fts() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed_doc(db.conn(), 7, "/x/d.pdf");
        index_doc(db.conn(), 7, &["hello".into()]).unwrap();
        db.conn()
            .execute("DELETE FROM library_documents WHERE id = 7", [])
            .unwrap();
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM library_fts WHERE doc_id = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "AFTER DELETE trigger must cascade to library_fts");
    }

    #[test]
    fn total_indexed_pages_counts_across_docs() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed_doc(db.conn(), 1, "/x/1.pdf");
        seed_doc(db.conn(), 2, "/x/2.pdf");
        index_doc(db.conn(), 1, &["a".into(), "b".into()]).unwrap();
        index_doc(db.conn(), 2, &["c".into()]).unwrap();
        assert_eq!(total_indexed_pages(db.conn()).unwrap(), 3);
    }
}
