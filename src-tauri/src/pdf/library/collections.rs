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

/// Maximum length, in Unicode scalars (not bytes), accepted for a manual
/// collection name. Mirrors the budget tags get for descriptions — generous
/// enough for emoji + CJK names, tight enough that the rail row stays
/// readable. v3.53.0 Atlas Collections.
pub(crate) const MAX_COLLECTION_NAME_LEN: usize = 120;

fn valid_collection_name(name: &str) -> bool {
    let len = name.chars().count();
    len > 0 && len <= MAX_COLLECTION_NAME_LEN
}

/// Rename a manual collection in place. Returns the updated row.
///
/// Trims the input. Empty (after trim) is rejected. Same-name (after trim)
/// short-circuits without an UPDATE — important because the rail's inline
/// rename UI commits on blur, and we don't want a no-op edit to fire a
/// library-changed event. A length cap protects the rail row. A collision
/// with a *different* collection's name (UNIQUE) is rejected with a named
/// error so the UI can show it inline; the bare rusqlite "UNIQUE
/// constraint failed" message is opaque. Unknown id errors.
/// v3.53.0 Atlas Collections — Slice 23.
pub fn rename_collection(
    db: &mut LibraryDb,
    id: i64,
    name: &str,
) -> Result<CollectionRecord, LibraryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(LibraryError::Other(
            "collection name cannot be empty".into(),
        ));
    }
    if !valid_collection_name(trimmed) {
        return Err(LibraryError::Other(format!(
            "collection name too long (max {MAX_COLLECTION_NAME_LEN} chars)"
        )));
    }
    let current = get_collection(db, id)?;
    // Same-name (after trim) is a no-op so a blur-to-commit inline rename
    // doesn't fire library-changed for nothing.
    if current.name == trimmed {
        return Ok(current);
    }
    // Reject a collision with a *different* collection's name. Look it up
    // first so the error message names the conflict instead of leaking the
    // raw UNIQUE-constraint string.
    let collision: Option<i64> = db
        .conn()
        .query_row(
            "SELECT id FROM library_collections WHERE name = ?1",
            params![trimmed],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    if let Some(other_id) = collision {
        if other_id != id {
            return Err(LibraryError::Other(format!(
                "a collection named {trimmed:?} already exists"
            )));
        }
    }
    db.conn_mut().execute(
        "UPDATE library_collections SET name = ?1 WHERE id = ?2",
        params![trimmed, id],
    )?;
    get_collection(db, id)
}

pub fn delete_collection(db: &mut LibraryDb, id: i64) -> Result<(), LibraryError> {
    db.conn_mut()
        .execute("DELETE FROM library_collections WHERE id = ?1", params![id])?;
    Ok(())
}

/// Update a collection's color (or clear it back to NULL with `None`).
/// Returns the updated row.
///
/// Input is trimmed; trimmed-empty is treated as `None` so the column only
/// ever holds a "real" color. Non-None values are checked by the shared
/// [`valid_tag_color`] guard (same CSS shape allowlist tags use — `#hex`
/// and functional `hsl()/hsla()/rgb()/rgba()`) so a stored value can never
/// carry CSS that breaks out of the property it's dropped into. Unknown id
/// errors. The guard runs BEFORE the UPDATE so a rejected color leaves the
/// row's prior color untouched.
/// v3.53.0 Atlas Collections — Slice 24.
pub fn set_collection_color(
    db: &mut LibraryDb,
    id: i64,
    color: Option<&str>,
) -> Result<CollectionRecord, LibraryError> {
    // Normalize: treat whitespace-only as a clear, validate real values.
    let normalized: Option<String> = match color {
        None => None,
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                if !super::registry::valid_tag_color(trimmed) {
                    return Err(LibraryError::Other(format!(
                        "invalid collection color: {trimmed:?}"
                    )));
                }
                Some(trimmed.to_string())
            }
        }
    };
    // Ensure the row exists up front so a bad id reports a clean error
    // instead of a silent 0-row UPDATE.
    let _current = get_collection(db, id)?;
    db.conn_mut().execute(
        "UPDATE library_collections SET color = ?1 WHERE id = ?2",
        params![normalized, id],
    )?;
    get_collection(db, id)
}

/// Rewrite the `sort_order` of every collection in `ordered_ids` so the
/// rail displays them in the supplied sequence on next list. Atomic.
///
/// Semantics:
/// - Any id in `ordered_ids` that doesn't exist in the table is silently
///   skipped (a stale id from a list-vs-reorder race shouldn't crash the
///   UI). Returns the count of rows whose sort_order actually moved.
/// - Any collection in the table NOT named in `ordered_ids` keeps its
///   existing sort_order (the rail can reorder a subset without
///   reshuffling the rest — though the rail today reorders the full
///   list).
/// - The new sort_order values use 100, 200, 300, … so a future
///   single-row reorder can splice between two existing rows without a
///   full rewrite (room to grow without rounding to fractions).
/// - Duplicate ids in `ordered_ids` are accepted but their last
///   appearance wins (we just keep stomping the UPDATE — final state
///   matches whoever came last). No error: the UI never produces dups,
///   but tolerating them keeps the wire contract forgiving.
///
/// v3.53.0 Atlas Collections — Slice 25.
pub fn reorder_collections(db: &mut LibraryDb, ordered_ids: &[i64]) -> Result<usize, LibraryError> {
    let tx = db.conn_mut().transaction()?;
    let mut moved = 0usize;
    {
        let mut stmt = tx.prepare(
            "UPDATE library_collections SET sort_order = ?1 WHERE id = ?2 AND sort_order != ?1",
        )?;
        for (idx, id) in ordered_ids.iter().enumerate() {
            // Step by 100 so a future precision-insert has room.
            let new_order = ((idx as i64) + 1) * 100;
            moved += stmt.execute(params![new_order, *id])?;
        }
    }
    tx.commit()?;
    Ok(moved)
}

/// Clone a manual collection — name, icon, color, AND its full document
/// membership — under a new auto-suffixed name. The new row drops in at
/// the end of the sort_order (`MAX(sort_order) + 1`). Atomic.
///
/// Name semantics: appends `" (copy)"` to the source name; if that's
/// already taken, tries `" (copy 2)"`, `" (copy 3)"`, …. Caps after
/// `MAX_COLLECTION_NAME_LEN - 8` source chars so the suffix fits.
/// Returns the freshly-created CollectionRecord with a populated
/// `doc_count` already reflecting the cloned membership (no extra
/// round-trip needed). Unknown id errors before any write.
/// v3.53.0 Atlas Collections — Slice 26.
pub fn duplicate_collection(
    db: &mut LibraryDb,
    source_id: i64,
) -> Result<CollectionRecord, LibraryError> {
    let source = get_collection(db, source_id)?;
    let new_name = next_copy_name(db, &source.name)?;
    let now = now_unix();
    let tx = db.conn_mut().transaction()?;
    let new_id = {
        tx.execute(
            "INSERT INTO library_collections (name, icon, color, created_at, sort_order)
             VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT MAX(sort_order) + 1 FROM library_collections), 0))",
            params![new_name, source.icon, source.color, now],
        )?;
        let id = tx.last_insert_rowid();
        // Clone the membership in one INSERT … SELECT so we never touch
        // doc_id rows individually (fewer round-trips, identical added_at
        // baseline for every doc copied across — so the new collection's
        // "added_at DESC" preview lands in a stable order).
        tx.execute(
            "INSERT OR IGNORE INTO library_collection_docs (collection_id, doc_id, added_at)
             SELECT ?1, doc_id, ?2 FROM library_collection_docs WHERE collection_id = ?3",
            params![id, now, source_id],
        )?;
        id
    };
    tx.commit()?;
    get_collection(db, new_id)
}

/// Pick the next available `"X (copy)"` / `"X (copy 2)"` / … name for a
/// duplicate, capped at [`MAX_COLLECTION_NAME_LEN`] scalars (the source is
/// truncated, not the suffix). Skips up to 999 collisions before giving
/// up — a paralegal who has duplicated a collection 999 times has bigger
/// problems than this loop.
fn next_copy_name(db: &LibraryDb, source_name: &str) -> Result<String, LibraryError> {
    // Reserve 8 chars for " (copy)" + buffer; suffix grows to " (copy 999)"
    // which is 11 chars — give a 12-char budget.
    let budget = MAX_COLLECTION_NAME_LEN.saturating_sub(12);
    let trimmed_source: String = source_name.chars().take(budget).collect();
    let name_exists = |name: &str| -> Result<bool, LibraryError> {
        let count: i64 = db.conn().query_row(
            "SELECT COUNT(*) FROM library_collections WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    };
    let first = format!("{trimmed_source} (copy)");
    if !name_exists(&first)? {
        return Ok(first);
    }
    for n in 2..=999 {
        let cand = format!("{trimmed_source} (copy {n})");
        if !name_exists(&cand)? {
            return Ok(cand);
        }
    }
    Err(LibraryError::Other(
        "ran out of copy-suffix slots (999 dupes already)".into(),
    ))
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
                d.ocr_state, d.ocr_output_path, d.ocr_error, d.notes, d.starred
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
                ocr_error: r.get(12)?,
                notes: r.get(13)?,
                starred: r.get::<_, i64>(14)? != 0,
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

/// Update one or more fields on a smart collection. Each `Option` is
/// "don't touch this field" when `None`. `Some(Some(v))` sets,
/// `Some(None)` clears a nullable column.
pub fn update_smart_collection(
    db: &mut LibraryDb,
    id: i64,
    name: Option<&str>,
    icon: Option<Option<&str>>,
    color: Option<Option<&str>>,
    filter: Option<&LibraryFilter>,
) -> Result<SmartCollectionRecord, LibraryError> {
    let conn = db.conn_mut();
    if let Some(n) = name {
        conn.execute(
            "UPDATE library_smart_collections SET name = ?1 WHERE id = ?2",
            params![n, id],
        )?;
    }
    if let Some(ic) = icon {
        conn.execute(
            "UPDATE library_smart_collections SET icon = ?1 WHERE id = ?2",
            params![ic, id],
        )?;
    }
    if let Some(c) = color {
        conn.execute(
            "UPDATE library_smart_collections SET color = ?1 WHERE id = ?2",
            params![c, id],
        )?;
    }
    if let Some(f) = filter {
        let json = serde_json::to_string(f)
            .map_err(|e| LibraryError::Db(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?;
        conn.execute(
            "UPDATE library_smart_collections SET query_json = ?1 WHERE id = ?2",
            params![json, id],
        )?;
    }
    get_smart_collection(db, id)
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
        // Schema migrates to the latest version on open; this assertion just
        // guards that open() ran migrations past the collections tables (v4).
        assert!(db.schema_version().unwrap() >= 4);
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
        let updated = rename_collection(&mut db, c.id, "New").unwrap();
        assert_eq!(updated.id, c.id);
        assert_eq!(updated.name, "New");
        assert_eq!(get_collection(&db, c.id).unwrap().name, "New");
        delete_collection(&mut db, c.id).unwrap();
        assert_eq!(list_collections(&db).unwrap().len(), 0);
    }

    #[test]
    fn rename_collection_trims_whitespace() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Old", None, None).unwrap();
        let updated = rename_collection(&mut db, c.id, "  Tax 2026  ").unwrap();
        assert_eq!(updated.name, "Tax 2026");
        assert_eq!(get_collection(&db, c.id).unwrap().name, "Tax 2026");
    }

    #[test]
    fn rename_collection_rejects_empty_and_whitespace_only() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Old", None, None).unwrap();
        assert!(rename_collection(&mut db, c.id, "").is_err());
        assert!(rename_collection(&mut db, c.id, "    ").is_err());
        // Row untouched.
        assert_eq!(get_collection(&db, c.id).unwrap().name, "Old");
    }

    #[test]
    fn rename_collection_same_name_is_noop() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Tax 2026", None, None).unwrap();
        // Returns the current row, no error. Trailing whitespace still trims.
        let got = rename_collection(&mut db, c.id, "  Tax 2026  ").unwrap();
        assert_eq!(got.name, "Tax 2026");
        assert_eq!(got.id, c.id);
    }

    #[test]
    fn rename_collection_rejects_unique_collision() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "Tax 2026", None, None).unwrap();
        let b = create_collection(&mut db, "Onboarding", None, None).unwrap();
        // Renaming b onto a's name is rejected with a named error rather
        // than the opaque rusqlite "UNIQUE constraint failed" message.
        let err = rename_collection(&mut db, b.id, "Tax 2026").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("already exists"), "got {msg:?}");
        // Both rows untouched.
        assert_eq!(get_collection(&db, a.id).unwrap().name, "Tax 2026");
        assert_eq!(get_collection(&db, b.id).unwrap().name, "Onboarding");
    }

    #[test]
    fn rename_collection_rejects_unknown_id() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        assert!(rename_collection(&mut db, 999_999, "Anything").is_err());
    }

    #[test]
    fn rename_collection_caps_length() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Old", None, None).unwrap();
        // At the cap: ok.
        let at_max: String = "x".repeat(MAX_COLLECTION_NAME_LEN);
        let ok = rename_collection(&mut db, c.id, &at_max).unwrap();
        assert_eq!(ok.name.chars().count(), MAX_COLLECTION_NAME_LEN);
        // One past: rejected, row untouched.
        let too_long: String = "x".repeat(MAX_COLLECTION_NAME_LEN + 1);
        assert!(rename_collection(&mut db, c.id, &too_long).is_err());
        assert_eq!(get_collection(&db, c.id).unwrap().name, at_max);
    }

    // -------- Slice 24: set_collection_color --------

    #[test]
    fn set_collection_color_updates_and_returns_row() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Tax 2026", None, Some("#aabbcc")).unwrap();
        let updated = set_collection_color(&mut db, c.id, Some("#6ab7ff")).unwrap();
        assert_eq!(updated.color.as_deref(), Some("#6ab7ff"));
        assert_eq!(
            get_collection(&db, c.id).unwrap().color.as_deref(),
            Some("#6ab7ff")
        );
        // Other fields preserved.
        assert_eq!(updated.name, "Tax 2026");
    }

    #[test]
    fn set_collection_color_trims_whitespace() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "x", None, None).unwrap();
        let updated = set_collection_color(&mut db, c.id, Some("  #7ee787  ")).unwrap();
        assert_eq!(updated.color.as_deref(), Some("#7ee787"));
    }

    #[test]
    fn set_collection_color_none_clears() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "x", None, Some("#aabbcc")).unwrap();
        let cleared = set_collection_color(&mut db, c.id, None).unwrap();
        assert!(cleared.color.is_none());
        // An all-whitespace value is treated as a clear (column never holds
        // "real but empty" trash).
        set_collection_color(&mut db, c.id, Some("#7ee787")).unwrap();
        let cleared2 = set_collection_color(&mut db, c.id, Some("   ")).unwrap();
        assert!(cleared2.color.is_none());
    }

    #[test]
    fn set_collection_color_accepts_hsl_from_pastel_for() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "x", None, None).unwrap();
        // The same shape pastel_for emits for tags; we accept it for collections too.
        let updated = set_collection_color(&mut db, c.id, Some("hsl(123, 60%, 80%)")).unwrap();
        assert_eq!(updated.color.as_deref(), Some("hsl(123, 60%, 80%)"));
    }

    #[test]
    fn set_collection_color_rejects_invalid_color_row_untouched() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "x", None, Some("#aabbcc")).unwrap();
        for bad in [
            "red",
            "javascript:alert(1)",
            "url(http://evil)",
            "#gg",
            "#1234567", // 7 chars not in {3,4,6,8}
            "hsl(120, 50%, 50%); color: red",
        ] {
            assert!(
                set_collection_color(&mut db, c.id, Some(bad)).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
        // Original color is intact — guard runs BEFORE the UPDATE.
        assert_eq!(
            get_collection(&db, c.id).unwrap().color.as_deref(),
            Some("#aabbcc")
        );
    }

    #[test]
    fn set_collection_color_rejects_unknown_id() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        assert!(set_collection_color(&mut db, 999_999, Some("#aabbcc")).is_err());
    }

    #[test]
    fn set_collection_color_preserves_name_and_count() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Onboarding", Some("folder"), None).unwrap();
        let d1 = make_doc(&mut db, "intro");
        add_docs(&mut db, c.id, &[d1]).unwrap();
        let updated = set_collection_color(&mut db, c.id, Some("#7ee787")).unwrap();
        assert_eq!(updated.name, "Onboarding");
        assert_eq!(updated.icon.as_deref(), Some("folder"));
        assert_eq!(updated.doc_count, 1);
    }

    // -------- Slice 25: reorder_collections --------

    #[test]
    fn reorder_collections_rewrites_sort_order_to_new_sequence() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "A", None, None).unwrap();
        let b = create_collection(&mut db, "B", None, None).unwrap();
        let c = create_collection(&mut db, "C", None, None).unwrap();
        // Default order is creation order: A, B, C.
        let listed: Vec<i64> = list_collections(&db)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(listed, vec![a.id, b.id, c.id]);
        // Reorder to C, A, B.
        let moved = reorder_collections(&mut db, &[c.id, a.id, b.id]).unwrap();
        // All three rows moved off their old defaults.
        assert_eq!(moved, 3);
        let after: Vec<i64> = list_collections(&db)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(after, vec![c.id, a.id, b.id]);
    }

    #[test]
    fn reorder_collections_returns_count_of_actual_moves() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "A", None, None).unwrap();
        let b = create_collection(&mut db, "B", None, None).unwrap();
        // First reorder: both rows move from defaults 0/1 -> 100/200.
        assert_eq!(reorder_collections(&mut db, &[a.id, b.id]).unwrap(), 2);
        // Same reorder again: zero moves because sort_order is already 100/200.
        assert_eq!(reorder_collections(&mut db, &[a.id, b.id]).unwrap(), 0);
    }

    #[test]
    fn reorder_collections_uses_stepped_sort_order_for_future_inserts() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "A", None, None).unwrap();
        let b = create_collection(&mut db, "B", None, None).unwrap();
        reorder_collections(&mut db, &[a.id, b.id]).unwrap();
        // Step is 100 so a later splice can insert at 150 without rounding.
        let so: Vec<i64> = list_collections(&db)
            .unwrap()
            .into_iter()
            .map(|c| c.sort_order)
            .collect();
        assert_eq!(so, vec![100, 200]);
    }

    #[test]
    fn reorder_collections_silently_skips_unknown_ids() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "A", None, None).unwrap();
        let b = create_collection(&mut db, "B", None, None).unwrap();
        // Throwing a bogus id in the middle shouldn't crash and shouldn't
        // hurt the surviving rows — the survivors land at positions 1 and 3
        // (100 and 300), with the unknown id at position 2 (skipped).
        let moved = reorder_collections(&mut db, &[a.id, 999_999, b.id]).unwrap();
        assert_eq!(moved, 2);
        let so: Vec<(i64, i64)> = list_collections(&db)
            .unwrap()
            .into_iter()
            .map(|c| (c.id, c.sort_order))
            .collect();
        assert_eq!(so, vec![(a.id, 100), (b.id, 300)]);
    }

    #[test]
    fn reorder_collections_leaves_unnamed_rows_alone() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "A", None, None).unwrap();
        let b = create_collection(&mut db, "B", None, None).unwrap();
        let c = create_collection(&mut db, "C", None, None).unwrap();
        // Pin original defaults so we can detect drift on c.
        let original_c = get_collection(&db, c.id).unwrap();
        // Reorder a subset (just b then a, c untouched).
        reorder_collections(&mut db, &[b.id, a.id]).unwrap();
        let after_c = get_collection(&db, c.id).unwrap();
        assert_eq!(after_c.sort_order, original_c.sort_order);
        // b lands at 100, a at 200, c keeps its old default (2 from creation).
        let so: Vec<(i64, i64)> = list_collections(&db)
            .unwrap()
            .into_iter()
            .map(|c| (c.id, c.sort_order))
            .collect();
        // List is ordered by sort_order ASC; c had sort_order=2 (still in
        // the original sequence), and b=100, a=200. So display is c,b,a.
        assert_eq!(so, vec![(c.id, 2), (b.id, 100), (a.id, 200)]);
    }

    #[test]
    fn reorder_collections_empty_slice_is_noop() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let _a = create_collection(&mut db, "A", None, None).unwrap();
        assert_eq!(reorder_collections(&mut db, &[]).unwrap(), 0);
    }

    // -------- Slice 26: duplicate_collection --------

    #[test]
    fn duplicate_collection_clones_name_icon_color_and_docs() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Tax 2026", Some("folder"), Some("#7ee787")).unwrap();
        let d1 = make_doc(&mut db, "intro");
        let d2 = make_doc(&mut db, "policy");
        add_docs(&mut db, c.id, &[d1, d2]).unwrap();
        let dup = duplicate_collection(&mut db, c.id).unwrap();
        assert_ne!(dup.id, c.id);
        assert_eq!(dup.name, "Tax 2026 (copy)");
        assert_eq!(dup.icon.as_deref(), Some("folder"));
        assert_eq!(dup.color.as_deref(), Some("#7ee787"));
        // doc_count reflects the cloned membership without an extra round-trip.
        assert_eq!(dup.doc_count, 2);
        // Source collection is untouched.
        let src_after = get_collection(&db, c.id).unwrap();
        assert_eq!(src_after.name, "Tax 2026");
        assert_eq!(src_after.doc_count, 2);
        // Both collections list the same doc set.
        let src_docs = list_collection_docs(&db, c.id).unwrap();
        let dup_docs = list_collection_docs(&db, dup.id).unwrap();
        assert_eq!(src_docs.len(), 2);
        assert_eq!(dup_docs.len(), 2);
    }

    #[test]
    fn duplicate_collection_auto_suffixes_on_collision() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Onboarding", None, None).unwrap();
        let first = duplicate_collection(&mut db, c.id).unwrap();
        assert_eq!(first.name, "Onboarding (copy)");
        let second = duplicate_collection(&mut db, c.id).unwrap();
        // Source name is still "Onboarding"; second clone goes to (copy 2)
        // because (copy) is already taken.
        assert_eq!(second.name, "Onboarding (copy 2)");
        let third = duplicate_collection(&mut db, c.id).unwrap();
        assert_eq!(third.name, "Onboarding (copy 3)");
        // Duplicating the FIRST clone takes a fresh slot: "(copy) (copy)".
        let chain = duplicate_collection(&mut db, first.id).unwrap();
        assert_eq!(chain.name, "Onboarding (copy) (copy)");
    }

    #[test]
    fn duplicate_collection_lands_at_end_of_sort_order() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let a = create_collection(&mut db, "A", None, None).unwrap();
        let b = create_collection(&mut db, "B", None, None).unwrap();
        let dup = duplicate_collection(&mut db, a.id).unwrap();
        // Existing rows kept their default sort_order (0/1); the clone lands
        // at MAX + 1 = 2, so list order is A, B, A (copy).
        let listed: Vec<i64> = list_collections(&db)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(listed, vec![a.id, b.id, dup.id]);
    }

    #[test]
    fn duplicate_collection_with_no_docs_returns_empty_copy() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let c = create_collection(&mut db, "Empty", None, None).unwrap();
        let dup = duplicate_collection(&mut db, c.id).unwrap();
        assert_eq!(dup.doc_count, 0);
        assert_eq!(list_collection_docs(&db, dup.id).unwrap().len(), 0);
    }

    #[test]
    fn duplicate_collection_rejects_unknown_id() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        assert!(duplicate_collection(&mut db, 999_999).is_err());
    }

    #[test]
    fn duplicate_collection_truncates_long_source_name_to_fit_suffix() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        // A source name right at the cap.
        let long: String = "y".repeat(MAX_COLLECTION_NAME_LEN);
        let c = create_collection(&mut db, &long, None, None).unwrap();
        let dup = duplicate_collection(&mut db, c.id).unwrap();
        // The clone fits under the cap because we truncated the source
        // portion before appending " (copy)" — never the suffix.
        assert!(dup.name.chars().count() <= MAX_COLLECTION_NAME_LEN);
        assert!(dup.name.ends_with(" (copy)"));
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
    fn smart_collection_update_roundtrip() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let sc = create_smart_collection(
            &mut db,
            &NewSmartCollection {
                name: "Old name".into(),
                icon: Some("star".into()),
                color: Some("#aabbcc".into()),
                filter: LibraryFilter::default(),
            },
        )
        .unwrap();

        let new_filter = LibraryFilter {
            title_substring: Some("invoice".into()),
            ..LibraryFilter::default()
        };
        let updated = update_smart_collection(
            &mut db,
            sc.id,
            Some("Invoices"),
            Some(Some("folder")),
            Some(Some("#ff8800")),
            Some(&new_filter),
        )
        .unwrap();
        assert_eq!(updated.name, "Invoices");
        assert_eq!(updated.icon.as_deref(), Some("folder"));
        assert_eq!(updated.color.as_deref(), Some("#ff8800"));

        let after = get_smart_collection(&db, sc.id).unwrap();
        let parsed: LibraryFilter = serde_json::from_str(&after.query_json).unwrap();
        assert_eq!(parsed.title_substring.as_deref(), Some("invoice"));

        // Clearing icon to NULL.
        update_smart_collection(&mut db, sc.id, None, Some(None), None, None).unwrap();
        let after2 = get_smart_collection(&db, sc.id).unwrap();
        assert!(after2.icon.is_none());
        // Name preserved.
        assert_eq!(after2.name, "Invoices");
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
