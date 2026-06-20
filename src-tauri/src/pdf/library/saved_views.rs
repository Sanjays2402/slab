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
}
