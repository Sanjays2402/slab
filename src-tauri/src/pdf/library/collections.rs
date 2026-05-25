// v3.32.0 "Atlas" — Collections & Smart Collections.
//
// **Manual collections** are user-curated bags of documents (think: "Tax
// 2026", "Onboarding packets"). They live in `library_collections` +
// `library_collection_docs`.
//
// **Smart collections** persist a `LibraryFilter` as JSON in
// `library_smart_collections.query_json`. Re-evaluated live every time
// the user clicks the collection in the sidebar — so new scans show up
// without the user re-running a saved search.
//
// Why a separate module: keeps `registry.rs` lean (it's already 900+
// lines) and gives Collections their own test surface.

use super::query::{query_documents, LibraryFilter};
use super::registry::{DocumentRecord, LibraryDb, LibraryError};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionRecord {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub created_at: i64,
    pub sort_order: i64,
    /// Eager-loaded count of documents in this collection. Powers the
    /// animated sidebar badges.
    pub doc_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmartCollectionRecord {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// Serialized `LibraryFilter`. The frontend never reads this raw —
    /// it asks `list_smart_collection_docs` to expand it.
    pub query_json: String,
    pub created_at: i64,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSmartCollection {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub filter: LibraryFilter,
}

// ---------------------------------------------------------------
// Manual collections
// ---------------------------------------------------------------

pub fn create_collection(
    db: &mut LibraryDb,
    name: &str,
    icon: Option<&str>,
    color: Option<&str>,
) -> Result<CollectionRecord, LibraryError> {
    let now = now_unix();
    let conn = db.conn_mut();
    conn.execute(
        "INSERT INTO library_collections (name, icon, color, created_at, sort_order)
         VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT MAX(sort_order) + 1 FROM library_collections), 0))",
        params![name, icon, color, now],
    )?;
    let id = conn.last_insert_rowid();
    get_collection(db, id)
}

pub fn get_collection(db: &LibraryDb, id: i64) -> Result<CollectionRecord, LibraryError> {
    let conn = db.conn();
    let row = conn.query_row(
        "SELECT c.id, c.name, c.icon, c.color, c.created_at, c.sort_order,
                (SELECT COUNT(*) FROM library_collection_docs d WHERE d.collection_id = c.id)
         FROM library_collections c
         WHERE c.id = ?1",
        params![id],
        |r| {
            Ok(CollectionRecord {
                id: r.get(0)?,
                name: r.get(1)?,
                icon: r.get(2)?,
                color: r.get(3)?,
                created_at: r.get(4)?,
                sort_order: r.get(5)?,
                doc_count: r.get(6)?,
            })
        },
    )?;
    Ok(row)
}

pub fn list_collections(db: &LibraryDb) -> Result<Vec<CollectionRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.icon, c.color, c.created_at, c.sort_order,
                (SELECT COUNT(*) FROM library_collection_docs d WHERE d.collection_id = c.id)
         FROM library_collections c
         ORDER BY c.sort_order ASC, c.id ASC",
    )?;
    let rows: Result<Vec<_>, _> = stmt
        .query_map([], |r| {
            Ok(CollectionRecord {
                id: r.get(0)?,
                name: r.get(1)?,
                icon: r.get(2)?,
                color: r.get(3)?,
                created_at: r.get(4)?,
                sort_order: r.get(5)?,
                doc_count: r.get(6)?,
            })
        })?
        .collect();
    Ok(rows?)
}

pub fn rename_collection(db: &mut LibraryDb, id: i64, name: &str) -> Result<(), LibraryError> {
    db.conn_mut().execute(
        "UPDATE library_collections SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    Ok(())
}

pub fn delete_collection(db: &mut LibraryDb, id: i64) -> Result<(), LibraryError> {
    db.conn_mut()
        .execute("DELETE FROM library_collections WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn add_docs(
    db: &mut LibraryDb,
    collection_id: i64,
    doc_ids: &[i64],
) -> Result<usize, LibraryError> {
    let now = now_unix();
    let tx = db.conn_mut().transaction()?;
    let mut added = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO library_collection_docs (collection_id, doc_id, added_at)
             VALUES (?1, ?2, ?3)",
        )?;
        for &doc_id in doc_ids {
            added += stmt.execute(params![collection_id, doc_id, now])?;
        }
    }
    tx.commit()?;
    Ok(added)
}

pub fn remove_docs(
    db: &mut LibraryDb,
    collection_id: i64,
    doc_ids: &[i64],
) -> Result<usize, LibraryError> {
    let tx = db.conn_mut().transaction()?;
    let mut removed = 0usize;
    {
        let mut stmt = tx.prepare(
            "DELETE FROM library_collection_docs
             WHERE collection_id = ?1 AND doc_id = ?2",
        )?;
        for &doc_id in doc_ids {
            removed += stmt.execute(params![collection_id, doc_id])?;
        }
    }
    tx.commit()?;
    Ok(removed)
}

pub fn list_collection_docs(
    db: &LibraryDb,
    collection_id: i64,
) -> Result<Vec<DocumentRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT d.id, d.folder_id, d.path, d.title, d.hash, d.size_bytes,
                d.mtime_ns, d.pages, d.added_at, d.last_seen_at,
                d.ocr_state, d.ocr_output_path
         FROM library_documents d
         JOIN library_collection_docs cd ON cd.doc_id = d.id
         WHERE cd.collection_id = ?1
         ORDER BY cd.added_at DESC, d.id DESC",
    )?;
    let rows: Result<Vec<_>, _> = stmt
        .query_map(params![collection_id], |r| {
            Ok(DocumentRecord {
                id: r.get(0)?,
                folder_id: r.get(1)?,
                path: r.get(2)?,
                title: r.get(3)?,
                hash: r.get(4)?,
                size_bytes: r.get(5)?,
                mtime_ns: r.get(6)?,
                pages: r.get(7)?,
                added_at: r.get(8)?,
                last_seen_at: r.get(9)?,
                ocr_state: r.get(10)?,
                ocr_output_path: r.get(11)?,
                tags: Vec::new(),
            })
        })?
        .collect();
    Ok(rows?)
}

// ---------------------------------------------------------------
// Smart collections
// ---------------------------------------------------------------

pub fn create_smart_collection(
    db: &mut LibraryDb,
    spec: &NewSmartCollection,
) -> Result<SmartCollectionRecord, LibraryError> {
    let now = now_unix();
    let json = serde_json::to_string(&spec.filter)
        .map_err(|e| LibraryError::Db(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?;
    let conn = db.conn_mut();
    conn.execute(
        "INSERT INTO library_smart_collections
            (name, icon, color, query_json, created_at, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5,
                 COALESCE((SELECT MAX(sort_order) + 1 FROM library_smart_collections), 0))",
        params![spec.name, spec.icon, spec.color, json, now],
    )?;
    let id = conn.last_insert_rowid();
    get_smart_collection(db, id)
}

pub fn get_smart_collection(
    db: &LibraryDb,
    id: i64,
) -> Result<SmartCollectionRecord, LibraryError> {
    let conn = db.conn();
    let row = conn.query_row(
        "SELECT id, name, icon, color, query_json, created_at, sort_order
         FROM library_smart_collections WHERE id = ?1",
        params![id],
        |r| {
            Ok(SmartCollectionRecord {
                id: r.get(0)?,
                name: r.get(1)?,
                icon: r.get(2)?,
                color: r.get(3)?,
                query_json: r.get(4)?,
                created_at: r.get(5)?,
                sort_order: r.get(6)?,
            })
        },
    )?;
    Ok(row)
}

pub fn list_smart_collections(db: &LibraryDb) -> Result<Vec<SmartCollectionRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, icon, color, query_json, created_at, sort_order
         FROM library_smart_collections
         ORDER BY sort_order ASC, id ASC",
    )?;
    let rows: Result<Vec<_>, _> = stmt
        .query_map([], |r| {
            Ok(SmartCollectionRecord {
                id: r.get(0)?,
                name: r.get(1)?,
                icon: r.get(2)?,
                color: r.get(3)?,
                query_json: r.get(4)?,
                created_at: r.get(5)?,
                sort_order: r.get(6)?,
            })
        })?
        .collect();
    Ok(rows?)
}

pub fn delete_smart_collection(db: &mut LibraryDb, id: i64) -> Result<(), LibraryError> {
    db.conn_mut().execute(
        "DELETE FROM library_smart_collections WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Expand a smart collection to its current document list by parsing
/// `query_json` and running it through `query_documents`.
pub fn expand_smart_collection(
    db: &LibraryDb,
    id: i64,
) -> Result<Vec<DocumentRecord>, LibraryError> {
    let sc = get_smart_collection(db, id)?;
    let filter: LibraryFilter = serde_json::from_str(&sc.query_json).map_err(|e| {
        LibraryError::Db(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(e),
        ))
    })?;
    query_documents(db, &filter)
}

/// Seed the default smart collections on first run. Idempotent:
/// honors UNIQUE(name) so re-running is a no-op.
pub fn seed_defaults(db: &mut LibraryDb) -> Result<(), LibraryError> {
    let defaults: [NewSmartCollection; 2] = [
        NewSmartCollection {
            name: "Recently added".into(),
            icon: Some("sparkles".into()),
            color: Some("#7cc4ff".into()),
            filter: LibraryFilter {
                sort: super::query::SortBy::AddedDesc,
                limit: Some(50),
                ..Default::default()
            },
        },
        NewSmartCollection {
            name: "All documents".into(),
            icon: Some("library".into()),
            color: Some("#a78bfa".into()),
            filter: LibraryFilter::default(),
        },
    ];
    for spec in &defaults {
        let json = serde_json::to_string(&spec.filter)
            .map_err(|e| LibraryError::Db(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?;
        let now = now_unix();
        db.conn_mut().execute(
            "INSERT OR IGNORE INTO library_smart_collections
                (name, icon, color, query_json, created_at, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     COALESCE((SELECT MAX(sort_order) + 1 FROM library_smart_collections), 0))",
            params![spec.name, spec.icon, spec.color, json, now],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------
// Tests
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::registry::LibraryDb;
    use std::fs;
    use tempfile::tempdir;

    fn make_doc(db: &mut LibraryDb, name: &str) -> i64 {
        // Minimal: stash a row directly via add_folder + a manual insert
        // so tests don't depend on a real PDF on disk.
        let tmp = tempdir().unwrap();
        let folder = db.add_folder(tmp.path().to_str().unwrap()).unwrap();
        let path = tmp.path().join(format!("{name}.pdf"));
        fs::write(&path, b"%PDF-1.4\n%dummy").unwrap();
        // Insert minimal row by hand so tests don't need full scanner success.
        db.conn_mut()
            .execute(
                "INSERT OR IGNORE INTO library_documents
                    (folder_id, path, title, hash, size_bytes, mtime_ns,
                     pages, added_at, last_seen_at, ocr_state)
                 VALUES (?1, ?2, ?3, 'h', 0, 0, 1, 0, 0, 'unknown')",
                params![folder.id, path.to_str().unwrap(), name],
            )
            .unwrap();
        db.conn().last_insert_rowid()
    }

    #[test]
    fn collections_tables_exist_after_open() {
        let db = LibraryDb::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), 4);
        // Bogus query proves the tables exist (would error if not).
        let n: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM library_collections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM library_smart_collections", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn create_and_list_collection() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Tax 2026", Some("folder"), Some("#fcd34d")).unwrap();
        assert_eq!(c.name, "Tax 2026");
        assert_eq!(c.doc_count, 0);
        let all = list_collections(&db).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, c.id);
    }

    #[test]
    fn add_remove_docs_updates_count() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Onboarding", None, None).unwrap();
        let d1 = make_doc(&mut db, "intro");
        let d2 = make_doc(&mut db, "policy");
        assert_eq!(add_docs(&mut db, c.id, &[d1, d2]).unwrap(), 2);
        let fetched = get_collection(&db, c.id).unwrap();
        assert_eq!(fetched.doc_count, 2);
        let docs = list_collection_docs(&db, c.id).unwrap();
        assert_eq!(docs.len(), 2);
        // Idempotent re-add
        assert_eq!(add_docs(&mut db, c.id, &[d1]).unwrap(), 0);
        assert_eq!(remove_docs(&mut db, c.id, &[d1]).unwrap(), 1);
        assert_eq!(get_collection(&db, c.id).unwrap().doc_count, 1);
    }

    #[test]
    fn rename_and_delete() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Old", None, None).unwrap();
        rename_collection(&mut db, c.id, "New").unwrap();
        assert_eq!(get_collection(&db, c.id).unwrap().name, "New");
        delete_collection(&mut db, c.id).unwrap();
        assert_eq!(list_collections(&db).unwrap().len(), 0);
    }

    #[test]
    fn smart_collection_roundtrip_and_expand() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let _d1 = make_doc(&mut db, "alpha");
        let _d2 = make_doc(&mut db, "beta");
        let sc = create_smart_collection(
            &mut db,
            &NewSmartCollection {
                name: "Everything".into(),
                icon: None,
                color: None,
                filter: LibraryFilter::default(),
            },
        )
        .unwrap();
        let docs = expand_smart_collection(&db, sc.id).unwrap();
        assert!(docs.len() >= 2);
        let all = list_smart_collections(&db).unwrap();
        assert_eq!(all.len(), 1);
        delete_smart_collection(&mut db, sc.id).unwrap();
        assert_eq!(list_smart_collections(&db).unwrap().len(), 0);
    }

    #[test]
    fn seed_defaults_is_idempotent() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        seed_defaults(&mut db).unwrap();
        seed_defaults(&mut db).unwrap();
        let all = list_smart_collections(&db).unwrap();
        assert_eq!(all.len(), 2);
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Recently added"));
        assert!(names.contains(&"All documents"));
    }
}
