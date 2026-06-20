// v3.50.0 "Atlas Saved Views" — one-click restorable rail filters.
//
// Why this exists
// ---------------
// The tag-management rail (v3.39 → v3.49) grew rich: folders, multi-select
// tags, All/Any combinator, untagged toggle, sort mode. Power users
// repeatedly reach for the same combination ("Project Apollo invoices",
// "Tax 2025 untagged backlog") and rebuild it click-by-click each session.
// A saved view is a NAMED shortcut that one click restores the rail to
// exactly that state and re-runs the live filter.
//
// How this differs from neighboring concepts
// ------------------------------------------
// * `library_smart_collections` (v3.32) — OWNS a doc list expanded from a
//   saved query. A "thing" the user navigates INTO.
// * `library_personal_presets`  (v3.36) — RECIPE that materializes INTO a
//   new smart_collection on apply (one-shot bootstrap).
// * `library_saved_views`       (v3.50, this) — RESTORES rail state.
//   No persistent doc list, no materialization; just `set the rail to
//   THIS filter and re-query`. The cheapest layer of the three.
//
// Storage
// -------
// Single table `library_saved_views` (id, name UNIQUE, filter_json,
// created_at, sort_order). Filter is the full `LibraryFilter` blob —
// opaque to SQL, decoded with serde_json on read so the entire
// LibraryFilter / FilterGroup tree survives query-language schema bumps.
// Mirrors personal_presets' opacity contract.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::query::LibraryFilter;
use super::registry::{LibraryDb, LibraryError};

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn map_serde(e: serde_json::Error) -> LibraryError {
    LibraryError::Other(format!("json: {e}"))
}

/// Row of `library_saved_views` decoded for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedViewRecord {
    pub id: i64,
    pub name: String,
    /// Decoded so a one-click restore can hand the LibraryFilter straight
    /// to the rail without a second round-trip.
    pub filter: LibraryFilter,
    pub created_at: i64,
    pub sort_order: i64,
    /// Whether the user has pinned this view to the top of the rail.
    /// Pinned views sort above unpinned ones; ties within each group fall
    /// back to `sort_order` then alphabetical name. Defaults to `false` so
    /// every pre-v15 row silently reads as unpinned. v3.56.0 Atlas
    /// Saved-Views-Polish.
    #[serde(default)]
    pub pinned: bool,
}

/// Caller-supplied spec for `save_view`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSavedView {
    pub name: String,
    pub filter: LibraryFilter,
}

// -----------------------------------------------------------------
// CRUD
// -----------------------------------------------------------------

/// Persist a new saved view. Trims the name; empty name is rejected.
/// Duplicate names are rejected by the UNIQUE constraint on the column.
pub fn save_view(db: &mut LibraryDb, spec: &NewSavedView) -> Result<SavedViewRecord, LibraryError> {
    let trimmed = spec.name.trim();
    if trimmed.is_empty() {
        return Err(LibraryError::Other("view name cannot be empty".into()));
    }
    let json = serde_json::to_string(&spec.filter).map_err(map_serde)?;
    let now = now_unix();
    let conn = db.conn_mut();
    conn.execute(
        "INSERT INTO library_saved_views
            (name, filter_json, created_at, sort_order)
         VALUES (?1, ?2, ?3,
                 COALESCE((SELECT MAX(sort_order) + 1 FROM library_saved_views), 0))",
        rusqlite::params![trimmed, json, now],
    )?;
    let id = conn.last_insert_rowid();
    get_view(db, id)
}

pub fn get_view(db: &LibraryDb, id: i64) -> Result<SavedViewRecord, LibraryError> {
    let conn = db.conn();
    conn.query_row(
        "SELECT id, name, filter_json, created_at, sort_order, pinned
         FROM library_saved_views WHERE id = ?1",
        rusqlite::params![id],
        row_to_record,
    )
    .map_err(LibraryError::from)
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<SavedViewRecord> {
    let json: String = row.get(2)?;
    let filter: LibraryFilter = serde_json::from_str(&json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let pinned_int: i64 = row.get(5)?;
    Ok(SavedViewRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        filter,
        created_at: row.get(3)?,
        sort_order: row.get(4)?,
        pinned: pinned_int != 0,
    })
}

/// All views, pinned-first then by sort_order ASC then name ASC. The
/// pinned-first dimension makes the rail's most-used views stay anchored
/// at the top while ordinary views drift on insert order.
pub fn list_views(db: &LibraryDb) -> Result<Vec<SavedViewRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, filter_json, created_at, sort_order, pinned
         FROM library_saved_views
         ORDER BY pinned DESC, sort_order ASC, name ASC",
    )?;
    let rows = stmt
        .query_map([], row_to_record)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_view(db: &mut LibraryDb, id: i64) -> Result<(), LibraryError> {
    db.conn_mut().execute(
        "DELETE FROM library_saved_views WHERE id = ?1",
        rusqlite::params![id],
    )?;
    Ok(())
}

/// Rename a view. Trims the new name; empty rejected; an unchanged name
/// (post-trim) is a no-op returning the existing row; a name that collides
/// with another view is rejected by the UNIQUE constraint — the UPDATE
/// leaves the row untouched on failure (sqlite is atomic per statement).
pub fn rename_view(
    db: &mut LibraryDb,
    id: i64,
    new_name: &str,
) -> Result<SavedViewRecord, LibraryError> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(LibraryError::Other("view name cannot be empty".into()));
    }
    let existing = get_view(db, id)?;
    if existing.name == trimmed {
        return Ok(existing);
    }
    let conn = db.conn_mut();
    conn.execute(
        "UPDATE library_saved_views SET name = ?1 WHERE id = ?2",
        rusqlite::params![trimmed, id],
    )?;
    get_view(db, id)
}

/// Update only the saved filter for an existing view, preserving id, name,
/// created_at, and sort_order. The pre-v3.56 behaviour required the user to
/// delete-and-recreate to tweak a view — that lost the id (breaking any
/// stored references), shuffled sort_order, and reset `created_at`. With this
/// setter the user can re-pin the current rail state onto an existing view as
/// a single in-place edit. Errors on unknown id; no payload validation
/// because LibraryFilter is opaque JSON and the rail already builds the
/// shape (the deserialize on the wire side rejects malformed blobs first).
pub fn update_view_filter(
    db: &mut LibraryDb,
    id: i64,
    filter: &LibraryFilter,
) -> Result<SavedViewRecord, LibraryError> {
    let json = serde_json::to_string(filter).map_err(map_serde)?;
    // Confirm the row exists FIRST — UPDATE with no matching id silently
    // affects zero rows, and we want a hard error so the caller knows the
    // setter rejected (rather than thinking it landed and re-querying for
    // nothing). `get_view` returns Err on unknown id via the underlying
    // rusqlite QueryReturnedNoRows path.
    let _ = get_view(db, id)?;
    let conn = db.conn_mut();
    conn.execute(
        "UPDATE library_saved_views SET filter_json = ?1 WHERE id = ?2",
        rusqlite::params![json, id],
    )?;
    get_view(db, id)
}

/// Duplicate an existing saved view: copies the filter, gives the copy a
/// fresh sort_order at the bottom of the list, and derives a unique name by
/// appending " (copy)" / " (copy 2)" / … so the UNIQUE constraint on `name`
/// never bites. Errors on unknown id. The duplicate is independent — editing
/// it later does NOT affect the source.
pub fn duplicate_view(db: &mut LibraryDb, id: i64) -> Result<SavedViewRecord, LibraryError> {
    let source = get_view(db, id)?;
    let new_name = derive_copy_name(db, &source.name)?;
    save_view(
        db,
        &NewSavedView {
            name: new_name,
            filter: source.filter,
        },
    )
}

/// Find the first available "<name> (copy)" / "<name> (copy N)" variant that
/// doesn't collide with an existing view. Capped at 999 attempts as a
/// belt-and-suspenders guard against a pathological library where someone
/// has somehow accumulated a thousand copies of the same view — at that
/// point we surface a clear error rather than burn CPU forever.
fn derive_copy_name(db: &LibraryDb, source_name: &str) -> Result<String, LibraryError> {
    let existing: std::collections::HashSet<String> =
        list_views(db)?.into_iter().map(|v| v.name).collect();
    let first = format!("{source_name} (copy)");
    if !existing.contains(&first) {
        return Ok(first);
    }
    for n in 2..=999 {
        let candidate = format!("{source_name} (copy {n})");
        if !existing.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(LibraryError::Other(
        "too many copies of this view already exist".into(),
    ))
}

/// Toggle the pin flag on a saved view. The rail surfaces pinned views
/// above unpinned ones (see [`list_views`] for the ORDER BY). Idempotent:
/// setting the same value twice succeeds (SQLite reports rows matched,
/// not rows whose value changed). Errors on unknown id so the caller
/// learns when the row vanished mid-flight (e.g. another window deleted
/// it) instead of thinking the toggle landed.
pub fn set_view_pinned(
    db: &mut LibraryDb,
    id: i64,
    pinned: bool,
) -> Result<SavedViewRecord, LibraryError> {
    // Same get-then-update shape as update_view_filter — the UPDATE on a
    // missing id silently affects 0 rows; we want a hard error.
    let _ = get_view(db, id)?;
    let conn = db.conn_mut();
    conn.execute(
        "UPDATE library_saved_views SET pinned = ?1 WHERE id = ?2",
        rusqlite::params![pinned as i64, id],
    )?;
    get_view(db, id)
}

/// Atomically re-stamp `sort_order` for every view in the order the caller
/// supplied. The caller passes the full list of view ids; each id's
/// zero-based position becomes its new sort_order. Runs inside a single
/// transaction so either every row is restamped or none are — a partial
/// failure can't leave the rail mid-shuffle. Mirrors the
/// `smart_folders::set_order` and `set_collection_order` patterns we
/// already ship.
///
/// Errors if any id in the input is not in the table (caught BEFORE the
/// UPDATE loop runs, so the txn is rolled back without touching a single
/// row). Duplicate ids in the input are also rejected — a duplicate would
/// mean the UI thinks the same view is in two slots, which is never what
/// the caller intends. Empty input is a zero no-op (succeeds without
/// touching the table).
///
/// NOTE: this re-stamps every row by `id` directly; it intentionally does
/// NOT mutate the `pinned` flag (mirrors the smart_folders pattern — the
/// rail's pinned-first sort is preserved because pinned DESC stays the
/// dominant key in `list_views`).
pub fn reorder_views(db: &mut LibraryDb, order: &[i64]) -> Result<(), LibraryError> {
    if order.is_empty() {
        return Ok(());
    }
    // Reject duplicate ids up front — calling UPDATE twice for the same
    // id would silently keep only the second sort_order, hiding the bug.
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for &id in order {
        if !seen.insert(id) {
            return Err(LibraryError::Other(format!(
                "duplicate view id {id} in reorder list"
            )));
        }
    }
    // Verify every id exists BEFORE we open the write txn — a missing id
    // mid-loop would leave the txn open and we'd have to remember to roll
    // back. Cheaper to validate up front against the current id set.
    let existing: std::collections::HashSet<i64> = {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT id FROM library_saved_views")?;
        let rows: Vec<i64> = stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter().collect()
    };
    for &id in order {
        if !existing.contains(&id) {
            return Err(LibraryError::Other(format!(
                "unknown view id {id} in reorder list"
            )));
        }
    }
    let conn = db.conn_mut();
    let tx = conn.transaction()?;
    {
        let mut stmt =
            tx.prepare("UPDATE library_saved_views SET sort_order = ?1 WHERE id = ?2")?;
        for (position, &id) in order.iter().enumerate() {
            stmt.execute(rusqlite::params![position as i64, id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::query::{
        FilterClause, FilterCombinator, FilterGroup, SortBy, TagMatch,
    };

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().unwrap()
    }

    fn flat_filter() -> LibraryFilter {
        LibraryFilter {
            folder_id: Some(7),
            tag_ids: vec![1, 2, 3],
            tag_match: TagMatch::Any,
            title_substring: Some("invoice".into()),
            sort: SortBy::TitleAsc,
            ..Default::default()
        }
    }

    fn clause_filter() -> LibraryFilter {
        LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::Untagged, FilterClause::Folder { id: 4 }],
            }),
            ..Default::default()
        }
    }

    fn spec(name: &str, filter: LibraryFilter) -> NewSavedView {
        NewSavedView {
            name: name.into(),
            filter,
        }
    }

    #[test]
    fn save_and_get_roundtrips_flat_filter() {
        let mut db = db();
        let saved = save_view(&mut db, &spec("Apollo invoices", flat_filter())).unwrap();
        assert_eq!(saved.name, "Apollo invoices");
        assert_eq!(saved.filter.folder_id, Some(7));
        assert_eq!(saved.filter.tag_ids, vec![1, 2, 3]);
        assert_eq!(saved.filter.tag_match, TagMatch::Any);
        assert_eq!(saved.filter.title_substring.as_deref(), Some("invoice"));
        assert_eq!(saved.filter.sort, SortBy::TitleAsc);

        let fetched = get_view(&db, saved.id).unwrap();
        assert_eq!(fetched.id, saved.id);
        assert_eq!(fetched.name, saved.name);
        assert_eq!(fetched.filter.folder_id, Some(7));
    }

    #[test]
    fn save_and_get_roundtrips_clause_tree() {
        let mut db = db();
        let saved = save_view(&mut db, &spec("Untagged backlog", clause_filter())).unwrap();
        assert!(saved.filter.clauses.is_some());
        let group = saved.filter.clauses.unwrap();
        assert_eq!(group.combinator, FilterCombinator::And);
        assert_eq!(group.clauses.len(), 2);
        // Order survives the round-trip too.
        assert!(matches!(group.clauses[0], FilterClause::Untagged));
        assert!(matches!(group.clauses[1], FilterClause::Folder { id: 4 }));
    }

    #[test]
    fn save_trims_name() {
        let mut db = db();
        let saved = save_view(&mut db, &spec("   Trim me   ", flat_filter())).unwrap();
        assert_eq!(saved.name, "Trim me");
    }

    #[test]
    fn save_rejects_empty_name() {
        let mut db = db();
        let err = save_view(&mut db, &spec("   ", flat_filter())).unwrap_err();
        assert!(format!("{err}").contains("empty"));
        assert_eq!(list_views(&db).unwrap().len(), 0);
    }

    #[test]
    fn duplicate_name_violates_unique_constraint() {
        let mut db = db();
        save_view(&mut db, &spec("Dup", flat_filter())).unwrap();
        let err = save_view(&mut db, &spec("Dup", clause_filter())).unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("unique"));
        // The failed insert didn't pollute the table.
        assert_eq!(list_views(&db).unwrap().len(), 1);
    }

    #[test]
    fn list_orders_by_sort_order_then_name() {
        let mut db = db();
        // sort_order is monotonically MAX+1 so insert order == sort order.
        save_view(&mut db, &spec("A", flat_filter())).unwrap();
        save_view(&mut db, &spec("B", flat_filter())).unwrap();
        save_view(&mut db, &spec("C", flat_filter())).unwrap();
        let names: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn delete_removes_only_target_row() {
        let mut db = db();
        let a = save_view(&mut db, &spec("Keep1", flat_filter())).unwrap();
        let b = save_view(&mut db, &spec("Doomed", flat_filter())).unwrap();
        let c = save_view(&mut db, &spec("Keep2", flat_filter())).unwrap();
        delete_view(&mut db, b.id).unwrap();
        let names: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, vec!["Keep1", "Keep2"]);
        assert!(get_view(&db, b.id).is_err());
        // Neighboring rows survived.
        assert!(get_view(&db, a.id).is_ok());
        assert!(get_view(&db, c.id).is_ok());
    }

    #[test]
    fn delete_unknown_id_is_noop() {
        let mut db = db();
        save_view(&mut db, &spec("Solo", flat_filter())).unwrap();
        // No row matches id 9999; DELETE affects 0 rows, no error.
        delete_view(&mut db, 9999).unwrap();
        assert_eq!(list_views(&db).unwrap().len(), 1);
    }

    #[test]
    fn rename_updates_name_only() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Old", flat_filter())).unwrap();
        let renamed = rename_view(&mut db, v.id, "New").unwrap();
        assert_eq!(renamed.id, v.id);
        assert_eq!(renamed.name, "New");
        assert_eq!(renamed.created_at, v.created_at);
        assert_eq!(renamed.sort_order, v.sort_order);
        // Filter is byte-identical (same JSON in storage).
        assert_eq!(renamed.filter.folder_id, v.filter.folder_id);
        assert_eq!(renamed.filter.tag_ids, v.filter.tag_ids);
    }

    #[test]
    fn rename_trims() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Old", flat_filter())).unwrap();
        let renamed = rename_view(&mut db, v.id, "   New   ").unwrap();
        assert_eq!(renamed.name, "New");
    }

    #[test]
    fn rename_same_name_is_noop() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Same", flat_filter())).unwrap();
        // Trimmed equality short-circuits without touching the row.
        let again = rename_view(&mut db, v.id, "   Same   ").unwrap();
        assert_eq!(again.id, v.id);
        assert_eq!(again.name, "Same");
    }

    #[test]
    fn rename_empty_is_rejected() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Untouched", flat_filter())).unwrap();
        let err = rename_view(&mut db, v.id, "   ").unwrap_err();
        assert!(format!("{err}").contains("empty"));
        // The original row is intact.
        let still = get_view(&db, v.id).unwrap();
        assert_eq!(still.name, "Untouched");
    }

    #[test]
    fn rename_unknown_id_is_rejected() {
        let mut db = db();
        let err = rename_view(&mut db, 9999, "Whatever").unwrap_err();
        // get_view on the unknown id returns a sqlite NotFound mapped
        // through LibraryError::Sqlite — the message contains "QueryReturnedNoRows"
        // on rusqlite or similar — either way it's an error, not a fake row.
        let _ = err;
        assert_eq!(list_views(&db).unwrap().len(), 0);
    }

    #[test]
    fn rename_collision_with_existing_name_is_rejected() {
        let mut db = db();
        save_view(&mut db, &spec("Taken", flat_filter())).unwrap();
        let mover = save_view(&mut db, &spec("Mover", flat_filter())).unwrap();
        let err = rename_view(&mut db, mover.id, "Taken").unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("unique"));
        // Both rows still exist with their original names — the UPDATE
        // failed atomically so "Mover" is unchanged.
        let names: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert!(names.contains(&"Taken".to_string()));
        assert!(names.contains(&"Mover".to_string()));
    }

    #[test]
    fn empty_db_lists_zero_views() {
        let db = db();
        assert_eq!(list_views(&db).unwrap().len(), 0);
    }

    #[test]
    fn restored_filter_is_byte_for_byte() {
        // The whole point: serialize a filter, store it, list it, and the
        // decoded LibraryFilter must serialize to the SAME JSON. This is
        // what makes "one-click restore" actually reproduce the doc set.
        let mut db = db();
        let original = flat_filter();
        let original_json = serde_json::to_string(&original).unwrap();
        save_view(&mut db, &spec("Stable", original.clone())).unwrap();
        let listed = &list_views(&db).unwrap()[0];
        let restored_json = serde_json::to_string(&listed.filter).unwrap();
        assert_eq!(original_json, restored_json);
    }

    #[test]
    fn restored_clause_filter_is_byte_for_byte() {
        let mut db = db();
        let original = clause_filter();
        let original_json = serde_json::to_string(&original).unwrap();
        save_view(&mut db, &spec("StableClauses", original.clone())).unwrap();
        let listed = &list_views(&db).unwrap()[0];
        let restored_json = serde_json::to_string(&listed.filter).unwrap();
        assert_eq!(original_json, restored_json);
    }

    // -----------------------------------------------------------------
    // v3.56.0 Atlas Saved-Views-Polish — slice 38: update_view_filter
    // -----------------------------------------------------------------

    #[test]
    fn update_view_filter_swaps_filter_only() {
        let mut db = db();
        let original = save_view(&mut db, &spec("Pinned", flat_filter())).unwrap();
        // Replace with a totally different shape (clause tree instead of
        // flat) — id / name / created_at / sort_order must all survive.
        let updated = update_view_filter(&mut db, original.id, &clause_filter()).unwrap();
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.name, "Pinned");
        assert_eq!(updated.created_at, original.created_at);
        assert_eq!(updated.sort_order, original.sort_order);
        // Filter swapped — confirm by serializing both ends.
        let updated_json = serde_json::to_string(&updated.filter).unwrap();
        let target_json = serde_json::to_string(&clause_filter()).unwrap();
        assert_eq!(updated_json, target_json);
    }

    #[test]
    fn update_view_filter_unknown_id_is_rejected() {
        let mut db = db();
        let err = update_view_filter(&mut db, 9999, &flat_filter()).unwrap_err();
        // Wraps the rusqlite NotFound — we just need confirmation that the
        // setter refused (rather than silently no-op'ing).
        let _ = err;
        assert_eq!(list_views(&db).unwrap().len(), 0);
    }

    #[test]
    fn update_view_filter_does_not_touch_other_rows() {
        let mut db = db();
        let a = save_view(&mut db, &spec("A", flat_filter())).unwrap();
        let b = save_view(&mut db, &spec("B", flat_filter())).unwrap();
        update_view_filter(&mut db, a.id, &clause_filter()).unwrap();
        // B's filter is untouched.
        let b_after = get_view(&db, b.id).unwrap();
        let b_json = serde_json::to_string(&b_after.filter).unwrap();
        let flat_json = serde_json::to_string(&flat_filter()).unwrap();
        assert_eq!(b_json, flat_json);
        assert_eq!(b_after.name, "B");
    }

    // -----------------------------------------------------------------
    // v3.56.0 Atlas Saved-Views-Polish — slice 39: duplicate_view
    // -----------------------------------------------------------------

    #[test]
    fn duplicate_view_first_copy_uses_copy_suffix() {
        let mut db = db();
        let src = save_view(&mut db, &spec("Apollo invoices", flat_filter())).unwrap();
        let dup = duplicate_view(&mut db, src.id).unwrap();
        assert_eq!(dup.name, "Apollo invoices (copy)");
        // Filter survives the duplication byte-for-byte.
        let src_json = serde_json::to_string(&src.filter).unwrap();
        let dup_json = serde_json::to_string(&dup.filter).unwrap();
        assert_eq!(src_json, dup_json);
        // Distinct id, fresh sort_order at the bottom.
        assert_ne!(dup.id, src.id);
        assert!(dup.sort_order > src.sort_order);
    }

    #[test]
    fn duplicate_view_walks_to_copy_2_when_copy_1_taken() {
        let mut db = db();
        let src = save_view(&mut db, &spec("Source", flat_filter())).unwrap();
        // Manually park a "Source (copy)" so the next duplicate must land at
        // "Source (copy 2)".
        save_view(&mut db, &spec("Source (copy)", flat_filter())).unwrap();
        let dup = duplicate_view(&mut db, src.id).unwrap();
        assert_eq!(dup.name, "Source (copy 2)");
    }

    #[test]
    fn duplicate_view_chain_keeps_walking_to_copy_3() {
        let mut db = db();
        let src = save_view(&mut db, &spec("Chain", flat_filter())).unwrap();
        let a = duplicate_view(&mut db, src.id).unwrap();
        assert_eq!(a.name, "Chain (copy)");
        let b = duplicate_view(&mut db, src.id).unwrap();
        assert_eq!(b.name, "Chain (copy 2)");
        let c = duplicate_view(&mut db, src.id).unwrap();
        assert_eq!(c.name, "Chain (copy 3)");
        // The 3 copies + the source — 4 total rows.
        assert_eq!(list_views(&db).unwrap().len(), 4);
    }

    #[test]
    fn duplicate_view_is_independent_from_source() {
        let mut db = db();
        let src = save_view(&mut db, &spec("Master", flat_filter())).unwrap();
        let dup = duplicate_view(&mut db, src.id).unwrap();
        // Edit the SOURCE — the duplicate must NOT follow along.
        update_view_filter(&mut db, src.id, &clause_filter()).unwrap();
        let dup_after = get_view(&db, dup.id).unwrap();
        let dup_json = serde_json::to_string(&dup_after.filter).unwrap();
        let flat_json = serde_json::to_string(&flat_filter()).unwrap();
        assert_eq!(dup_json, flat_json);
    }

    #[test]
    fn duplicate_view_unknown_id_is_rejected() {
        let mut db = db();
        let err = duplicate_view(&mut db, 9999).unwrap_err();
        let _ = err;
        assert_eq!(list_views(&db).unwrap().len(), 0);
    }

    // -----------------------------------------------------------------
    // v3.56.0 Atlas Saved-Views-Polish — slice 40: set_view_pinned
    // -----------------------------------------------------------------

    #[test]
    fn fresh_view_is_unpinned_by_default() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Fresh", flat_filter())).unwrap();
        assert!(!v.pinned, "fresh saved views must default to unpinned");
    }

    #[test]
    fn set_view_pinned_toggles_round_trip() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Toggle", flat_filter())).unwrap();
        let pinned = set_view_pinned(&mut db, v.id, true).unwrap();
        assert!(pinned.pinned);
        // Other fields are untouched.
        assert_eq!(pinned.id, v.id);
        assert_eq!(pinned.name, "Toggle");
        assert_eq!(pinned.created_at, v.created_at);
        assert_eq!(pinned.sort_order, v.sort_order);

        let unpinned = set_view_pinned(&mut db, v.id, false).unwrap();
        assert!(!unpinned.pinned);
    }

    #[test]
    fn set_view_pinned_is_idempotent() {
        let mut db = db();
        let v = save_view(&mut db, &spec("Twice", flat_filter())).unwrap();
        let a = set_view_pinned(&mut db, v.id, true).unwrap();
        let b = set_view_pinned(&mut db, v.id, true).unwrap();
        // Both calls succeed and report the same final state.
        assert!(a.pinned);
        assert!(b.pinned);
    }

    #[test]
    fn set_view_pinned_unknown_id_is_rejected() {
        let mut db = db();
        let err = set_view_pinned(&mut db, 9999, true).unwrap_err();
        let _ = err;
        assert_eq!(list_views(&db).unwrap().len(), 0);
    }

    #[test]
    fn list_views_surfaces_pinned_first() {
        let mut db = db();
        // Insert order: A, B, C. By default they'd surface A, B, C.
        let _a = save_view(&mut db, &spec("A", flat_filter())).unwrap();
        let b = save_view(&mut db, &spec("B", flat_filter())).unwrap();
        let _c = save_view(&mut db, &spec("C", flat_filter())).unwrap();
        // Pin B — it must surface FIRST despite having a higher sort_order
        // than A (the pinned dimension dominates the ORDER BY).
        set_view_pinned(&mut db, b.id, true).unwrap();
        let names: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, vec!["B", "A", "C"]);
    }

    #[test]
    fn list_views_two_pinned_keep_their_relative_order() {
        let mut db = db();
        let a = save_view(&mut db, &spec("A", flat_filter())).unwrap();
        let _b = save_view(&mut db, &spec("B", flat_filter())).unwrap();
        let c = save_view(&mut db, &spec("C", flat_filter())).unwrap();
        // Pin A and C — within the pinned group their relative order
        // falls back to sort_order ASC (A's sort_order < C's), so the
        // expected listing is A, C, B.
        set_view_pinned(&mut db, a.id, true).unwrap();
        set_view_pinned(&mut db, c.id, true).unwrap();
        let names: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, vec!["A", "C", "B"]);
    }

    #[test]
    fn pin_legacy_json_without_pinned_deserialises_as_false() {
        // Pre-v3.56 SavedViewRecord JSON didn't carry the `pinned` field.
        // The serde default keeps backwards compat — the rail can decode
        // legacy snapshots without choking on the missing field.
        let legacy = r#"{
            "id": 1,
            "name": "Old",
            "filter": {"folder_id": null, "tag_ids": [], "tag_match": "all",
                       "title_substring": null, "sort": "added_desc"},
            "created_at": 0,
            "sort_order": 0
        }"#;
        let parsed: SavedViewRecord = serde_json::from_str(legacy).unwrap();
        assert!(!parsed.pinned);
    }

    // -----------------------------------------------------------------
    // v3.56.0 Atlas Saved-Views-Polish — slice 41: reorder_views
    // -----------------------------------------------------------------

    fn make_three(db: &mut LibraryDb) -> [SavedViewRecord; 3] {
        let a = save_view(db, &spec("A", flat_filter())).unwrap();
        let b = save_view(db, &spec("B", flat_filter())).unwrap();
        let c = save_view(db, &spec("C", flat_filter())).unwrap();
        [a, b, c]
    }

    #[test]
    fn reorder_views_restamps_sort_order_by_position() {
        let mut db = db();
        let [a, b, c] = make_three(&mut db);
        // Send the reverse order; sort_order must now be C=0, B=1, A=2 so
        // the list surfaces as C, B, A.
        reorder_views(&mut db, &[c.id, b.id, a.id]).unwrap();
        let listed = list_views(&db).unwrap();
        let names: Vec<_> = listed.iter().map(|v| v.name.clone()).collect();
        assert_eq!(names, vec!["C", "B", "A"]);
        // The new sort_order values match the input positions exactly.
        let order_for = |id: i64| listed.iter().find(|v| v.id == id).unwrap().sort_order;
        assert_eq!(order_for(c.id), 0);
        assert_eq!(order_for(b.id), 1);
        assert_eq!(order_for(a.id), 2);
    }

    #[test]
    fn reorder_views_empty_is_noop() {
        let mut db = db();
        let [_a, _b, _c] = make_three(&mut db);
        let before: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.sort_order))
            .collect();
        reorder_views(&mut db, &[]).unwrap();
        let after: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.sort_order))
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn reorder_views_duplicate_id_is_rejected_no_rows_touched() {
        let mut db = db();
        let [a, b, _c] = make_three(&mut db);
        let before: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.sort_order))
            .collect();
        let err = reorder_views(&mut db, &[a.id, b.id, a.id]).unwrap_err();
        assert!(format!("{err}").contains("duplicate"));
        // Atomic — every row's sort_order is exactly what it was before.
        let after: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.sort_order))
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn reorder_views_unknown_id_is_rejected_no_rows_touched() {
        let mut db = db();
        let [a, b, _c] = make_three(&mut db);
        let before: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.sort_order))
            .collect();
        let err = reorder_views(&mut db, &[a.id, 9999, b.id]).unwrap_err();
        assert!(format!("{err}").contains("unknown"));
        // Validation runs BEFORE the txn opens — no row mutated.
        let after: Vec<_> = list_views(&db)
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.sort_order))
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn reorder_views_does_not_mutate_pinned_or_filter() {
        let mut db = db();
        let [a, b, c] = make_three(&mut db);
        // Pin B before reordering.
        set_view_pinned(&mut db, b.id, true).unwrap();
        reorder_views(&mut db, &[c.id, b.id, a.id]).unwrap();
        // B's pin survives the reorder — the rail's "pinned-first" still
        // surfaces B above the others even though we shuffled sort_order.
        let listed = list_views(&db).unwrap();
        let b_after = listed.iter().find(|v| v.id == b.id).unwrap();
        assert!(b_after.pinned, "reorder must not clear the pin flag");
        // Filter shape on every row is byte-for-byte unchanged.
        for v in &listed {
            let restored = serde_json::to_string(&v.filter).unwrap();
            let target = serde_json::to_string(&flat_filter()).unwrap();
            assert_eq!(restored, target);
        }
        // And because pinned DESC dominates, B is FIRST regardless of the
        // sort_order shuffle we just applied.
        assert_eq!(listed[0].id, b.id);
    }

    #[test]
    fn reorder_views_subset_only_restamps_named_rows() {
        // The reorder is positional by id — sending a SUBSET of rows is
        // permitted (and useful when the rail wants to bubble a few items
        // to the top without touching the tail). Unmentioned rows keep
        // their pre-reorder sort_order, which can mean they end up
        // intermingled with the reordered ones — that's intentional, the
        // UI is expected to send the full list when it wants strict order.
        // This test pins down the documented "only restamps named ids"
        // behaviour so a future change can't regress it.
        let mut db = db();
        let [a, b, c] = make_three(&mut db);
        let pre_c_order = c.sort_order;
        reorder_views(&mut db, &[b.id, a.id]).unwrap();
        let listed = list_views(&db).unwrap();
        let order_for = |id: i64| listed.iter().find(|v| v.id == id).unwrap().sort_order;
        assert_eq!(order_for(b.id), 0);
        assert_eq!(order_for(a.id), 1);
        // C wasn't named, so its sort_order is unchanged.
        assert_eq!(order_for(c.id), pre_c_order);
    }
}
