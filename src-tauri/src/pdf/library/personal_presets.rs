// v3.36.0 "Atlas Personal Presets" — user-saved smart-collection recipes
// + portable `.slabpresets` JSON pack export/import.
//
// Why this exists
// ---------------
// v3.35 shipped six built-in presets ("Tax 2025", "Invoices", etc.) but
// every team has its own bespoke vocabulary. A 200-lawyer firm has
// "NDA awaiting countersign" and "Litigation hold — Project Apollo".
// Personal presets let the paralegal who built that smart collection
// click "Save as preset…" and have it appear in every teammate's
// preset picker via a one-file `.slabpresets` import.
//
// Storage
// -------
// One table `library_personal_presets` holding the same shape as
// built-in presets (name, icon, color, description) plus an opaque
// serialized `LibraryFilter` JSON blob. Filter shape evolves with
// the query language; presets keep working as long as serde_json
// can still decode them.
//
// Pack format
// -----------
// `.slabpresets` is a small JSON document:
//
//   { "version": 1,
//     "presets": [
//        { "name": "...", "icon": "...", "color": "...",
//          "description": "...", "filter": { ...LibraryFilter... } },
//        ...
//     ]
//   }
//
// Version 1 is the only version today; future bumps will support
// migration via a `From<PackV1>` / `From<PackV2>` chain.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::collections::{create_smart_collection, NewSmartCollection, SmartCollectionRecord};
use super::query::LibraryFilter;
use super::registry::{LibraryDb, LibraryError};

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Row of `library_personal_presets` as seen by Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalPresetRecord {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    /// Decoded so the frontend can preview / edit it.
    pub filter: LibraryFilter,
    pub created_at: i64,
    pub sort_order: i64,
}

/// Caller-supplied spec for `save_personal_preset`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPersonalPreset {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub filter: LibraryFilter,
}

/// Wire-format for a single preset inside a `.slabpresets` pack.
/// Intentionally identical-shaped to `NewPersonalPreset` so the
/// import path is a trivial map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalPresetExport {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub filter: LibraryFilter,
}

/// `.slabpresets` v1 document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetPack {
    pub version: u32,
    pub presets: Vec<PersonalPresetExport>,
}

/// What to do when an imported preset's name already exists.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImportConflictPolicy {
    /// Don't touch the existing row, skip the incoming one.
    #[default]
    Skip,
    /// Append "(2)", "(3)"... until the name is unique, then insert.
    Rename,
}

/// Summary returned to the UI after an import.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported: u32,
    pub skipped: u32,
    pub renamed: u32,
    pub errors: Vec<String>,
}

// -----------------------------------------------------------------
// CRUD
// -----------------------------------------------------------------

fn map_serde(e: serde_json::Error) -> LibraryError {
    LibraryError::Other(format!("json: {e}"))
}

pub fn save_personal_preset(
    db: &mut LibraryDb,
    spec: &NewPersonalPreset,
) -> Result<PersonalPresetRecord, LibraryError> {
    let trimmed = spec.name.trim();
    if trimmed.is_empty() {
        return Err(LibraryError::Other("preset name cannot be empty".into()));
    }
    let json = serde_json::to_string(&spec.filter).map_err(map_serde)?;
    let now = now_unix();
    let conn = db.conn_mut();
    conn.execute(
        "INSERT INTO library_personal_presets
            (name, icon, color, description, filter_json, created_at, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                 COALESCE((SELECT MAX(sort_order) + 1 FROM library_personal_presets), 0))",
        rusqlite::params![trimmed, spec.icon, spec.color, spec.description, json, now,],
    )?;
    let id = conn.last_insert_rowid();
    get_personal_preset(db, id)
}

pub fn get_personal_preset(db: &LibraryDb, id: i64) -> Result<PersonalPresetRecord, LibraryError> {
    let conn = db.conn();
    conn.query_row(
        "SELECT id, name, icon, color, description, filter_json, created_at, sort_order
         FROM library_personal_presets WHERE id = ?1",
        rusqlite::params![id],
        row_to_record,
    )
    .map_err(LibraryError::from)
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<PersonalPresetRecord> {
    let json: String = row.get(5)?;
    let filter: LibraryFilter = serde_json::from_str(&json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(PersonalPresetRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        color: row.get(3)?,
        description: row.get(4)?,
        filter,
        created_at: row.get(6)?,
        sort_order: row.get(7)?,
    })
}

pub fn list_personal_presets(db: &LibraryDb) -> Result<Vec<PersonalPresetRecord>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, icon, color, description, filter_json, created_at, sort_order
         FROM library_personal_presets
         ORDER BY sort_order ASC, name ASC",
    )?;
    let rows = stmt
        .query_map([], row_to_record)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_personal_preset(db: &mut LibraryDb, id: i64) -> Result<(), LibraryError> {
    db.conn_mut().execute(
        "DELETE FROM library_personal_presets WHERE id = ?1",
        rusqlite::params![id],
    )?;
    Ok(())
}

/// Materialize this personal preset into a real smart collection.
/// The collection name is "<preset.name>" — if that already exists,
/// we append " (copy)" / " (copy 2)" until unique. This matches the
/// behavior of `apply_preset` for built-ins.
pub fn apply_personal_preset(
    db: &mut LibraryDb,
    id: i64,
) -> Result<SmartCollectionRecord, LibraryError> {
    let preset = get_personal_preset(db, id)?;
    let name = unique_smart_collection_name(db, &preset.name)?;
    let spec = NewSmartCollection {
        name,
        icon: preset.icon,
        color: preset.color,
        filter: preset.filter,
    };
    create_smart_collection(db, &spec)
}

fn unique_smart_collection_name(db: &LibraryDb, base: &str) -> Result<String, LibraryError> {
    let existing: Vec<String> = db
        .conn()
        .prepare("SELECT name FROM library_smart_collections")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    if !existing.iter().any(|n| n == base) {
        return Ok(base.to_string());
    }
    let mut i = 1;
    loop {
        let candidate = if i == 1 {
            format!("{base} (copy)")
        } else {
            format!("{base} (copy {i})")
        };
        if !existing.iter().any(|n| n == &candidate) {
            return Ok(candidate);
        }
        i += 1;
        if i > 999 {
            return Err(LibraryError::Other(
                "could not find unique smart collection name".into(),
            ));
        }
    }
}

// -----------------------------------------------------------------
// Pack export / import
// -----------------------------------------------------------------

pub const PACK_VERSION: u32 = 1;

/// Export the given personal preset ids (or ALL if `ids` is empty) into
/// a pack. Returns the JSON string so the caller can either write it
/// to a path or pass it back to the frontend.
pub fn export_pack(db: &LibraryDb, ids: &[i64]) -> Result<String, LibraryError> {
    let all = list_personal_presets(db)?;
    let chosen: Vec<&PersonalPresetRecord> = if ids.is_empty() {
        all.iter().collect()
    } else {
        all.iter().filter(|p| ids.contains(&p.id)).collect()
    };
    let presets: Vec<PersonalPresetExport> = chosen
        .into_iter()
        .map(|p| PersonalPresetExport {
            name: p.name.clone(),
            icon: p.icon.clone(),
            color: p.color.clone(),
            description: p.description.clone(),
            filter: p.filter.clone(),
        })
        .collect();
    let pack = PresetPack {
        version: PACK_VERSION,
        presets,
    };
    serde_json::to_string_pretty(&pack).map_err(map_serde)
}

pub fn export_pack_to_path(db: &LibraryDb, ids: &[i64], path: &Path) -> Result<(), LibraryError> {
    let text = export_pack(db, ids)?;
    fs::write(path, text)?;
    Ok(())
}

/// Parse + import a pack from JSON text. Returns a report — never
/// throws on per-preset failure, only on top-level parse failure.
pub fn import_pack_from_str(
    db: &mut LibraryDb,
    text: &str,
    policy: ImportConflictPolicy,
) -> Result<ImportReport, LibraryError> {
    let pack: PresetPack = serde_json::from_str(text).map_err(map_serde)?;
    if pack.version != PACK_VERSION {
        return Err(LibraryError::Other(format!(
            "unsupported pack version {} (expected {PACK_VERSION})",
            pack.version
        )));
    }
    let mut report = ImportReport::default();
    let existing_names: Vec<String> = db
        .conn()
        .prepare("SELECT name FROM library_personal_presets")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    for incoming in pack.presets {
        let trimmed = incoming.name.trim();
        if trimmed.is_empty() {
            report.errors.push("preset with empty name skipped".into());
            continue;
        }
        let final_name = if existing_names.iter().any(|n| n == trimmed) {
            match policy {
                ImportConflictPolicy::Skip => {
                    report.skipped += 1;
                    continue;
                }
                ImportConflictPolicy::Rename => {
                    let renamed = next_free_name(trimmed, &existing_names);
                    report.renamed += 1;
                    renamed
                }
            }
        } else {
            trimmed.to_string()
        };
        let spec = NewPersonalPreset {
            name: final_name,
            icon: incoming.icon,
            color: incoming.color,
            description: incoming.description,
            filter: incoming.filter,
        };
        match save_personal_preset(db, &spec) {
            Ok(_) => report.imported += 1,
            Err(e) => report.errors.push(format!("{}: {e}", spec.name)),
        }
    }
    Ok(report)
}

pub fn import_pack_from_path(
    db: &mut LibraryDb,
    path: &Path,
    policy: ImportConflictPolicy,
) -> Result<ImportReport, LibraryError> {
    let text = fs::read_to_string(path)?;
    import_pack_from_str(db, &text, policy)
}

fn next_free_name(base: &str, taken: &[String]) -> String {
    let mut i = 2;
    loop {
        let candidate = format!("{base} ({i})");
        if !taken.iter().any(|n| n == &candidate) {
            return candidate;
        }
        i += 1;
        if i > 999 {
            return format!("{base} ({})", now_unix());
        }
    }
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::query::{FilterClause, FilterCombinator, FilterGroup};

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().unwrap()
    }

    fn sample_filter() -> LibraryFilter {
        LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::TitleContains {
                    value: "invoice".into(),
                }],
            }),
            ..Default::default()
        }
    }

    fn sample_spec(name: &str) -> NewPersonalPreset {
        NewPersonalPreset {
            name: name.into(),
            icon: Some("📌".into()),
            color: Some("#3b82f6".into()),
            description: Some("test preset".into()),
            filter: sample_filter(),
        }
    }

    #[test]
    fn save_and_get_roundtrips_filter() {
        let mut db = db();
        let saved = save_personal_preset(&mut db, &sample_spec("Foo")).unwrap();
        assert_eq!(saved.name, "Foo");
        let fetched = get_personal_preset(&db, saved.id).unwrap();
        assert_eq!(fetched.id, saved.id);
        assert_eq!(fetched.name, saved.name);
        assert_eq!(fetched.icon, saved.icon);
        // filter survived the round trip
        assert!(fetched.filter.clauses.is_some());
    }

    #[test]
    fn save_rejects_empty_name() {
        let mut db = db();
        let err = save_personal_preset(&mut db, &sample_spec("   ")).unwrap_err();
        assert!(format!("{err}").contains("empty"));
    }

    #[test]
    fn duplicate_name_violates_unique_constraint() {
        let mut db = db();
        save_personal_preset(&mut db, &sample_spec("Dup")).unwrap();
        let err = save_personal_preset(&mut db, &sample_spec("Dup")).unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("unique"));
    }

    #[test]
    fn list_returns_sorted_by_sort_order() {
        let mut db = db();
        save_personal_preset(&mut db, &sample_spec("A")).unwrap();
        save_personal_preset(&mut db, &sample_spec("B")).unwrap();
        save_personal_preset(&mut db, &sample_spec("C")).unwrap();
        let names: Vec<_> = list_personal_presets(&db)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn delete_removes_row() {
        let mut db = db();
        let p = save_personal_preset(&mut db, &sample_spec("Tmp")).unwrap();
        delete_personal_preset(&mut db, p.id).unwrap();
        assert!(get_personal_preset(&db, p.id).is_err());
    }

    #[test]
    fn apply_materializes_smart_collection() {
        let mut db = db();
        let p = save_personal_preset(&mut db, &sample_spec("Findme")).unwrap();
        let sc = apply_personal_preset(&mut db, p.id).unwrap();
        assert_eq!(sc.name, "Findme");
        assert!(sc.query_json.contains("invoice"));
    }

    #[test]
    fn apply_twice_appends_copy_suffix() {
        let mut db = db();
        let p = save_personal_preset(&mut db, &sample_spec("Twin")).unwrap();
        let a = apply_personal_preset(&mut db, p.id).unwrap();
        let b = apply_personal_preset(&mut db, p.id).unwrap();
        let c = apply_personal_preset(&mut db, p.id).unwrap();
        assert_eq!(a.name, "Twin");
        assert_eq!(b.name, "Twin (copy)");
        assert_eq!(c.name, "Twin (copy 2)");
    }

    #[test]
    fn export_then_import_is_lossless() {
        let mut db1 = db();
        save_personal_preset(&mut db1, &sample_spec("E1")).unwrap();
        save_personal_preset(&mut db1, &sample_spec("E2")).unwrap();
        let pack_json = export_pack(&db1, &[]).unwrap();
        assert!(pack_json.contains("\"version\": 1"));

        let mut db2 = db();
        let report =
            import_pack_from_str(&mut db2, &pack_json, ImportConflictPolicy::Skip).unwrap();
        assert_eq!(report.imported, 2);
        assert_eq!(report.skipped, 0);
        let names: Vec<_> = list_personal_presets(&db2)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["E1", "E2"]);
    }

    #[test]
    fn import_skip_policy_leaves_existing_alone() {
        let mut db = db();
        save_personal_preset(&mut db, &sample_spec("Keeper")).unwrap();
        let pack_json = export_pack(&db, &[]).unwrap();
        let report = import_pack_from_str(&mut db, &pack_json, ImportConflictPolicy::Skip).unwrap();
        assert_eq!(report.imported, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(list_personal_presets(&db).unwrap().len(), 1);
    }

    #[test]
    fn import_rename_policy_appends_numeric_suffix() {
        let mut db = db();
        save_personal_preset(&mut db, &sample_spec("Clash")).unwrap();
        let pack_json = export_pack(&db, &[]).unwrap();
        let report =
            import_pack_from_str(&mut db, &pack_json, ImportConflictPolicy::Rename).unwrap();
        assert_eq!(report.renamed, 1);
        let names: Vec<_> = list_personal_presets(&db)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert!(names.contains(&"Clash".to_string()));
        assert!(names.contains(&"Clash (2)".to_string()));
    }

    #[test]
    fn import_rejects_unknown_version() {
        let mut db = db();
        let bad = r#"{"version": 999, "presets": []}"#;
        let err = import_pack_from_str(&mut db, bad, ImportConflictPolicy::Skip).unwrap_err();
        assert!(format!("{err}").contains("unsupported"));
    }

    #[test]
    fn export_specific_ids_only() {
        let mut db = db();
        let a = save_personal_preset(&mut db, &sample_spec("A")).unwrap();
        save_personal_preset(&mut db, &sample_spec("B")).unwrap();
        let pack_json = export_pack(&db, &[a.id]).unwrap();
        let pack: PresetPack = serde_json::from_str(&pack_json).unwrap();
        assert_eq!(pack.presets.len(), 1);
        assert_eq!(pack.presets[0].name, "A");
    }
}
