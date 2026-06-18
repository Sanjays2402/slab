// Library query — filter / sort / list documents with tags eager-loaded.
//
// `query_documents` is the public entry point that the
// `slab_library_list_docs` Tauri command wraps. It builds dynamic SQL
// based on the filter (folder_id / tag intersection / title
// substring) and eager-loads tags in a second query to avoid an
// O(rows × tags) round-trip storm.

use super::registry::{DocumentRecord, LibraryDb, LibraryError, TagRecord};
use rusqlite::types::ToSqlOutput;
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryFilter {
    pub folder_id: Option<i64>,
    /// All listed tag ids must be attached to the doc (AND match).
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    /// Case-insensitive substring match against either title or
    /// filename component of `path`. None = no title filter.
    pub title_substring: Option<String>,
    pub limit: Option<u32>,
    #[serde(default)]
    pub sort: SortBy,
    /// v3.34.0 Atlas Smart+: nested AND/OR/NOT clause tree. When
    /// `Some`, this overrides the flat `folder_id`/`tag_ids`/
    /// `title_substring` legacy fields. When `None`, behavior is
    /// unchanged — so all pre-v3.34 stored `query_json` blobs keep
    /// working byte-for-byte. Forward + backward compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clauses: Option<FilterGroup>,
}

/// Combinator for a `FilterGroup`. Default is `And` to match historical
/// flat-filter semantics where every condition was AND'd together.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterCombinator {
    #[default]
    And,
    Or,
}

/// One node in the recursive clause tree. Variants are tagged so the
/// frontend can dispatch on `clause.type` without sniffing fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilterClause {
    /// Document has tag with this id.
    Tag { id: i64 },
    /// Document does NOT have tag with this id.
    NotTag { id: i64 },
    /// Document is in this folder.
    Folder { id: i64 },
    /// Document is NOT in this folder.
    NotFolder { id: i64 },
    /// Case-insensitive substring match against title OR path.
    TitleContains { value: String },
    /// Negation of TitleContains.
    TitleNotContains { value: String },
    /// Document has NO tags at all (no `library_doc_tags` rows).
    Untagged,
    /// Document has at least one tag.
    Tagged,
    /// Nested group — enables `(A OR B) AND NOT C` style rules.
    Group(FilterGroup),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FilterGroup {
    #[serde(default)]
    pub combinator: FilterCombinator,
    #[serde(default)]
    pub clauses: Vec<FilterClause>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    AddedDesc,
    TitleAsc,
    LastSeenDesc,
}

impl SortBy {
    fn sql(self) -> &'static str {
        match self {
            SortBy::AddedDesc => "added_at DESC, id DESC",
            SortBy::TitleAsc => "COALESCE(LOWER(title), LOWER(path)) ASC, id ASC",
            SortBy::LastSeenDesc => "last_seen_at DESC, id DESC",
        }
    }
}

/// Query documents matching `filter`, with tags eager-loaded.
pub fn query_documents(
    db: &LibraryDb,
    filter: &LibraryFilter,
) -> Result<Vec<DocumentRecord>, LibraryError> {
    let conn = db.conn();

    let mut sql = String::from(
        "SELECT id, folder_id, path, title, hash, size_bytes, mtime_ns, pages, added_at, last_seen_at, ocr_state, ocr_output_path
         FROM library_documents",
    );
    let mut where_clauses: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();

    // v3.34.0: when a clause tree is present, it FULLY replaces the
    // legacy flat-filter where-clauses (folder/tags/title). The legacy
    // sort + limit fields still apply.
    if let Some(group) = filter.clauses.as_ref() {
        let frag = build_group_sql(group, &mut params);
        if !frag.is_empty() {
            where_clauses.push(frag);
        }
    } else {
        if let Some(fid) = filter.folder_id {
            where_clauses.push("folder_id = ?".into());
            params.push(Box::new(fid));
        }
        if let Some(ref sub) = filter.title_substring {
            if !sub.is_empty() {
                where_clauses.push(
                    "(LOWER(COALESCE(title, '')) LIKE LOWER(?) OR LOWER(path) LIKE LOWER(?))"
                        .into(),
                );
                let pat = format!("%{}%", sub);
                params.push(Box::new(pat.clone()));
                params.push(Box::new(pat));
            }
        }
        if !filter.tag_ids.is_empty() {
            // Force AND-match across all tag ids: doc must have a
            // doc_tags row for each requested tag. We do this with a
            // subquery so we can keep one prepared statement.
            let placeholders = filter
                .tag_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            where_clauses.push(format!(
                "id IN (SELECT doc_id FROM library_doc_tags WHERE tag_id IN ({placeholders})
                        GROUP BY doc_id HAVING COUNT(DISTINCT tag_id) = ?)",
            ));
            for tid in &filter.tag_ids {
                params.push(Box::new(*tid));
            }
            params.push(Box::new(filter.tag_ids.len() as i64));
        }
    }

    if !where_clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_clauses.join(" AND "));
    }

    sql.push_str(" ORDER BY ");
    sql.push_str(filter.sort.sql());

    if let Some(lim) = filter.limit {
        sql.push_str(" LIMIT ?");
        params.push(Box::new(lim as i64));
    }

    let param_refs: Vec<&dyn ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<DocumentRecord> = stmt
        .query_map(rusqlite::params_from_iter(param_refs.iter()), |row| {
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
                tags: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        return Ok(rows);
    }

    // Eager-load tags for all returned rows in ONE batched query.
    let id_list = rows
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut tag_stmt = conn.prepare(&format!(
        "SELECT dt.doc_id, t.id, t.name, t.color
         FROM library_doc_tags dt
         INNER JOIN library_tags t ON t.id = dt.tag_id
         WHERE dt.doc_id IN ({id_list})
         ORDER BY t.name ASC",
    ))?;
    let mut tags_by_doc: HashMap<i64, Vec<TagRecord>> = HashMap::new();
    let tag_rows = tag_stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            TagRecord {
                id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
            },
        ))
    })?;
    for r in tag_rows {
        let (doc_id, tag) = r?;
        tags_by_doc.entry(doc_id).or_default().push(tag);
    }
    let rows = rows
        .into_iter()
        .map(|mut r| {
            if let Some(ts) = tags_by_doc.remove(&r.id) {
                r.tags = ts;
            }
            r
        })
        .collect();
    Ok(rows)
}

// Tiny ToSql wrapper to bypass clippy's "unused trait" warning on the
// dyn import — this is here because `params_from_iter` wants
// `&dyn ToSql`, not `Box<dyn ToSql>`.
#[allow(dead_code)]
fn _toql_anchor(b: &dyn ToSql) -> rusqlite::Result<ToSqlOutput<'_>> {
    b.to_sql()
}

/// Recursive SQL fragment builder for a `FilterGroup`. Pushes positional
/// parameters into `params` in left-to-right order and returns the
/// parenthesized fragment.
///
/// Empty groups are short-circuited to the combinator's identity:
/// - empty AND → `1=1` (matches everything, like a no-op WHERE)
/// - empty OR  → `0=1` (matches nothing, since "any of zero" is false)
fn build_group_sql(group: &FilterGroup, params: &mut Vec<Box<dyn ToSql>>) -> String {
    if group.clauses.is_empty() {
        return match group.combinator {
            FilterCombinator::And => "1=1".into(),
            FilterCombinator::Or => "0=1".into(),
        };
    }
    let joiner = match group.combinator {
        FilterCombinator::And => " AND ",
        FilterCombinator::Or => " OR ",
    };
    let parts: Vec<String> = group
        .clauses
        .iter()
        .map(|c| build_clause_sql(c, params))
        .collect();
    format!("({})", parts.join(joiner))
}

fn build_clause_sql(clause: &FilterClause, params: &mut Vec<Box<dyn ToSql>>) -> String {
    match clause {
        FilterClause::Tag { id } => {
            params.push(Box::new(*id));
            "id IN (SELECT doc_id FROM library_doc_tags WHERE tag_id = ?)".into()
        }
        FilterClause::NotTag { id } => {
            params.push(Box::new(*id));
            "id NOT IN (SELECT doc_id FROM library_doc_tags WHERE tag_id = ?)".into()
        }
        FilterClause::Folder { id } => {
            params.push(Box::new(*id));
            "folder_id = ?".into()
        }
        FilterClause::NotFolder { id } => {
            params.push(Box::new(*id));
            "(folder_id IS NULL OR folder_id <> ?)".into()
        }
        FilterClause::TitleContains { value } => {
            let pat = format!("%{}%", value);
            params.push(Box::new(pat.clone()));
            params.push(Box::new(pat));
            "(LOWER(COALESCE(title, '')) LIKE LOWER(?) OR LOWER(path) LIKE LOWER(?))".into()
        }
        FilterClause::TitleNotContains { value } => {
            let pat = format!("%{}%", value);
            params.push(Box::new(pat.clone()));
            params.push(Box::new(pat));
            "NOT (LOWER(COALESCE(title, '')) LIKE LOWER(?) OR LOWER(path) LIKE LOWER(?))".into()
        }
        FilterClause::Untagged => "id NOT IN (SELECT doc_id FROM library_doc_tags)".into(),
        FilterClause::Tagged => "id IN (SELECT doc_id FROM library_doc_tags)".into(),
        FilterClause::Group(g) => build_group_sql(g, params),
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::registry::LibraryDb;

    fn seed() -> LibraryDb {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let f1 = db.add_folder("/papers").unwrap();
        let f2 = db.add_folder("/contracts").unwrap();
        let t1 = db.add_tag("research", None).unwrap();
        let t2 = db.add_tag("urgent", None).unwrap();
        let t3 = db.add_tag("done", None).unwrap();

        let d1 = db
            .upsert_document(
                Some(f1.id),
                "/papers/alpha.pdf",
                Some("Alpha Paper"),
                "h1",
                100,
                1,
                Some(5),
                None,
            )
            .unwrap();
        let d2 = db
            .upsert_document(
                Some(f1.id),
                "/papers/beta.pdf",
                Some("Beta Paper"),
                "h2",
                200,
                2,
                Some(7),
                None,
            )
            .unwrap();
        let _d3 = db
            .upsert_document(
                Some(f2.id),
                "/contracts/lease.pdf",
                Some("Lease Agreement"),
                "h3",
                300,
                3,
                Some(2),
                None,
            )
            .unwrap();

        db.set_doc_tags(d1.id, &[t1.id, t2.id]).unwrap();
        db.set_doc_tags(d2.id, &[t1.id, t3.id]).unwrap();
        db
    }

    #[test]
    fn query_no_filter_returns_all() {
        let db = seed();
        let rows = query_documents(&db, &LibraryFilter::default()).unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn query_by_folder_id() {
        let db = seed();
        let f = db.list_folders().unwrap();
        let papers_id = f.iter().find(|x| x.path == "/papers").unwrap().id;
        let rows = query_documents(
            &db,
            &LibraryFilter {
                folder_id: Some(papers_id),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.folder_id == Some(papers_id)));
    }

    #[test]
    fn query_title_substring_case_insensitive() {
        let db = seed();
        let rows = query_documents(
            &db,
            &LibraryFilter {
                title_substring: Some("paper".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        let titles: Vec<_> = rows.iter().filter_map(|r| r.title.clone()).collect();
        assert!(titles.contains(&"Alpha Paper".to_string()));
        assert!(titles.contains(&"Beta Paper".to_string()));
    }

    #[test]
    fn query_by_tag_id() {
        let db = seed();
        let tags = db.list_tags().unwrap();
        let research = tags.iter().find(|t| t.name == "research").unwrap().id;
        let rows = query_documents(
            &db,
            &LibraryFilter {
                tag_ids: vec![research],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.tags.iter().any(|t| t.id == research)));
    }

    #[test]
    fn query_by_multiple_tags_is_and_match() {
        let db = seed();
        let tags = db.list_tags().unwrap();
        let research = tags.iter().find(|t| t.name == "research").unwrap().id;
        let urgent = tags.iter().find(|t| t.name == "urgent").unwrap().id;
        let rows = query_documents(
            &db,
            &LibraryFilter {
                tag_ids: vec![research, urgent],
                ..Default::default()
            },
        )
        .unwrap();
        // Only alpha.pdf has BOTH research and urgent.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/papers/alpha.pdf");
    }

    #[test]
    fn query_sort_title_asc() {
        let db = seed();
        let rows = query_documents(
            &db,
            &LibraryFilter {
                sort: SortBy::TitleAsc,
                ..Default::default()
            },
        )
        .unwrap();
        let titles: Vec<_> = rows.iter().filter_map(|r| r.title.clone()).collect();
        assert_eq!(titles, vec!["Alpha Paper", "Beta Paper", "Lease Agreement"]);
    }

    #[test]
    fn query_limit_truncates() {
        let db = seed();
        let rows = query_documents(
            &db,
            &LibraryFilter {
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn query_eager_loads_tags() {
        let db = seed();
        let rows = query_documents(&db, &LibraryFilter::default()).unwrap();
        let alpha = rows.iter().find(|r| r.path == "/papers/alpha.pdf").unwrap();
        assert_eq!(alpha.tags.len(), 2);
        let names: Vec<_> = alpha.tags.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"research"));
        assert!(names.contains(&"urgent"));
    }

    #[test]
    fn filter_clauses_roundtrip_json() {
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::Or,
                clauses: vec![
                    FilterClause::Tag { id: 1 },
                    FilterClause::NotTag { id: 2 },
                    FilterClause::Group(FilterGroup {
                        combinator: FilterCombinator::And,
                        clauses: vec![FilterClause::Folder { id: 3 }],
                    }),
                ],
            }),
            ..Default::default()
        };
        let s = serde_json::to_string(&f).unwrap();
        let back: LibraryFilter = serde_json::from_str(&s).unwrap();
        assert_eq!(f.clauses, back.clauses);
        // Tagged serde: `{"type":"tag","id":1}` in the JSON.
        assert!(s.contains("\"type\":\"tag\""));
        assert!(s.contains("\"type\":\"not_tag\""));
        assert!(s.contains("\"type\":\"group\""));
        assert!(s.contains("\"combinator\":\"or\""));
    }

    #[test]
    fn legacy_filter_json_still_deserializes() {
        // Shape persisted by v3.32 / v3.33 — no `clauses` field.
        let legacy = r#"{"folder_id":null,"tag_ids":[1,2],"title_substring":"tax","limit":null,"sort":"added_desc"}"#;
        let f: LibraryFilter = serde_json::from_str(legacy).unwrap();
        assert_eq!(f.tag_ids, vec![1, 2]);
        assert_eq!(f.title_substring.as_deref(), Some("tax"));
        assert!(f.clauses.is_none());
    }

    #[test]
    fn empty_default_filter_serializes_without_clauses_field() {
        // Ensure `clauses: None` is dropped on serialize so old Slab
        // versions reading new query_json don't barf on unknown fields.
        let f = LibraryFilter::default();
        let s = serde_json::to_string(&f).unwrap();
        assert!(!s.contains("clauses"));
    }

    #[test]
    fn query_nested_or_and_not_clauses() {
        let db = seed();
        let tags = db.list_tags().unwrap();
        let research = tags.iter().find(|t| t.name == "research").unwrap().id;
        let urgent = tags.iter().find(|t| t.name == "urgent").unwrap().id;
        let done = tags.iter().find(|t| t.name == "done").unwrap().id;

        // (research AND NOT urgent) → only beta.pdf (alpha also has urgent)
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![
                    FilterClause::Tag { id: research },
                    FilterClause::NotTag { id: urgent },
                ],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/papers/beta.pdf");

        // (urgent OR done) → alpha + beta
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::Or,
                clauses: vec![
                    FilterClause::Tag { id: urgent },
                    FilterClause::Tag { id: done },
                ],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        assert_eq!(rows.len(), 2);
        let paths: Vec<_> = rows.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"/papers/alpha.pdf"));
        assert!(paths.contains(&"/papers/beta.pdf"));
    }

    #[test]
    fn query_empty_and_group_returns_all() {
        let db = seed();
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![],
            }),
            ..Default::default()
        };
        assert_eq!(query_documents(&db, &f).unwrap().len(), 3);
    }

    #[test]
    fn query_empty_or_group_returns_none() {
        let db = seed();
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::Or,
                clauses: vec![],
            }),
            ..Default::default()
        };
        assert_eq!(query_documents(&db, &f).unwrap().len(), 0);
    }

    #[test]
    fn query_deeply_nested_groups() {
        // ((research OR done) AND NOT urgent) → only beta.pdf
        // alpha has research+urgent → fails NOT urgent
        // beta  has research+done   → passes
        // lease has neither         → fails inner OR
        let db = seed();
        let tags = db.list_tags().unwrap();
        let research = tags.iter().find(|t| t.name == "research").unwrap().id;
        let urgent = tags.iter().find(|t| t.name == "urgent").unwrap().id;
        let done = tags.iter().find(|t| t.name == "done").unwrap().id;
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![
                    FilterClause::Group(FilterGroup {
                        combinator: FilterCombinator::Or,
                        clauses: vec![
                            FilterClause::Tag { id: research },
                            FilterClause::Tag { id: done },
                        ],
                    }),
                    FilterClause::NotTag { id: urgent },
                ],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/papers/beta.pdf");
    }

    #[test]
    fn query_clauses_override_legacy_fields() {
        // Legacy tag_ids says "research" (would return alpha+beta),
        // but clauses says "NotTag research" — clauses wins.
        let db = seed();
        let tags = db.list_tags().unwrap();
        let research = tags.iter().find(|t| t.name == "research").unwrap().id;
        let f = LibraryFilter {
            tag_ids: vec![research],
            title_substring: Some("paper".into()),
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::NotTag { id: research }],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        // Only lease.pdf has neither "research" tag.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/contracts/lease.pdf");
    }

    #[test]
    fn query_title_not_contains() {
        let db = seed();
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::TitleNotContains {
                    value: "Lease".into(),
                }],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| !r.path.contains("lease")));
    }

    #[test]
    fn query_folder_clause_and_not_folder() {
        let db = seed();
        let folders = db.list_folders().unwrap();
        let papers = folders.iter().find(|f| f.path == "/papers").unwrap().id;
        let contracts = folders.iter().find(|f| f.path == "/contracts").unwrap().id;

        let f_in = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::Folder { id: papers }],
            }),
            ..Default::default()
        };
        assert_eq!(query_documents(&db, &f_in).unwrap().len(), 2);

        let f_not = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::NotFolder { id: contracts }],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f_not).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.folder_id == Some(papers)));
    }

    #[test]
    fn query_untagged_clause_returns_only_untagged() {
        // seed(): alpha + beta are tagged, lease has no tags.
        let db = seed();
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::Untagged],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/contracts/lease.pdf");
        assert!(rows[0].tags.is_empty());
    }

    #[test]
    fn query_tagged_clause_returns_only_tagged() {
        let db = seed();
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::Tagged],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| !r.tags.is_empty()));
        let paths: Vec<_> = rows.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"/papers/alpha.pdf"));
        assert!(paths.contains(&"/papers/beta.pdf"));
    }

    #[test]
    fn query_untagged_composes_with_folder_clause() {
        // Untagged AND in /contracts → just lease. Untagged AND in
        // /papers → nothing (both papers are tagged).
        let db = seed();
        let folders = db.list_folders().unwrap();
        let papers = folders.iter().find(|f| f.path == "/papers").unwrap().id;
        let contracts = folders.iter().find(|f| f.path == "/contracts").unwrap().id;

        let f_contracts = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![
                    FilterClause::Untagged,
                    FilterClause::Folder { id: contracts },
                ],
            }),
            ..Default::default()
        };
        let rows = query_documents(&db, &f_contracts).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/contracts/lease.pdf");

        let f_papers = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::Untagged, FilterClause::Folder { id: papers }],
            }),
            ..Default::default()
        };
        assert_eq!(query_documents(&db, &f_papers).unwrap().len(), 0);
    }

    #[test]
    fn untagged_clause_roundtrips_json() {
        let f = LibraryFilter {
            clauses: Some(FilterGroup {
                combinator: FilterCombinator::And,
                clauses: vec![FilterClause::Untagged, FilterClause::Tagged],
            }),
            ..Default::default()
        };
        let s = serde_json::to_string(&f).unwrap();
        let back: LibraryFilter = serde_json::from_str(&s).unwrap();
        assert_eq!(f.clauses, back.clauses);
        // Unit variants serialize as `{"type":"untagged"}` (internally tagged).
        assert!(s.contains("\"type\":\"untagged\""));
        assert!(s.contains("\"type\":\"tagged\""));
    }
}
