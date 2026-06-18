// Atlas Bulk Tag-Apply — attach or detach a single tag across many
// documents in one atomic action. (v3.41.0 roadmap slice #2.)
//
// The per-document tag editor (`registry::set_doc_tags`) and the AI
// auto-tagger (`auto_tagger.rs`) both operate one doc at a time. This
// module is the *bulk* surface the multi-select library grid drives:
// pick N documents, then apply (find-or-create) or remove one tag across
// all of them in a single SQLite transaction.
//
// Design notes:
//   * Apply takes a tag *name* and find-or-creates it (auto-coloring new
//     tags with the same deterministic pastel `accept_tag_suggestion`
//     uses), so the UI can offer both "apply existing tag" and "apply a
//     brand-new tag" through one path.
//   * Remove takes a tag *id* (you can only remove a tag that exists) and
//     detaches it from the named docs — it does NOT delete the tag
//     globally. `registry::remove_tag` remains the destructive op.
//   * Both report `affected` (docs whose tag set actually changed) vs
//     `total` (docs in the request), so the UI can say "Applied to 7 of
//     9 (2 already had it)". Missing/duplicate doc ids are skipped, never
//     fatal — a bulk action over a stale selection still succeeds.

use super::registry::{LibraryDb, LibraryError, TagRecord};
use super::tag_suggest::pastel_for;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Outcome of a bulk apply/remove over a document set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkTagResult {
    /// The tag that was applied or removed (resolved/created).
    pub tag: TagRecord,
    /// Number of documents whose tag set actually changed.
    pub affected: usize,
    /// Number of document ids in the request — including ids that were
    /// already in the target state or no longer exist.
    pub total: usize,
}

/// Apply `tag_name` to every document in `doc_ids`, find-or-creating the
/// tag. Documents that already carry the tag, and ids that no longer
/// reference a live document, are skipped (counted in `total`, not
/// `affected`). The whole apply runs in one transaction.
pub fn apply_tag_to_docs(
    db: &mut LibraryDb,
    tag_name: &str,
    doc_ids: &[i64],
) -> Result<BulkTagResult, LibraryError> {
    let normalized = tag_name.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(LibraryError::Other("tag name is empty".into()));
    }

    // Find or create the tag (auto-color new ones to match suggestions).
    let tag = match db.find_tag_by_name(&normalized)? {
        Some(t) => t,
        None => db.add_tag(&normalized, Some(&pastel_for(&normalized)))?,
    };

    let total = doc_ids.len();
    let mut affected = 0usize;

    let tx = db.conn_mut().transaction()?;
    {
        let mut doc_exists = tx.prepare("SELECT 1 FROM library_documents WHERE id = ?1")?;
        let mut already_linked =
            tx.prepare("SELECT 1 FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2")?;
        let mut insert =
            tx.prepare("INSERT INTO library_doc_tags (doc_id, tag_id) VALUES (?1, ?2)")?;
        for &doc_id in doc_ids {
            if !doc_exists.exists(params![doc_id])? {
                continue; // stale selection — silently skip
            }
            if already_linked.exists(params![doc_id, tag.id])? {
                continue; // no-op, doc already tagged
            }
            insert.execute(params![doc_id, tag.id])?;
            affected += 1;
        }
    }
    tx.commit()?;

    Ok(BulkTagResult {
        tag,
        affected,
        total,
    })
}

/// Remove the tag `tag_id` from every document in `doc_ids`. The tag row
/// itself is left intact (still attached to any docs not in the set, still
/// listed in the tag rail) — only the named links are detached. Documents
/// that don't carry the tag are skipped. Errors if `tag_id` is unknown.
pub fn remove_tag_from_docs(
    db: &mut LibraryDb,
    tag_id: i64,
    doc_ids: &[i64],
) -> Result<BulkTagResult, LibraryError> {
    let tag = db
        .find_tag_by_id(tag_id)?
        .ok_or_else(|| LibraryError::Other(format!("tag {tag_id} not found")))?;

    let total = doc_ids.len();
    let mut affected = 0usize;

    let tx = db.conn_mut().transaction()?;
    {
        let mut delete =
            tx.prepare("DELETE FROM library_doc_tags WHERE doc_id = ?1 AND tag_id = ?2")?;
        for &doc_id in doc_ids {
            if delete.execute(params![doc_id, tag_id])? > 0 {
                affected += 1;
            }
        }
    }
    tx.commit()?;

    Ok(BulkTagResult {
        tag,
        affected,
        total,
    })
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::registry::LibraryDb;
    use std::collections::HashSet;

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().expect("open in-memory DB")
    }

    fn add_doc(db: &mut LibraryDb, title: &str, path: &str) -> i64 {
        let title_opt = if title.is_empty() { None } else { Some(title) };
        db.upsert_document(None, path, title_opt, "hash", 100, 1, Some(1), None)
            .expect("insert doc")
            .id
    }

    fn tag_names(db: &LibraryDb, doc_id: i64) -> HashSet<String> {
        db.tags_for_document(doc_id)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    #[test]
    fn apply_creates_tag_when_missing_and_colors_it() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let res = apply_tag_to_docs(&mut d, "Archive", &[a]).unwrap();
        assert_eq!(res.tag.name, "archive"); // normalized
        assert!(res.tag.color.is_some(), "new tag should be auto-colored");
        assert_eq!(res.affected, 1);
        assert_eq!(res.total, 1);
        assert!(tag_names(&d, a).contains("archive"));
    }

    #[test]
    fn apply_attaches_to_many_docs() {
        let mut d = db();
        let ids: Vec<i64> = (0..3)
            .map(|i| add_doc(&mut d, "x", &format!("/tmp/{i}.pdf")))
            .collect();
        let res = apply_tag_to_docs(&mut d, "review", &ids).unwrap();
        assert_eq!(res.affected, 3);
        assert_eq!(res.total, 3);
        for id in ids {
            assert!(tag_names(&d, id).contains("review"));
        }
    }

    #[test]
    fn apply_skips_docs_already_carrying_the_tag() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let b = add_doc(&mut d, "B", "/tmp/b.pdf");
        // First apply attaches to both.
        apply_tag_to_docs(&mut d, "paid", &[a, b]).unwrap();
        // Second apply over a+b plus a fresh doc c only changes c.
        let c = add_doc(&mut d, "C", "/tmp/c.pdf");
        let res = apply_tag_to_docs(&mut d, "paid", &[a, b, c]).unwrap();
        assert_eq!(res.affected, 1, "only the new doc changes");
        assert_eq!(res.total, 3);
    }

    #[test]
    fn apply_reuses_existing_tag_no_duplicate_row() {
        let mut d = db();
        let existing = d.add_tag("invoice", Some("#fff")).unwrap();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let res = apply_tag_to_docs(&mut d, "Invoice", &[a]).unwrap();
        assert_eq!(res.tag.id, existing.id, "reuses the existing tag row");
        assert_eq!(d.list_tags().unwrap().len(), 1, "no duplicate tag created");
    }

    #[test]
    fn apply_unions_with_existing_doc_tags() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let t = d.add_tag("keep", None).unwrap();
        d.set_doc_tags(a, &[t.id]).unwrap();
        apply_tag_to_docs(&mut d, "extra", &[a]).unwrap();
        let names = tag_names(&d, a);
        assert!(names.contains("keep"));
        assert!(names.contains("extra"));
    }

    #[test]
    fn apply_empty_name_errors() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        assert!(apply_tag_to_docs(&mut d, "   ", &[a]).is_err());
    }

    #[test]
    fn apply_skips_missing_doc_ids() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        // 999999 doesn't exist — must be skipped, not a FK error.
        let res = apply_tag_to_docs(&mut d, "tag", &[a, 999_999]).unwrap();
        assert_eq!(res.affected, 1);
        assert_eq!(res.total, 2);
        assert!(tag_names(&d, a).contains("tag"));
    }

    #[test]
    fn apply_empty_doc_set_is_noop_but_creates_tag() {
        let mut d = db();
        let res = apply_tag_to_docs(&mut d, "orphan", &[]).unwrap();
        assert_eq!(res.affected, 0);
        assert_eq!(res.total, 0);
        // Tag is still created so the rail can show it immediately.
        assert!(d.find_tag_by_name("orphan").unwrap().is_some());
    }

    #[test]
    fn remove_detaches_from_named_docs_only() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let b = add_doc(&mut d, "B", "/tmp/b.pdf");
        let res_apply = apply_tag_to_docs(&mut d, "wip", &[a, b]).unwrap();
        let tag_id = res_apply.tag.id;
        // Remove from a only.
        let res = remove_tag_from_docs(&mut d, tag_id, &[a]).unwrap();
        assert_eq!(res.affected, 1);
        assert!(!tag_names(&d, a).contains("wip"));
        assert!(tag_names(&d, b).contains("wip"), "b keeps the tag");
    }

    #[test]
    fn remove_counts_only_docs_that_had_the_tag() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let b = add_doc(&mut d, "B", "/tmp/b.pdf");
        let tag = apply_tag_to_docs(&mut d, "x", &[a]).unwrap().tag;
        // b never had the tag → affected should be 1 (only a).
        let res = remove_tag_from_docs(&mut d, tag.id, &[a, b]).unwrap();
        assert_eq!(res.affected, 1);
        assert_eq!(res.total, 2);
    }

    #[test]
    fn remove_keeps_tag_row_in_library() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        let tag = apply_tag_to_docs(&mut d, "temp", &[a]).unwrap().tag;
        remove_tag_from_docs(&mut d, tag.id, &[a]).unwrap();
        // The tag is detached from the doc but still exists globally.
        assert!(d.find_tag_by_id(tag.id).unwrap().is_some());
        assert_eq!(d.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn remove_unknown_tag_errors() {
        let mut d = db();
        let a = add_doc(&mut d, "A", "/tmp/a.pdf");
        assert!(remove_tag_from_docs(&mut d, 4242, &[a]).is_err());
    }
}
