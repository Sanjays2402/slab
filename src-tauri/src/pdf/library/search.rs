//! Cross-document full-text search backed by FTS5.
//!
//! Translates a user query into an FTS5 MATCH expression, runs it against
//! `library_fts`, joins back to `library_documents` for path + title, and
//! returns a ranked list of [`SearchHit`]s with bm25 ranking and
//! `<mark>`-wrapped snippets. Used by the `slab_library_search` Tauri
//! command (Slice 2 of v2.2.0 Atlas).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::registry::LibraryError;

/// Maximum characters in a returned snippet. FTS5's `snippet()` aux
/// function will truncate beyond this when given the column-width arg.
pub const SNIPPET_TOKEN_BUDGET: i32 = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub doc_id: i64,
    pub path: String,
    pub title: Option<String>,
    pub page_index: i64,
    /// Snippet with FTS5 `<mark>…</mark>` wrappers around matched terms.
    pub snippet: String,
    /// bm25 rank (lower = better in FTS5; we negate for caller sanity).
    pub rank: f64,
}

/// Run a search against the FTS5 index.
///
/// Arguments:
/// * `query` — raw user input. We escape and quote it as a single
///   FTS5 phrase so users can type natural words (no MATCH syntax
///   required). An empty / whitespace-only query returns `Ok(vec![])`.
/// * `limit` — hard cap on returned rows. Clamped to `1..=500`.
/// * `folder_id` — when `Some`, restrict to docs in that folder.
///
/// Returns hits sorted by bm25 ascending (best first).
pub fn search(
    conn: &Connection,
    query: &str,
    limit: u32,
    folder_id: Option<i64>,
) -> Result<Vec<SearchHit>, LibraryError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let match_expr = build_match_expr(trimmed);
    if match_expr.is_empty() {
        // Query was all FTS5 metacharacters / punctuation — no tokens.
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 500) as i64;

    // We can't bind FTS5 MATCH params via `?` for `library_fts` MATCH
    // operand on all rusqlite versions; passing a string param is fine,
    // but the snippet() column index argument must be a literal.
    let (sql, joined_params): (&'static str, Vec<rusqlite::types::Value>) =
        if let Some(fid) = folder_id {
            (
                "SELECT d.id, d.path, d.title, f.page_index,
                    snippet(library_fts, 2, '<mark>', '</mark>', ' … ', 16) AS snippet,
                    bm25(library_fts) AS rank
             FROM library_fts AS f
             JOIN library_documents AS d ON d.id = f.doc_id
             WHERE library_fts MATCH ?1
               AND d.folder_id = ?2
             ORDER BY rank
             LIMIT ?3",
                vec![match_expr.into(), fid.into(), limit.into()],
            )
        } else {
            (
                "SELECT d.id, d.path, d.title, f.page_index,
                    snippet(library_fts, 2, '<mark>', '</mark>', ' … ', 16) AS snippet,
                    bm25(library_fts) AS rank
             FROM library_fts AS f
             JOIN library_documents AS d ON d.id = f.doc_id
             WHERE library_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
                vec![match_expr.into(), limit.into()],
            )
        };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(joined_params.iter()), |row| {
        Ok(SearchHit {
            doc_id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            page_index: row.get(3)?,
            snippet: row.get(4)?,
            rank: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Build an FTS5 MATCH expression from raw user input.
///
/// We do NOT pass user input to FTS5 verbatim — it has its own query
/// syntax (`AND`, `OR`, `NOT`, `*` wildcards, column filters, quoted
/// phrases) and unescaped input can blow up with `SQL logic error`. The
/// safe approach is:
///
/// 1. Strip FTS5 metacharacters (`"^*-:()`) from each token.
/// 2. Wrap each non-empty token in double quotes (FTS5 phrase syntax).
/// 3. Append `*` AFTER the closing quote on the LAST token to enable
///    prefix-search-as-you-type (so `indemn` matches `indemnification`).
///
/// Result for input `indemnification clause` → `"indemnification" "clause"*`.
pub fn build_match_expr(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| !matches!(c, '"' | '^' | '*' | '-' | ':' | '(' | ')'))
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect();
    if tokens.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = tokens.iter().map(|t| format!("\"{}\"", t)).collect();
    // Prefix-search on the last token.
    if let Some(last) = parts.last_mut() {
        last.push('*');
    }
    parts.join(" ")
}

/// Helper for the indexer — also useful in tests. Currently unused outside
/// of tests but exposed so future callers (e.g. an "Index Status" panel)
/// can poll without a fresh function plumb.
pub fn count_indexed_docs(conn: &Connection) -> Result<i64, LibraryError> {
    let n: i64 = conn.query_row("SELECT count(DISTINCT doc_id) FROM library_fts", [], |r| {
        r.get(0)
    })?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::library::{fts, registry::LibraryDb};

    fn seed(db: &LibraryDb) {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO library_folders (id, path, added_at) VALUES (1, '/x', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_folders (id, path, added_at) VALUES (2, '/y', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_documents (id, folder_id, path, title, hash, size_bytes, mtime_ns, added_at, last_seen_at)
             VALUES (1, 1, '/x/a.pdf', 'Contract A', 'h', 1, 0, 0, 0),
                    (2, 1, '/x/b.pdf', 'Contract B', 'h', 1, 0, 0, 0),
                    (3, 2, '/y/c.pdf', NULL, 'h', 1, 0, 0, 0)",
            [],
        )
        .unwrap();
        fts::index_doc(
            conn,
            1,
            &["The indemnification clause is here on page one".into()],
        )
        .unwrap();
        fts::index_doc(
            conn,
            2,
            &[
                "arbitration only on page zero".into(),
                "indemnification on page two".into(),
            ],
        )
        .unwrap();
        fts::index_doc(conn, 3, &["unrelated content lives in folder y".into()]).unwrap();
    }

    #[test]
    fn build_match_expr_quotes_each_token() {
        assert_eq!(build_match_expr("hello world"), "\"hello\" \"world\"*");
    }

    #[test]
    fn build_match_expr_strips_metacharacters() {
        // Stars, quotes, colons must not survive — they'd parse as FTS5
        // operators and could either match nothing or error out.
        assert_eq!(
            build_match_expr(r#"foo* "bar" baz:qux"#),
            "\"foo\" \"bar\" \"bazqux\"*"
        );
    }

    #[test]
    fn build_match_expr_handles_empty() {
        assert_eq!(build_match_expr(""), "");
        assert_eq!(build_match_expr("   "), "");
        assert_eq!(build_match_expr("**"), "");
    }

    #[test]
    fn single_word_query_returns_ranked_hits() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), "indemnification", 10, None).unwrap();
        assert_eq!(hits.len(), 2);
        // Both hits hit page 0 (doc 1) and page 1 (doc 2).
        assert!(hits.iter().any(|h| h.doc_id == 1 && h.page_index == 0));
        assert!(hits.iter().any(|h| h.doc_id == 2 && h.page_index == 1));
    }

    #[test]
    fn snippet_wraps_match_in_mark_tags() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), "arbitration", 10, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].snippet.contains("<mark>arbitration</mark>"),
            "snippet was {:?}",
            hits[0].snippet
        );
    }

    #[test]
    fn folder_filter_restricts_results() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        // Folder 2 only has doc 3 which doesn't mention indemnification.
        let hits = search(db.conn(), "indemnification", 10, Some(2)).unwrap();
        assert!(hits.is_empty());
        let hits = search(db.conn(), "indemnification", 10, Some(1)).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn limit_caps_result_count() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), "indemnification", 1, None).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn empty_query_returns_empty() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        assert!(search(db.conn(), "", 10, None).unwrap().is_empty());
        assert!(search(db.conn(), "   ", 10, None).unwrap().is_empty());
    }

    #[test]
    fn prefix_search_matches_partial_last_token() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        // "indemn" with prefix-search-on-last-token should match
        // "indemnification" in both docs.
        let hits = search(db.conn(), "indemn", 10, None).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn title_is_returned_when_set() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), "indemnification", 10, None).unwrap();
        let doc1 = hits.iter().find(|h| h.doc_id == 1).unwrap();
        assert_eq!(doc1.title.as_deref(), Some("Contract A"));
        let doc2 = hits.iter().find(|h| h.doc_id == 2).unwrap();
        assert_eq!(doc2.title.as_deref(), Some("Contract B"));
    }

    #[test]
    fn count_indexed_docs_matches_seeded_set() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        assert_eq!(count_indexed_docs(db.conn()).unwrap(), 3);
    }

    #[test]
    fn multi_word_query_uses_and_semantics() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        // "indemnification clause" should match only doc 1 (page 0)
        // because doc 2 splits those words across pages.
        let hits = search(db.conn(), "indemnification clause", 10, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, 1);
    }

    #[test]
    fn malicious_input_does_not_blow_up() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        // These would all crash an unguarded FTS5 MATCH.
        for bad in ["\"", "*", "(", ":foo", "NEAR/3", "AND OR"] {
            let _ = search(db.conn(), bad, 10, None).unwrap();
        }
    }
}
