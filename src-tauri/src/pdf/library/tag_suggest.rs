// Atlas Tag-Suggest — heuristic per-document tag suggestion engine.
//
// For any document, proposes up to `SUGGEST_LIMIT` relevant tags computed
// locally from three deterministic signals:
//
//   1. Vocabulary match — tokens of (title ∪ basename) that exactly match
//      an existing `library_tags.name` (case/punctuation normalized).
//   2. Co-occurrence boost — for each tag the doc already has, sibling
//      tags that frequently appear alongside it on other documents.
//   3. Domain hints — a small built-in dictionary so a doc named
//      "INV-001.pdf" surfaces "invoice" even in a library with no tags yet.
//
// This mirrors the v3.38.0 `folder_suggest.rs` pattern: pure-Rust,
// deterministic, no network, fast enough to run over thousands of docs in
// milliseconds. The existing `auto_tagger.rs` (AI-powered assignment from
// page content) is the heavier *commit* surface and stays untouched; this
// is the cheap, always-on *suggestion* surface.

use super::registry::{LibraryDb, LibraryError, TagRecord};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Hard cap on suggestions returned per document.
pub const SUGGEST_LIMIT: usize = 5;

/// Tokens too common or structural to be useful tag candidates.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "of", "and", "or", "to", "in", "on", "for", "with", "is", "by", "at", "pdf",
    "doc", "document", "file", "scan", "scanned", "copy", "final", "draft", "version", "untitled",
    "new",
];

/// Built-in domain dictionary: canonical tag → alias tokens that imply it.
/// Lets a brand-new library still get sensible suggestions before the user
/// has created any tags of their own.
const DOMAIN_HINTS: &[(&str, &[&str])] = &[
    (
        "invoice",
        &["invoice", "invoices", "inv", "bill", "billing"],
    ),
    ("receipt", &["receipt", "receipts", "rcpt"]),
    (
        "contract",
        &["contract", "contracts", "agreement", "mou", "nda"],
    ),
    (
        "manual",
        &["manual", "handbook", "guide", "userguide", "spec"],
    ),
    ("paper", &["paper", "arxiv", "preprint", "research"]),
    (
        "slides",
        &["slides", "deck", "keynote", "pptx", "presentation"],
    ),
    ("recipe", &["recipe", "cookbook"]),
    ("lease", &["lease", "rental", "tenancy"]),
    ("tax", &["tax", "taxes", "irs", "w2", "w9", "1099"]),
    ("resume", &["resume", "cv", "curriculum"]),
    (
        "report",
        &["report", "reports", "quarterly", "annual", "summary"],
    ),
    ("notes", &["notes", "note"]),
    ("letter", &["letter", "memo"]),
    ("form", &["form", "forms", "application"]),
];

/// One suggested tag for a single document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagSuggestion {
    pub tag_name: String,
    pub score: i64,
    /// "vocabulary" | "cooccurrence" | "domain"
    pub source: String,
    /// True if a `library_tags` row already exists with this name.
    pub existing: bool,
}

/// A document plus its suggested tags, used by the bulk endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkTagSuggestion {
    pub doc_id: i64,
    pub title: Option<String>,
    pub path: String,
    pub suggestions: Vec<TagSuggestion>,
}

/// Split text into lowercased candidate tokens, dropping stopwords,
/// short tokens (<3 chars), and pure-numeric tokens (years, ids).
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .filter(|t| !STOPWORDS.contains(t))
        .filter(|t| t.chars().any(|c| c.is_alphabetic()))
        .map(|s| s.to_string())
        .collect()
}

/// Compute ranked tag suggestions for a single document.
pub fn suggest_tags_for_doc(
    db: &LibraryDb,
    doc_id: i64,
) -> Result<Vec<TagSuggestion>, LibraryError> {
    let conn = db.conn();

    // 1. Fetch doc title + path. If the doc doesn't exist, no suggestions.
    let row = conn
        .query_row(
            "SELECT title, path FROM library_documents WHERE id = ?1",
            params![doc_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)),
        )
        .ok();
    let (title, path) = match row {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };
    let basename = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let tokens: HashSet<String> = tokenize(&format!("{} {}", title.unwrap_or_default(), basename))
        .into_iter()
        .collect();

    // 2. Tags already on this doc (never suggest a tag the doc already has).
    let mut already: HashSet<String> = HashSet::new();
    let mut already_ids: Vec<i64> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name FROM library_tags t
             INNER JOIN library_doc_tags dt ON dt.tag_id = t.id
             WHERE dt.doc_id = ?1",
        )?;
        let rows = stmt.query_map(params![doc_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, name) = row?;
            already_ids.push(id);
            already.insert(name.to_lowercase());
        }
    }

    // 3. Dismissed suggestions for this doc.
    let dismissed: HashSet<String> = {
        let mut stmt = conn
            .prepare("SELECT tag_name FROM library_tag_suggestion_dismissed WHERE doc_id = ?1")?;
        let rows = stmt.query_map(params![doc_id], |r| r.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok().map(|s| s.to_lowercase()))
            .collect()
    };

    // 4. Vocabulary match — exact token ↔ existing tag name.
    let vocab: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM library_tags")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // name(lowercased) -> (score, source, existing)
    let mut scores: HashMap<String, (i64, &'static str, bool)> = HashMap::new();
    for name in &vocab {
        let lname = name.to_lowercase();
        if already.contains(&lname) || dismissed.contains(&lname) {
            continue;
        }
        if tokens.contains(&lname) {
            let e = scores.entry(lname).or_insert((0, "vocabulary", true));
            e.0 += 20;
        }
    }

    // 5. Co-occurrence — sibling tags of the doc's existing tags.
    if !already_ids.is_empty() {
        let placeholders: String = already_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT t.name, COUNT(*) AS cnt
             FROM library_doc_tags dt
             INNER JOIN library_tags t ON t.id = dt.tag_id
             WHERE dt.doc_id IN (
                 SELECT DISTINCT doc_id FROM library_doc_tags
                 WHERE tag_id IN ({ph}) AND doc_id != ?
             )
             AND t.id NOT IN ({ph})
             GROUP BY t.name
             ORDER BY cnt DESC
             LIMIT 20",
            ph = placeholders,
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut bind: Vec<&dyn rusqlite::ToSql> = already_ids
            .iter()
            .map(|x| x as &dyn rusqlite::ToSql)
            .collect();
        bind.push(&doc_id);
        for id in &already_ids {
            bind.push(id as &dyn rusqlite::ToSql);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(bind), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, cnt) = row?;
            let lname = name.to_lowercase();
            if already.contains(&lname) || dismissed.contains(&lname) {
                continue;
            }
            let entry = scores.entry(lname).or_insert((0, "cooccurrence", true));
            entry.0 += 5 * cnt;
        }
    }

    // 6. Domain hints — only when nothing else already proposed that tag.
    for (canonical, aliases) in DOMAIN_HINTS {
        let lcanon = canonical.to_string();
        if already.contains(&lcanon) || dismissed.contains(&lcanon) {
            continue;
        }
        if scores.contains_key(&lcanon) {
            continue;
        }
        if tokens.iter().any(|t| aliases.contains(&t.as_str())) {
            let existing = vocab.iter().any(|n| n.to_lowercase() == lcanon);
            scores.insert(lcanon, (10, "domain", existing));
        }
    }

    // 7. Sort by score desc, then name asc for stable ordering; truncate.
    let mut out: Vec<TagSuggestion> = scores
        .into_iter()
        .map(|(name, (score, source, existing))| TagSuggestion {
            tag_name: name,
            score,
            source: source.to_string(),
            existing,
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.tag_name.cmp(&b.tag_name)));
    out.truncate(SUGGEST_LIMIT);
    Ok(out)
}

/// Find every untagged document and run the suggester over each, skipping
/// docs that produce no suggestions. Used to populate the library-wide
/// "tag your collection" surface.
pub fn suggest_for_untagged(
    db: &LibraryDb,
    limit: usize,
) -> Result<Vec<BulkTagSuggestion>, LibraryError> {
    let conn = db.conn();
    let candidates: Vec<(i64, Option<String>, String)> = {
        let mut stmt = conn.prepare(
            "SELECT d.id, d.title, d.path
             FROM library_documents d
             LEFT JOIN library_doc_tags dt ON dt.doc_id = d.id
             WHERE dt.tag_id IS NULL
             ORDER BY d.last_seen_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut out = Vec::new();
    for (doc_id, title, path) in candidates {
        let suggestions = suggest_tags_for_doc(db, doc_id)?;
        if suggestions.is_empty() {
            continue;
        }
        out.push(BulkTagSuggestion {
            doc_id,
            title,
            path,
            suggestions,
        });
    }
    Ok(out)
}

/// Accept a suggestion: find-or-create the tag (auto-coloring new ones with
/// a deterministic pastel) and attach it to the doc, unioned with whatever
/// tags the doc already has.
pub fn accept_tag_suggestion(
    db: &mut LibraryDb,
    doc_id: i64,
    tag_name: &str,
) -> Result<TagRecord, LibraryError> {
    let normalized = tag_name.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(LibraryError::Other("tag name is empty".into()));
    }

    // 1. Find or create the tag.
    let tag = match db.find_tag_by_name(&normalized)? {
        Some(t) => t,
        None => db.add_tag(&normalized, Some(&pastel_for(&normalized)))?,
    };

    // 2. Union with existing doc tags, then persist.
    let mut current_ids: Vec<i64> = db
        .tags_for_document(doc_id)?
        .into_iter()
        .map(|t| t.id)
        .collect();
    if !current_ids.contains(&tag.id) {
        current_ids.push(tag.id);
    }
    db.set_doc_tags(doc_id, &current_ids)?;
    Ok(tag)
}

/// One element of a bulk-accept request: pin a single tag onto one doc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptItem {
    pub doc_id: i64,
    pub tag_name: String,
}

/// Outcome of a bulk-accept call. `attached` is one TagRecord per
/// successfully attached `(doc_id, tag_name)` pair — already-present
/// tags COUNT as attached so the caller can patch its in-memory doc
/// rows uniformly. `failed` carries `(doc_id, tag_name, reason)` for
/// any item the batch could not apply (e.g. empty name, unknown doc).
///
/// Bulk semantics are per-item, NOT all-or-nothing: a malformed name in
/// item 12 fails item 12 alone, items 0..11 + 13..N still attach. This
/// matches what a paralegal reviewing 50 chips actually wants — one
/// typo doesn't undo 49 good accepts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkAcceptResult {
    pub attached: Vec<(i64, TagRecord)>,
    pub failed: Vec<(i64, String, String)>,
}

/// Apply `items` one at a time, collecting per-item outcomes. Each pair
/// pins the named tag (creating it if missing) onto the given doc,
/// unioned with whatever tags the doc already has. Per-item failures
/// (empty name, unknown doc) land in the `failed` vec; the rest attach.
///
/// Duplicate `(doc_id, tag_name)` pairs in the input are coalesced
/// (after normalising the tag name) so a UI that double-checks the
/// same row by accident is not penalised.
pub fn accept_tag_suggestions_bulk(
    db: &mut LibraryDb,
    items: &[AcceptItem],
) -> Result<BulkAcceptResult, LibraryError> {
    // 1. Pre-pass: validate names, drop empties, dedupe pairs.
    //    Validation runs BEFORE any DB mutation so a rejected name in
    //    item 12 doesn't leave items 0..11 attached.
    let mut failed: Vec<(i64, String, String)> = Vec::new();
    let mut seen: HashSet<(i64, String)> = HashSet::new();
    let mut clean: Vec<(i64, String)> = Vec::new();
    for it in items {
        let normalized = it.tag_name.trim().to_lowercase();
        if normalized.is_empty() {
            failed.push((it.doc_id, it.tag_name.clone(), "tag name is empty".into()));
            continue;
        }
        let key = (it.doc_id, normalized.clone());
        if !seen.insert(key) {
            // Silent dedupe — not a failure.
            continue;
        }
        clean.push((it.doc_id, normalized));
    }
    if clean.is_empty() {
        return Ok(BulkAcceptResult {
            attached: Vec::new(),
            failed,
        });
    }

    // 2. Apply each pair through the per-doc primitive. The primitive
    //    already handles find-or-create + union + pastel coloring, so
    //    this loop stays slim. We don't open a manual transaction here
    //    because set_doc_tags() opens its own txn per call — wrapping
    //    multiple short txns in the same loop is simpler than threading
    //    a single connection-level txn through three setter helpers,
    //    and still rolls back the offending pair on error.
    let mut attached: Vec<(i64, TagRecord)> = Vec::new();
    for (doc_id, name) in &clean {
        match accept_tag_suggestion(db, *doc_id, name) {
            Ok(tag) => attached.push((*doc_id, tag)),
            Err(e) => failed.push((*doc_id, name.clone(), e.to_string())),
        }
    }
    Ok(BulkAcceptResult { attached, failed })
}

/// Record a dismissal so the suggestion never resurfaces for this doc.
pub fn dismiss_tag_suggestion(
    db: &LibraryDb,
    doc_id: i64,
    tag_name: &str,
) -> Result<(), LibraryError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    db.conn().execute(
        "INSERT OR IGNORE INTO library_tag_suggestion_dismissed
         (doc_id, tag_name, dismissed_at) VALUES (?1, ?2, ?3)",
        params![doc_id, tag_name.to_lowercase(), now],
    )?;
    Ok(())
}

/// True if `(doc_id, tag_name)` has been dismissed.
pub fn is_tag_suggestion_dismissed(
    db: &LibraryDb,
    doc_id: i64,
    tag_name: &str,
) -> Result<bool, LibraryError> {
    let n: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM library_tag_suggestion_dismissed
         WHERE doc_id = ?1 AND tag_name = ?2",
        params![doc_id, tag_name.to_lowercase()],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Clear all dismissals for a doc (settings escape hatch — "show me
/// suggestions again").
pub fn undismiss_all_for_doc(db: &LibraryDb, doc_id: i64) -> Result<usize, LibraryError> {
    let n = db.conn().execute(
        "DELETE FROM library_tag_suggestion_dismissed WHERE doc_id = ?1",
        params![doc_id],
    )?;
    Ok(n)
}

/// One dismissal row — the tag the user explicitly hid for this doc + when.
/// `dismissed_at` is unix seconds; the frontend renders this as a relative
/// timestamp ("2 hours ago") so users can recognise yesterday's mistakes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DismissedSuggestion {
    pub tag_name: String,
    pub dismissed_at: i64,
}

/// List every dismissed (doc_id, tag_name) for `doc_id`, newest first.
/// Powers the inspector's "Hidden suggestions" disclosure so a user
/// who dismissed `tax` by accident can undo just that one without
/// nuking every other dismissal on the doc.
pub fn list_dismissed_for_doc(
    db: &LibraryDb,
    doc_id: i64,
) -> Result<Vec<DismissedSuggestion>, LibraryError> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT tag_name, dismissed_at FROM library_tag_suggestion_dismissed
         WHERE doc_id = ?1
         ORDER BY dismissed_at DESC, tag_name ASC",
    )?;
    let rows = stmt.query_map(params![doc_id], |r| {
        Ok(DismissedSuggestion {
            tag_name: r.get::<_, String>(0)?,
            dismissed_at: r.get::<_, i64>(1)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Clear ONE dismissal for `(doc_id, tag_name)`. Returns `true` if a row
/// was deleted; `false` if no such dismissal existed (caller can treat
/// this as a soft-success — there's nothing to undo). Tag name matches
/// the same lowercased normalisation as `dismiss_tag_suggestion` so the
/// undo path is symmetric.
pub fn undismiss_one_for_doc(
    db: &LibraryDb,
    doc_id: i64,
    tag_name: &str,
) -> Result<bool, LibraryError> {
    let n = db.conn().execute(
        "DELETE FROM library_tag_suggestion_dismissed
         WHERE doc_id = ?1 AND tag_name = ?2",
        params![doc_id, tag_name.to_lowercase()],
    )?;
    Ok(n > 0)
}

/// Deterministic soft-pastel hex-ish HSL color from a tag name (FNV-1a →
/// hue). Fixed saturation/lightness so the palette stays cohesive.
///
/// Shared with `bulk_tag.rs` so a tag auto-created by a bulk apply gets the
/// same color it would if created via an accepted suggestion.
pub(crate) fn pastel_for(name: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    let hue = h % 360;
    format!("hsl({}, 60%, 80%)", hue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::registry::LibraryDb;

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().expect("open in-memory DB")
    }

    /// Insert a document with the given title + path, returning its id.
    fn add_doc(db: &mut LibraryDb, title: &str, path: &str) -> i64 {
        let title_opt = if title.is_empty() { None } else { Some(title) };
        db.upsert_document(None, path, title_opt, "hash", 100, 1, Some(1), None)
            .expect("insert doc")
            .id
    }

    /// Attach a fresh-or-existing tag to a doc (test convenience).
    fn tag_doc(db: &mut LibraryDb, doc_id: i64, tags: &[&str]) {
        let mut ids: Vec<i64> = db
            .tags_for_document(doc_id)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        for name in tags {
            let t = db.add_tag(&name.to_lowercase(), None).unwrap();
            if !ids.contains(&t.id) {
                ids.push(t.id);
            }
        }
        db.set_doc_tags(doc_id, &ids).unwrap();
    }

    #[test]
    fn empty_library_returns_empty() {
        let mut d = db();
        let id = add_doc(&mut d, "", "/tmp/qqqq.pdf");
        assert!(suggest_tags_for_doc(&d, id).unwrap().is_empty());
    }

    #[test]
    fn nonexistent_doc_returns_empty() {
        let d = db();
        assert!(suggest_tags_for_doc(&d, 99999).unwrap().is_empty());
    }

    #[test]
    fn vocabulary_match_finds_existing_tag() {
        let mut d = db();
        // Seed an "invoice" tag onto some OTHER doc so it's in the vocab.
        let other = add_doc(&mut d, "Other", "/tmp/other.pdf");
        tag_doc(&mut d, other, &["invoice"]);
        // Target doc titled with "invoice" but untagged.
        let id = add_doc(&mut d, "Invoice 2026", "/tmp/inv2026.pdf");
        let s = suggest_tags_for_doc(&d, id).unwrap();
        let hit = s
            .iter()
            .find(|x| x.tag_name == "invoice")
            .expect("invoice suggested");
        assert_eq!(hit.source, "vocabulary");
        assert!(hit.existing);
    }

    #[test]
    fn cooccurrence_boost_pulls_in_related_tag() {
        let mut d = db();
        // 5 docs tagged ["invoice","2026client"] establishing co-occurrence.
        for i in 0..5 {
            let did = add_doc(&mut d, "x", &format!("/tmp/c{}.pdf", i));
            tag_doc(&mut d, did, &["invoice", "2026client"]);
        }
        // New doc already has "invoice"; expect "2026client" to surface.
        let id = add_doc(&mut d, "z", "/tmp/z.pdf");
        tag_doc(&mut d, id, &["invoice"]);
        let s = suggest_tags_for_doc(&d, id).unwrap();
        assert!(
            s.iter()
                .any(|x| x.tag_name == "2026client" && x.source == "cooccurrence"),
            "expected cooccurrence suggestion, got {:?}",
            s
        );
    }

    #[test]
    fn domain_hint_invoice_seeds_when_no_user_tag_exists() {
        let mut d = db();
        let id = add_doc(&mut d, "", "/tmp/INV-001.pdf");
        let s = suggest_tags_for_doc(&d, id).unwrap();
        let hit = s
            .iter()
            .find(|x| x.tag_name == "invoice")
            .expect("domain hint");
        assert_eq!(hit.source, "domain");
        assert!(!hit.existing, "no invoice tag exists yet");
    }

    #[test]
    fn dismissed_suggestion_is_filtered() {
        let mut d = db();
        let id = add_doc(&mut d, "", "/tmp/INV-001.pdf");
        assert!(suggest_tags_for_doc(&d, id)
            .unwrap()
            .iter()
            .any(|x| x.tag_name == "invoice"));
        dismiss_tag_suggestion(&d, id, "invoice").unwrap();
        assert!(is_tag_suggestion_dismissed(&d, id, "invoice").unwrap());
        assert!(suggest_tags_for_doc(&d, id)
            .unwrap()
            .iter()
            .all(|x| x.tag_name != "invoice"));
    }

    #[test]
    fn stopwords_and_short_tokens_dropped() {
        let mut d = db();
        // Seed tags matching stopwords so only the filter prevents a hit.
        let other = add_doc(&mut d, "o", "/tmp/o.pdf");
        tag_doc(&mut d, other, &["the", "pdf"]);
        let id = add_doc(&mut d, "the a of pdf", "/tmp/aa.pdf");
        let s = suggest_tags_for_doc(&d, id).unwrap();
        assert!(s.iter().all(|x| x.tag_name != "the" && x.tag_name != "pdf"));
    }

    #[test]
    fn case_and_punctuation_normalised() {
        let mut d = db();
        let other = add_doc(&mut d, "o", "/tmp/o.pdf");
        tag_doc(&mut d, other, &["tax"]);
        let id = add_doc(&mut d, "TAX_2025.pdf", "/tmp/TAX_2025.pdf");
        let s = suggest_tags_for_doc(&d, id).unwrap();
        assert!(s.iter().any(|x| x.tag_name == "tax"));
    }

    #[test]
    fn limit_top_k() {
        let mut d = db();
        // Create 20 tags on another doc, all matching tokens in the target.
        let other = add_doc(&mut d, "o", "/tmp/o.pdf");
        let names: Vec<String> = (0..20).map(|i| format!("alpha{:02}", i)).collect();
        tag_doc(
            &mut d,
            other,
            &names.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        let title = names.join(" ");
        let id = add_doc(&mut d, &title, "/tmp/many.pdf");
        let s = suggest_tags_for_doc(&d, id).unwrap();
        assert!(s.len() <= SUGGEST_LIMIT);
    }

    // ---- Slice 3: bulk ----

    #[test]
    fn bulk_skips_already_tagged() {
        let mut d = db();
        let tagged = add_doc(&mut d, "Invoice", "/tmp/inv.pdf");
        tag_doc(&mut d, tagged, &["invoice"]);
        let out = suggest_for_untagged(&d, 50).unwrap();
        assert!(out.iter().all(|b| b.doc_id != tagged));
    }

    #[test]
    fn bulk_skips_zero_suggestion_docs() {
        let mut d = db();
        // No title tokens, no domain hint → zero suggestions → skipped.
        add_doc(&mut d, "", "/tmp/zzz.pdf");
        let out = suggest_for_untagged(&d, 50).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn bulk_respects_limit() {
        let mut d = db();
        for i in 0..6 {
            add_doc(&mut d, "Invoice", &format!("/tmp/inv{}.pdf", i));
        }
        let out = suggest_for_untagged(&d, 3).unwrap();
        assert!(out.len() <= 3);
    }

    // ---- Slice 4: accept ----

    #[test]
    fn accept_creates_tag_when_missing() {
        let mut d = db();
        let id = add_doc(&mut d, "Invoice", "/tmp/inv.pdf");
        assert!(d.find_tag_by_name("invoice").unwrap().is_none());
        accept_tag_suggestion(&mut d, id, "invoice").unwrap();
        assert!(d.find_tag_by_name("invoice").unwrap().is_some());
    }

    #[test]
    fn accept_attaches_to_doc() {
        let mut d = db();
        let id = add_doc(&mut d, "Invoice", "/tmp/inv.pdf");
        accept_tag_suggestion(&mut d, id, "invoice").unwrap();
        let names: Vec<String> = d
            .tags_for_document(id)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(names.contains(&"invoice".to_string()));
    }

    #[test]
    fn accept_is_idempotent() {
        let mut d = db();
        let id = add_doc(&mut d, "Invoice", "/tmp/inv.pdf");
        accept_tag_suggestion(&mut d, id, "invoice").unwrap();
        accept_tag_suggestion(&mut d, id, "invoice").unwrap();
        let count = d.tags_for_document(id).unwrap().len();
        assert_eq!(count, 1);
    }

    #[test]
    fn accept_unions_with_existing_tags() {
        let mut d = db();
        let id = add_doc(&mut d, "Invoice", "/tmp/inv.pdf");
        tag_doc(&mut d, id, &["existing"]);
        accept_tag_suggestion(&mut d, id, "invoice").unwrap();
        let names: HashSet<String> = d
            .tags_for_document(id)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(names.contains("existing"));
        assert!(names.contains("invoice"));
    }

    #[test]
    fn pastel_for_is_deterministic() {
        assert_eq!(pastel_for("invoice"), pastel_for("invoice"));
        assert_ne!(pastel_for("invoice"), pastel_for("contract"));
        assert!(pastel_for("invoice").starts_with("hsl("));
    }

    #[test]
    fn undismiss_all_clears_dismissals() {
        let mut d = db();
        let id = add_doc(&mut d, "", "/tmp/INV-001.pdf");
        dismiss_tag_suggestion(&d, id, "invoice").unwrap();
        assert!(is_tag_suggestion_dismissed(&d, id, "invoice").unwrap());
        let n = undismiss_all_for_doc(&d, id).unwrap();
        assert_eq!(n, 1);
        assert!(!is_tag_suggestion_dismissed(&d, id, "invoice").unwrap());
    }

    // ---- Slice 48: bulk accept ----

    #[test]
    fn bulk_accept_attaches_all_pairs() {
        let mut d = db();
        let a = add_doc(&mut d, "Invoice", "/tmp/a.pdf");
        let b = add_doc(&mut d, "Receipt", "/tmp/b.pdf");
        let items = vec![
            AcceptItem {
                doc_id: a,
                tag_name: "invoice".into(),
            },
            AcceptItem {
                doc_id: b,
                tag_name: "receipt".into(),
            },
        ];
        let result = accept_tag_suggestions_bulk(&mut d, &items).unwrap();
        assert_eq!(result.attached.len(), 2);
        assert!(result.failed.is_empty());
        let a_tags: Vec<String> = d
            .tags_for_document(a)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        let b_tags: Vec<String> = d
            .tags_for_document(b)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(a_tags.contains(&"invoice".to_string()));
        assert!(b_tags.contains(&"receipt".to_string()));
    }

    #[test]
    fn bulk_accept_dedupes_repeated_pairs() {
        let mut d = db();
        let id = add_doc(&mut d, "Invoice", "/tmp/a.pdf");
        let items = vec![
            AcceptItem {
                doc_id: id,
                tag_name: "invoice".into(),
            },
            AcceptItem {
                doc_id: id,
                tag_name: "Invoice".into(), // Case dedupe after normalisation.
            },
            AcceptItem {
                doc_id: id,
                tag_name: "  invoice  ".into(), // Whitespace dedupe.
            },
        ];
        let result = accept_tag_suggestions_bulk(&mut d, &items).unwrap();
        // Three input items, one effective accept after dedupe.
        assert_eq!(result.attached.len(), 1);
        assert!(result.failed.is_empty());
        assert_eq!(d.tags_for_document(id).unwrap().len(), 1);
    }

    #[test]
    fn bulk_accept_collects_per_item_failures() {
        let mut d = db();
        let id = add_doc(&mut d, "Invoice", "/tmp/a.pdf");
        let items = vec![
            AcceptItem {
                doc_id: id,
                tag_name: "invoice".into(),
            },
            AcceptItem {
                doc_id: id,
                tag_name: "".into(), // Empty name -> fail.
            },
            AcceptItem {
                doc_id: id,
                tag_name: "   ".into(), // Whitespace-only -> fail.
            },
            AcceptItem {
                doc_id: id,
                tag_name: "receipt".into(),
            },
        ];
        let result = accept_tag_suggestions_bulk(&mut d, &items).unwrap();
        assert_eq!(result.attached.len(), 2);
        assert_eq!(result.failed.len(), 2);
        // Good items still landed even though items 1 + 2 failed.
        let tags: Vec<String> = d
            .tags_for_document(id)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(tags.contains(&"invoice".to_string()));
        assert!(tags.contains(&"receipt".to_string()));
    }

    #[test]
    fn bulk_accept_empty_input_is_noop() {
        let mut d = db();
        let result = accept_tag_suggestions_bulk(&mut d, &[]).unwrap();
        assert!(result.attached.is_empty());
        assert!(result.failed.is_empty());
    }

    #[test]
    fn bulk_accept_only_empty_names_collects_failures_no_attach() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/x.pdf");
        let items = vec![
            AcceptItem {
                doc_id: id,
                tag_name: "".into(),
            },
            AcceptItem {
                doc_id: id,
                tag_name: "  ".into(),
            },
        ];
        let result = accept_tag_suggestions_bulk(&mut d, &items).unwrap();
        assert!(result.attached.is_empty());
        assert_eq!(result.failed.len(), 2);
    }

    #[test]
    fn bulk_accept_creates_missing_tags() {
        let mut d = db();
        let id = add_doc(&mut d, "Brand New", "/tmp/n.pdf");
        assert!(d.find_tag_by_name("brandnew").unwrap().is_none());
        let result = accept_tag_suggestions_bulk(
            &mut d,
            &[AcceptItem {
                doc_id: id,
                tag_name: "brandnew".into(),
            }],
        )
        .unwrap();
        assert_eq!(result.attached.len(), 1);
        assert!(d.find_tag_by_name("brandnew").unwrap().is_some());
    }

    // ---- Slice 49: granular undismiss ----

    #[test]
    fn list_dismissed_returns_empty_when_no_dismissals() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/a.pdf");
        assert!(list_dismissed_for_doc(&d, id).unwrap().is_empty());
    }

    #[test]
    fn list_dismissed_returns_normalised_tag_names() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/a.pdf");
        dismiss_tag_suggestion(&d, id, "Invoice").unwrap();
        dismiss_tag_suggestion(&d, id, "TAX").unwrap();
        let rows = list_dismissed_for_doc(&d, id).unwrap();
        assert_eq!(rows.len(), 2);
        let names: HashSet<String> = rows.iter().map(|r| r.tag_name.clone()).collect();
        assert!(names.contains("invoice"));
        assert!(names.contains("tax"));
    }

    #[test]
    fn list_dismissed_orders_newest_first() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/a.pdf");
        // dismiss_tag_suggestion stamps now() so we manually insert with
        // controlled timestamps to pin the ordering.
        d.conn()
            .execute(
                "INSERT INTO library_tag_suggestion_dismissed
                 (doc_id, tag_name, dismissed_at) VALUES (?, 'older', 1000)",
                params![id],
            )
            .unwrap();
        d.conn()
            .execute(
                "INSERT INTO library_tag_suggestion_dismissed
                 (doc_id, tag_name, dismissed_at) VALUES (?, 'newer', 2000)",
                params![id],
            )
            .unwrap();
        let rows = list_dismissed_for_doc(&d, id).unwrap();
        assert_eq!(rows[0].tag_name, "newer");
        assert_eq!(rows[1].tag_name, "older");
        assert_eq!(rows[0].dismissed_at, 2000);
    }

    #[test]
    fn list_dismissed_isolates_by_doc() {
        let mut d = db();
        let a = add_doc(&mut d, "x", "/tmp/a.pdf");
        let b = add_doc(&mut d, "x", "/tmp/b.pdf");
        dismiss_tag_suggestion(&d, a, "invoice").unwrap();
        dismiss_tag_suggestion(&d, b, "receipt").unwrap();
        let a_rows = list_dismissed_for_doc(&d, a).unwrap();
        let b_rows = list_dismissed_for_doc(&d, b).unwrap();
        assert_eq!(a_rows.len(), 1);
        assert_eq!(b_rows.len(), 1);
        assert_eq!(a_rows[0].tag_name, "invoice");
        assert_eq!(b_rows[0].tag_name, "receipt");
    }

    #[test]
    fn undismiss_one_clears_only_matching_row() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/a.pdf");
        dismiss_tag_suggestion(&d, id, "invoice").unwrap();
        dismiss_tag_suggestion(&d, id, "tax").unwrap();
        let cleared = undismiss_one_for_doc(&d, id, "invoice").unwrap();
        assert!(cleared);
        assert!(!is_tag_suggestion_dismissed(&d, id, "invoice").unwrap());
        assert!(is_tag_suggestion_dismissed(&d, id, "tax").unwrap());
    }

    #[test]
    fn undismiss_one_returns_false_when_no_match() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/a.pdf");
        let cleared = undismiss_one_for_doc(&d, id, "never-dismissed").unwrap();
        assert!(!cleared);
    }

    #[test]
    fn undismiss_one_case_insensitive_match() {
        let mut d = db();
        let id = add_doc(&mut d, "x", "/tmp/a.pdf");
        dismiss_tag_suggestion(&d, id, "Invoice").unwrap();
        // Different casing on the undismiss path still matches because
        // both ends lowercase before comparison.
        let cleared = undismiss_one_for_doc(&d, id, "INVOICE").unwrap();
        assert!(cleared);
        assert!(!is_tag_suggestion_dismissed(&d, id, "invoice").unwrap());
    }
}
