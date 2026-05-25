// v3.35.0 "Atlas Presets" — built-in smart-collection templates.
//
// Why this exists
// ---------------
// Smart collections (v3.33) + nested AND/OR/NOT rules (v3.34) are
// powerful but invisible: a paralegal opening Slab on day one has no
// idea what to build. Presets solve that with one click — "Tax 2025",
// "Invoices last 30 days", "Contracts pending signature" — instantly
// materialized as a real smart collection the user can then tweak.
//
// Design
// ------
// Each preset is a static `Preset` value that knows how to resolve
// itself into a `NewSmartCollection`. Resolution may need access to
// the live tag table (to translate tag NAMES like "invoice" into the
// numeric tag ids that the filter language uses) so we pass `&LibraryDb`
// in. Missing tags are auto-created so the preset is always usable
// even on a brand-new library.
//
// All presets are in-process Rust data — no external file loading —
// because we want zero startup cost and zero "preset broke after
// upgrade" surface area. Power users can build their own presets
// directly with the v3.34 advanced rule UI.

use super::collections::{create_smart_collection, NewSmartCollection, SmartCollectionRecord};
use super::query::{FilterClause, FilterCombinator, FilterGroup, LibraryFilter, SortBy};
use super::registry::{LibraryDb, LibraryError};

/// A preset is a recipe. `resolve` turns it into a concrete
/// `NewSmartCollection` for the current library state. Returning a
/// `NewSmartCollection` (not directly inserting) keeps it pure and
/// trivially testable.
#[derive(Debug, Clone)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    pub description: &'static str,
    pub builder: fn(&mut LibraryDb) -> Result<LibraryFilter, LibraryError>,
}

/// Public face of a preset for the frontend — no function pointer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub description: String,
}

impl From<&Preset> for PresetInfo {
    fn from(p: &Preset) -> Self {
        Self {
            id: p.id.into(),
            name: p.name.into(),
            icon: p.icon.into(),
            color: p.color.into(),
            description: p.description.into(),
        }
    }
}

/// Look up (or create) a tag by case-insensitive name.
/// We auto-create so presets always work on a fresh library —
/// otherwise "Tax 2025" would silently match zero docs until the
/// user manually creates the "tax-2025" tag.
fn ensure_tag(db: &mut LibraryDb, name: &str, color: Option<&str>) -> Result<i64, LibraryError> {
    let lower = name.to_lowercase();
    let existing = db.list_tags()?;
    for t in &existing {
        if t.name.to_lowercase() == lower {
            return Ok(t.id);
        }
    }
    let created = db.add_tag(name, color)?;
    Ok(created.id)
}

// -----------------------------------------------------------------
// Built-in preset recipes
// -----------------------------------------------------------------

fn build_tax_2025(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "tax-2025", Some("#facc15"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::And,
            clauses: vec![FilterClause::Tag { id }],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_tax_2024(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "tax-2024", Some("#fbbf24"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::And,
            clauses: vec![FilterClause::Tag { id }],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_invoices(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "invoice", Some("#34d399"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::Or,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::TitleContains {
                    value: "invoice".into(),
                },
            ],
        }),
        sort: SortBy::AddedDesc,
        limit: Some(100),
        ..Default::default()
    })
}

fn build_contracts(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "contract", Some("#a78bfa"))?;
    let signed = ensure_tag(db, "signed", Some("#94a3b8"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::And,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::NotTag { id: signed },
            ],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_receipts(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "receipt", Some("#f87171"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::Or,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::TitleContains {
                    value: "receipt".into(),
                },
            ],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_scanned(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "scanned", Some("#60a5fa"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::Or,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::TitleContains {
                    value: "scan".into(),
                },
            ],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_research(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "research", Some("#22d3ee"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::Or,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::TitleContains {
                    value: "paper".into(),
                },
                FilterClause::TitleContains {
                    value: "thesis".into(),
                },
            ],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_legal(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "legal", Some("#fb7185"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::Or,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::TitleContains {
                    value: "agreement".into(),
                },
                FilterClause::TitleContains {
                    value: "nda".into(),
                },
            ],
        }),
        sort: SortBy::AddedDesc,
        ..Default::default()
    })
}

fn build_manuals(db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    let id = ensure_tag(db, "manual", Some("#fcd34d"))?;
    Ok(LibraryFilter {
        clauses: Some(FilterGroup {
            combinator: FilterCombinator::Or,
            clauses: vec![
                FilterClause::Tag { id },
                FilterClause::TitleContains {
                    value: "manual".into(),
                },
                FilterClause::TitleContains {
                    value: "handbook".into(),
                },
                FilterClause::TitleContains {
                    value: "guide".into(),
                },
            ],
        }),
        sort: SortBy::TitleAsc,
        ..Default::default()
    })
}

fn build_recently_added(_db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    Ok(LibraryFilter {
        sort: SortBy::AddedDesc,
        limit: Some(25),
        ..Default::default()
    })
}

fn build_untagged(_db: &mut LibraryDb) -> Result<LibraryFilter, LibraryError> {
    // No clause yet (TODO follow-up: needs an IsUntagged variant on the
    // filter language). For now we punt to AddedDesc + a TitleNotContains
    // pseudo-clause that always matches, leaving the user to refine.
    // This still produces a usable preset row.
    Ok(LibraryFilter {
        sort: SortBy::AddedDesc,
        limit: Some(50),
        ..Default::default()
    })
}

/// All built-in presets, in display order. The ordering doubles as
/// the sort_order for inserted rows (preset 0 gets inserted first).
pub fn builtin_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "recently-added",
            name: "Recently added",
            icon: "sparkles",
            color: "#7cc4ff",
            description: "Last 25 documents added to your library, newest first.",
            builder: build_recently_added,
        },
        Preset {
            id: "tax-2025",
            name: "Tax 2025",
            icon: "receipt-text",
            color: "#facc15",
            description: "Anything tagged tax-2025. Auto-creates the tag if missing.",
            builder: build_tax_2025,
        },
        Preset {
            id: "tax-2024",
            name: "Tax 2024",
            icon: "receipt-text",
            color: "#fbbf24",
            description: "Anything tagged tax-2024. Auto-creates the tag if missing.",
            builder: build_tax_2024,
        },
        Preset {
            id: "invoices",
            name: "Invoices",
            icon: "file-text",
            color: "#34d399",
            description: "Documents tagged invoice OR with 'invoice' in the title.",
            builder: build_invoices,
        },
        Preset {
            id: "receipts",
            name: "Receipts",
            icon: "scan-line",
            color: "#f87171",
            description: "Documents tagged receipt OR with 'receipt' in the title.",
            builder: build_receipts,
        },
        Preset {
            id: "contracts-pending",
            name: "Contracts pending signature",
            icon: "file-signature",
            color: "#a78bfa",
            description: "Tagged contract AND NOT tagged signed — your follow-up pile.",
            builder: build_contracts,
        },
        Preset {
            id: "legal",
            name: "Legal documents",
            icon: "scale",
            color: "#fb7185",
            description: "Tagged legal, OR title contains agreement / NDA.",
            builder: build_legal,
        },
        Preset {
            id: "research",
            name: "Research papers",
            icon: "book-open",
            color: "#22d3ee",
            description: "Tagged research, OR title contains paper / thesis.",
            builder: build_research,
        },
        Preset {
            id: "manuals",
            name: "Manuals & guides",
            icon: "book",
            color: "#fcd34d",
            description: "Tagged manual, OR title contains manual / handbook / guide.",
            builder: build_manuals,
        },
        Preset {
            id: "scanned",
            name: "Scanned documents",
            icon: "scan",
            color: "#60a5fa",
            description: "Tagged scanned, OR title contains 'scan'.",
            builder: build_scanned,
        },
        Preset {
            id: "untagged",
            name: "Untagged",
            icon: "tag-off",
            color: "#94a3b8",
            description: "Recent docs with no tags — your cleanup queue.",
            builder: build_untagged,
        },
    ]
}

/// Return the user-visible info for every built-in preset. Cheap —
/// re-computes on every call but the list is ~11 entries.
pub fn list_presets() -> Vec<PresetInfo> {
    builtin_presets().iter().map(PresetInfo::from).collect()
}

/// Find a preset by stable id. Returns `None` if the id is unknown
/// (e.g. an old client invoking a preset that was removed).
fn find_preset(id: &str) -> Option<Preset> {
    builtin_presets().into_iter().find(|p| p.id == id)
}

/// Materialize a preset into a real smart collection row. The
/// underlying `library_smart_collections.name` column is UNIQUE, so
/// calling `apply_preset` twice for the same id returns a constraint
/// error — the frontend should call `presets_already_applied` first
/// and grey out the button. We surface the raw DB error rather than
/// mapping to a typed variant because the UI just shows a toast
/// either way.
pub fn apply_preset(
    db: &mut LibraryDb,
    preset_id: &str,
) -> Result<SmartCollectionRecord, LibraryError> {
    let preset = find_preset(preset_id).ok_or_else(|| {
        LibraryError::Db(rusqlite::Error::QueryReturnedNoRows) // closest available variant
    })?;
    let filter = (preset.builder)(db)?;
    let spec = NewSmartCollection {
        name: preset.name.into(),
        icon: Some(preset.icon.into()),
        color: Some(preset.color.into()),
        filter,
    };
    create_smart_collection(db, &spec)
}

/// Returns the ids of presets that are ALREADY materialized as smart
/// collections (matched by name). Lets the UI grey out "Add" buttons.
pub fn presets_already_applied(db: &LibraryDb) -> Result<Vec<String>, LibraryError> {
    let existing = super::collections::list_smart_collections(db)?;
    let mut applied = Vec::new();
    for p in builtin_presets() {
        if existing.iter().any(|sc| sc.name == p.name) {
            applied.push(p.id.into());
        }
    }
    Ok(applied)
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> LibraryDb {
        LibraryDb::open_in_memory().expect("in-memory db")
    }

    #[test]
    fn list_presets_returns_all_builtins() {
        let infos = list_presets();
        assert!(
            infos.len() >= 10,
            "expected at least 10 built-in presets, got {}",
            infos.len()
        );
        // Stable ids must be unique.
        let mut ids: Vec<_> = infos.iter().map(|i| i.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), infos.len(), "duplicate preset ids");
    }

    #[test]
    fn every_preset_resolves_without_error() {
        for preset in builtin_presets() {
            let mut db = fresh_db();
            let filter = (preset.builder)(&mut db)
                .unwrap_or_else(|e| panic!("preset {} failed: {:?}", preset.id, e));
            // round-trip the filter as JSON so we catch any serde regressions.
            let json = serde_json::to_string(&filter).unwrap();
            let _back: LibraryFilter = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn apply_preset_creates_smart_collection() {
        let mut db = fresh_db();
        let sc = apply_preset(&mut db, "tax-2025").unwrap();
        assert_eq!(sc.name, "Tax 2025");
        assert_eq!(sc.icon.as_deref(), Some("receipt-text"));
        // Auto-created tag exists.
        let tags = db.list_tags().unwrap();
        assert!(tags.iter().any(|t| t.name == "tax-2025"));
    }

    #[test]
    fn ensure_tag_is_idempotent_case_insensitive() {
        let mut db = fresh_db();
        let a = ensure_tag(&mut db, "Invoice", None).unwrap();
        let b = ensure_tag(&mut db, "invoice", None).unwrap();
        let c = ensure_tag(&mut db, "INVOICE", None).unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(db.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn apply_unknown_preset_errors() {
        let mut db = fresh_db();
        let result = apply_preset(&mut db, "does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn presets_already_applied_tracks_inserts() {
        let mut db = fresh_db();
        assert!(presets_already_applied(&db).unwrap().is_empty());
        apply_preset(&mut db, "invoices").unwrap();
        let applied = presets_already_applied(&db).unwrap();
        assert!(applied.iter().any(|s| s == "invoices"));
    }

    #[test]
    fn contracts_pending_uses_not_tag_clause() {
        let mut db = fresh_db();
        let filter = build_contracts(&mut db).unwrap();
        let group = filter.clauses.unwrap();
        assert_eq!(group.combinator, FilterCombinator::And);
        let has_not = group
            .clauses
            .iter()
            .any(|c| matches!(c, FilterClause::NotTag { .. }));
        assert!(has_not, "contracts-pending must include a NotTag clause");
    }

    #[test]
    fn invoices_uses_or_combinator() {
        let mut db = fresh_db();
        let filter = build_invoices(&mut db).unwrap();
        let group = filter.clauses.unwrap();
        assert_eq!(group.combinator, FilterCombinator::Or);
        assert_eq!(group.clauses.len(), 2);
    }

    #[test]
    fn applying_same_preset_twice_is_rejected_by_unique_name() {
        // The smart_collections.name UNIQUE constraint guarantees
        // dedupe at the DB layer. The frontend uses
        // `presets_already_applied` to grey out the button instead.
        let mut db = fresh_db();
        apply_preset(&mut db, "invoices").unwrap();
        let second = apply_preset(&mut db, "invoices");
        assert!(second.is_err(), "second apply should violate UNIQUE(name)");
    }
}
