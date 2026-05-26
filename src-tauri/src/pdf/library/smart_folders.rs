//! v3.37.0 — Atlas Smart Folders Hub.
//!
//! Merges built-in [`PresetInfo`](super::presets::PresetInfo) entries with
//! user-saved [`PersonalPresetRecord`](super::personal_presets::PersonalPresetRecord)
//! entries into one ordered, pinnable list backed by the
//! `library_smart_folder_order` table (schema v6).
//!
//! Surface API (consumed by `slab_smart_folders_*` Tauri commands):
//! - [`list_smart_folders`] — merged list, sorted by (pinned, sort_order, name).
//! - [`set_order`]          — atomic reorder for the visible UI list.
//! - [`set_pinned`]         — toggle the pin flag for a single entry.

use serde::{Deserialize, Serialize};

use super::personal_presets::list_personal_presets;
use super::presets::list_presets;
use super::registry::{LibraryDb, LibraryError};

/// Display row for the hub. Stable across kind so the Svelte side can render
/// every entry with one component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmartFolderEntry {
    /// `"builtin"` or `"personal"`.
    pub kind: String,
    /// Built-in preset id (`"invoices"`) OR personal preset row id as text.
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub description: String,
    pub pinned: bool,
    pub sort_order: i64,
}

/// Reorder spec passed to [`set_order`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub kind: String,
    pub id: String,
    pub sort_order: i64,
}

/// Return the merged list of every visible smart folder, in display order.
///
/// Sort key: pinned-first, then persisted `sort_order`, then alphabetical.
/// Entries without a persisted order get a synthetic key — built-ins start
/// at 1_000, personal presets at 2_000 — so first-launch produces a stable,
/// predictable layout (built-ins above personal).
pub fn list_smart_folders(db: &LibraryDb) -> Result<Vec<SmartFolderEntry>, LibraryError> {
    let builtins = list_presets();
    let personal = list_personal_presets(db)?;

    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT entry_kind, entry_id, sort_order, pinned
           FROM library_smart_folder_order",
    )?;
    let rows: Vec<(String, String, i64, bool)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? != 0,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let lookup = |kind: &str, id: &str| -> (i64, bool) {
        rows.iter()
            .find(|(k, i, _, _)| k == kind && i == id)
            .map(|(_, _, s, p)| (*s, *p))
            .unwrap_or((i64::MAX, false))
    };

    let mut out: Vec<SmartFolderEntry> = Vec::with_capacity(builtins.len() + personal.len());

    for (idx, b) in builtins.iter().enumerate() {
        let (order, pinned) = lookup("builtin", &b.id);
        out.push(SmartFolderEntry {
            kind: "builtin".into(),
            id: b.id.clone(),
            name: b.name.clone(),
            icon: b.icon.clone(),
            color: b.color.clone(),
            description: b.description.clone(),
            pinned,
            sort_order: if order == i64::MAX {
                1_000 + idx as i64
            } else {
                order
            },
        });
    }

    for p in &personal {
        let id_str = p.id.to_string();
        let (order, pinned) = lookup("personal", &id_str);
        out.push(SmartFolderEntry {
            kind: "personal".into(),
            id: id_str,
            name: p.name.clone(),
            icon: p.icon.clone().unwrap_or_else(|| "star".into()),
            color: p.color.clone().unwrap_or_else(|| "#888888".into()),
            description: p.description.clone().unwrap_or_default(),
            pinned,
            sort_order: if order == i64::MAX {
                2_000 + p.sort_order
            } else {
                order
            },
        });
    }

    out.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(a.sort_order.cmp(&b.sort_order))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(out)
}

/// Persist a new display order. The caller passes the FULL visible list;
/// each entry's `sort_order` is its zero-based position. Atomic.
pub fn set_order(db: &mut LibraryDb, items: &[OrderItem]) -> Result<(), LibraryError> {
    let tx = db.conn_mut().transaction()?;
    for it in items {
        tx.execute(
            "INSERT INTO library_smart_folder_order (entry_kind, entry_id, sort_order, pinned)
             VALUES (?1, ?2, ?3,
                     COALESCE((SELECT pinned FROM library_smart_folder_order
                               WHERE entry_kind=?1 AND entry_id=?2), 0))
             ON CONFLICT(entry_kind, entry_id)
             DO UPDATE SET sort_order = excluded.sort_order",
            rusqlite::params![it.kind, it.id, it.sort_order],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Toggle the pin flag on a single entry. No-op if the row's already in the
/// desired state.
pub fn set_pinned(
    db: &mut LibraryDb,
    kind: &str,
    id: &str,
    pinned: bool,
) -> Result<(), LibraryError> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO library_smart_folder_order (entry_kind, entry_id, sort_order, pinned)
         VALUES (?1, ?2,
                 COALESCE((SELECT sort_order FROM library_smart_folder_order
                           WHERE entry_kind=?1 AND entry_id=?2), 0),
                 ?3)
         ON CONFLICT(entry_kind, entry_id)
         DO UPDATE SET pinned = excluded.pinned",
        rusqlite::params![kind, id, pinned as i64],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::personal_presets::{save_personal_preset, NewPersonalPreset};
    use crate::pdf::library::query::{FilterClause, FilterCombinator, FilterGroup, LibraryFilter};

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

    fn add_personal(db: &mut LibraryDb, name: &str) -> i64 {
        save_personal_preset(
            db,
            &NewPersonalPreset {
                name: name.into(),
                icon: Some("star".into()),
                color: Some("#3b82f6".into()),
                description: Some("test".into()),
                filter: sample_filter(),
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn merged_list_returns_builtins_first_by_default() {
        let db = db();
        let entries = list_smart_folders(&db).unwrap();
        // Built-ins exist and come before any personal entries.
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.kind == "builtin"));
    }

    #[test]
    fn personal_presets_appear_after_builtins() {
        let mut db = db();
        add_personal(&mut db, "My Tax Stuff");
        let entries = list_smart_folders(&db).unwrap();
        let builtin_count = entries.iter().filter(|e| e.kind == "builtin").count();
        let personal_count = entries.iter().filter(|e| e.kind == "personal").count();
        assert!(builtin_count >= 1);
        assert_eq!(personal_count, 1);
        // Default ordering: built-ins precede personal.
        assert_eq!(entries[builtin_count].kind, "personal");
        assert_eq!(entries[builtin_count].name, "My Tax Stuff");
    }

    #[test]
    fn reorder_persists_and_takes_effect() {
        let mut db = db();
        // Force two known built-ins to swap into a custom order.
        set_order(
            &mut db,
            &[
                OrderItem {
                    kind: "builtin".into(),
                    id: "invoices".into(),
                    sort_order: 0,
                },
                OrderItem {
                    kind: "builtin".into(),
                    id: "recently-added".into(),
                    sort_order: 1,
                },
            ],
        )
        .unwrap();
        let entries = list_smart_folders(&db).unwrap();
        assert_eq!(entries[0].id, "invoices");
        assert_eq!(entries[1].id, "recently-added");
    }

    #[test]
    fn pin_floats_entry_to_top() {
        let mut db = db();
        set_pinned(&mut db, "builtin", "scanned", true).unwrap();
        let entries = list_smart_folders(&db).unwrap();
        assert_eq!(entries[0].id, "scanned", "pinned entry must come first");
        assert!(entries[0].pinned);
    }

    #[test]
    fn unpin_returns_entry_to_natural_order() {
        let mut db = db();
        set_pinned(&mut db, "builtin", "scanned", true).unwrap();
        set_pinned(&mut db, "builtin", "scanned", false).unwrap();
        let entries = list_smart_folders(&db).unwrap();
        assert!(!entries.iter().any(|e| e.pinned));
    }

    #[test]
    fn personal_preset_can_be_pinned_above_builtins() {
        let mut db = db();
        let pid = add_personal(&mut db, "Pinned Personal");
        set_pinned(&mut db, "personal", &pid.to_string(), true).unwrap();
        let entries = list_smart_folders(&db).unwrap();
        assert_eq!(entries[0].kind, "personal");
        assert_eq!(entries[0].name, "Pinned Personal");
    }
}
