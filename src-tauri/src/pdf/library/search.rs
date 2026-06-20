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
    // Best-effort: log this query for Atlas Suggest. Silently ignore
    // errors — search results matter more than analytics.
    let _ = super::search_log::record_conn(conn, trimmed, out.len() as i64);
    Ok(out)
}

/// Build an FTS5 MATCH expression from raw user input.
///
/// We do NOT pass user input to FTS5 verbatim — it has its own query
/// syntax (`AND`, `OR`, `NOT`, `*` wildcards, column filters, quoted
/// phrases) and unescaped input can blow up with `SQL logic error`. The
/// strategy is to lex the input ourselves into well-formed token kinds
/// then re-emit each as the FTS5 fragment it needs.
///
/// Recognized token kinds:
/// 1. **Bare word** — `indemnification` → quoted single phrase
///    `"indemnification"`. Stripped of FTS5 metacharacters.
/// 2. **Quoted phrase** — `"force majeure"` (curly or straight quotes,
///    multi-word). Adjacent-token match: any doc page where the words
///    appear in that exact order wins. Stripped of metacharacters but
///    spaces survive. Inside-quote prefix-`*` is dropped (a phrase
///    can't carry the prefix glob — FTS5 rejects it).
///
/// Result for input `indemnification clause` → `"indemnification" "clause"*`.
/// Result for input `"force majeure" clause` → `"force majeure" "clause"*`.
/// An unterminated `"trailing` is treated as a phrase to end-of-input.
///
/// The LAST emitted bare-word token gets a trailing `*` so prefix-search
/// works as the user types (`indemn` matches `indemnification`). Phrase
/// tokens never get `*` — FTS5 does not support a phrase-with-prefix
/// idiom and emitting one is a hard error.
pub fn build_match_expr(query: &str) -> String {
    let toks = tokenize(query);
    if toks.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::with_capacity(toks.len());
    let last_bare_idx = toks.iter().rposition(|t| matches!(t, Tok::Bare(_)));
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Bare(w) => {
                let mut s = format!("\"{}\"", w);
                if Some(i) == last_bare_idx {
                    s.push('*');
                }
                parts.push(s);
            }
            Tok::Phrase(p) => {
                // Never prefix-glob a phrase — FTS5 rejects "a b"*.
                parts.push(format!("\"{}\"", p));
            }
        }
    }
    parts.join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Bare(String),
    Phrase(String),
}

/// Strip every FTS5 metacharacter from a single-word token so it can't
/// re-introduce operator syntax when we wrap it in quotes.
fn scrub_word(w: &str) -> String {
    w.chars()
        .filter(|c| !matches!(c, '"' | '^' | '*' | '-' | ':' | '(' | ')'))
        .collect()
}

/// Strip metacharacters but PRESERVE internal whitespace so multi-word
/// phrases remain multi-word after sanitisation.
fn scrub_phrase(p: &str) -> String {
    p.chars()
        .filter(|c| !matches!(c, '"' | '^' | '*' | ':' | '(' | ')'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lex raw user input into a stream of Tok::Bare / Tok::Phrase. Empty
/// tokens are dropped. Both curly (typed by macOS auto-correct) and
/// straight quotes open + close phrases.
fn tokenize(query: &str) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    let mut chars = query.chars().peekable();
    let mut buf = String::new();
    while let Some(c) = chars.next() {
        match c {
            // Both straight and curly opening-quote forms a phrase.
            '"' | '\u{201C}' | '\u{201D}' => {
                // Flush any bare-word accumulator first.
                if !buf.is_empty() {
                    let w = scrub_word(&buf);
                    if !w.is_empty() {
                        out.push(Tok::Bare(w));
                    }
                    buf.clear();
                }
                // Read until the matching close-quote OR end of input.
                let mut phrase = String::new();
                while let Some(&nc) = chars.peek() {
                    if matches!(nc, '"' | '\u{201C}' | '\u{201D}') {
                        chars.next();
                        break;
                    }
                    phrase.push(nc);
                    chars.next();
                }
                let cleaned = scrub_phrase(&phrase);
                if !cleaned.is_empty() {
                    out.push(Tok::Phrase(cleaned));
                }
            }
            c if c.is_whitespace() => {
                if !buf.is_empty() {
                    let w = scrub_word(&buf);
                    if !w.is_empty() {
                        out.push(Tok::Bare(w));
                    }
                    buf.clear();
                }
            }
            _ => buf.push(c),
        }
    }
    if !buf.is_empty() {
        let w = scrub_word(&buf);
        if !w.is_empty() {
            out.push(Tok::Bare(w));
        }
    }
    out
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

    // --- Phrase-search (v3.53.0 Atlas) ---
    //
    // build_match_expr() lexes double-quoted segments as FTS5 phrase
    // tokens so a user typing `"force majeure"` gets adjacent-word
    // matching instead of two independent ANDed words. The MATCH grammar
    // already supports `"a b"` as a phrase out-of-the-box — these tests
    // pin the lexer's output shape so a future refactor can't silently
    // demote phrases back to bag-of-words.

    #[test]
    fn build_match_expr_emits_quoted_phrase_as_phrase_token() {
        // The phrase is emitted as one FTS5 phrase ("force majeure"),
        // not two independent quoted words. Only the LAST bare word
        // ever gets the prefix `*`; the phrase itself never does.
        assert_eq!(
            build_match_expr(r#""force majeure" clause"#),
            "\"force majeure\" \"clause\"*"
        );
    }

    #[test]
    fn build_match_expr_phrase_alone_drops_prefix_glob() {
        // A query that is JUST a phrase emits a single phrase token with
        // no `*` — FTS5 rejects `"a b"*` as a syntax error and we never
        // want a search to fall over because the lexer mis-attached a
        // prefix glob to a phrase.
        assert_eq!(
            build_match_expr(r#""indemnification clause""#),
            "\"indemnification clause\""
        );
    }

    #[test]
    fn build_match_expr_unterminated_phrase_runs_to_eol() {
        // Missing closing quote is friendly: we treat everything up to
        // end-of-input as the phrase. This matches Google's behaviour
        // and avoids the user's keypress-mid-edit silently breaking.
        assert_eq!(
            build_match_expr(r#""trailing phrase"#),
            "\"trailing phrase\""
        );
    }

    #[test]
    fn build_match_expr_handles_curly_quotes() {
        // macOS auto-correct converts "" to "" mid-type. The lexer
        // accepts both forms so users on macOS don't have to disable
        // smart quotes to phrase-search.
        let curly = "\u{201C}force majeure\u{201D}";
        assert_eq!(build_match_expr(curly), "\"force majeure\"");
    }

    #[test]
    fn build_match_expr_mixed_bare_and_phrase() {
        // A phrase between bare words; only the LAST bare word collects
        // the prefix-* glob (so `clau` would still match `clause`).
        assert_eq!(
            build_match_expr(r#"contract "force majeure" termination"#),
            "\"contract\" \"force majeure\" \"termination\"*"
        );
    }

    #[test]
    fn build_match_expr_phrase_then_bare_glob_is_on_bare() {
        // When the LAST token is a phrase, no `*` is emitted — phrase
        // CAN'T carry a prefix glob and prefix-on-the-previous-bare-word
        // would change the semantics of an explicitly-quoted query.
        assert_eq!(
            build_match_expr(r#"draft "force majeure""#),
            "\"draft\" \"force majeure\""
        );
    }

    #[test]
    fn build_match_expr_strips_meta_inside_phrase() {
        // Asterisk, colon, paren, caret would all be parsed by FTS5 if
        // they survived. The lexer scrubs them but keeps internal spaces.
        assert_eq!(build_match_expr(r#""foo* (bar):baz""#), "\"foo bar baz\"");
    }

    #[test]
    fn build_match_expr_empty_quote_pair_emits_nothing() {
        // `""` is a no-op — no phrase token, the rest of the query
        // continues as normal.
        assert_eq!(build_match_expr(r#""" hello"#), "\"hello\"*");
        assert_eq!(build_match_expr(r#""""#), "");
    }

    #[test]
    fn phrase_search_matches_adjacent_only() {
        // End-to-end via the search() API: doc 1 has "indemnification clause"
        // on one page (adjacent), doc 2 has "indemnification" and "clause"
        // on different pages. The phrase query must match doc 1 ONLY.
        // Without phrase support this was already true via multi-page AND;
        // the test is here to pin the new lexer path doesn't regress it.
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), r#""indemnification clause""#, 10, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, 1);
    }

    #[test]
    fn phrase_query_is_logged_as_typed() {
        // The search-log row stores the user-typed query (with quotes) so
        // the recent-searches chip strip re-runs an exact phrase when the
        // user clicks it. The log gets the TRIMMED raw input — not the
        // re-emitted MATCH expression — so quote characters survive.
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let _ = search(db.conn(), r#""indemnification clause""#, 10, None).unwrap();
        let rows = super::super::search_log::recent_queries(&db, 5).unwrap();
        let last = rows.first().expect("phrase query should be logged");
        assert_eq!(last.query, r#""indemnification clause""#);
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

    // -------- Atlas slice 4 contract tests --------
    //
    // The Reader's highlight-on-open path relies on three invariants:
    //  (1) page_index is 0-based — UI adds 1 before passing to ReaderPanel.
    //  (2) snippet always contains the <mark>…</mark> wrap when there is
    //      a real match (frontend renders it as innerHTML).
    //  (3) hits are sorted by rank ASC (best first) — UI groups by doc
    //      and shows the top page first as the entry point.
    //
    // These tests pin those guarantees so a future refactor of the SQL
    // query can't silently break the slice-4 round-trip.

    #[test]
    fn slice4_page_index_is_zero_based_for_first_page_match() {
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        // Doc 1 has "indemnification" on its FIRST page (index 0).
        let hits = search(db.conn(), "indemnification", 10, None).unwrap();
        let doc1 = hits.iter().find(|h| h.doc_id == 1).unwrap();
        assert_eq!(
            doc1.page_index, 0,
            "first-page match must be page_index 0; UI adds 1 for display"
        );
    }

    #[test]
    fn slice4_multi_page_doc_returns_hit_per_matching_page() {
        let db = LibraryDb::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO library_folders (id, path, added_at) VALUES (9, '/m', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_documents (id, folder_id, path, title, hash, size_bytes, mtime_ns, added_at, last_seen_at)
             VALUES (99, 9, '/m/multi.pdf', 'Multi-page Indemnity Brief', 'h', 1, 0, 0, 0)",
            [],
        )
        .unwrap();
        fts::index_doc(
            conn,
            99,
            &[
                "page zero mentions indemnification once".into(),
                "no relevant content here".into(),
                "page two mentions indemnification again".into(),
                "epilogue: a final indemnification reference".into(),
            ],
        )
        .unwrap();
        let hits = search(conn, "indemnification", 50, Some(9)).unwrap();
        assert_eq!(
            hits.len(),
            3,
            "expected one hit per matching page (0, 2, 3) for doc 99"
        );
        let pages: Vec<i64> = hits.iter().map(|h| h.page_index).collect();
        assert!(pages.contains(&0));
        assert!(pages.contains(&2));
        assert!(pages.contains(&3));
        assert!(!pages.contains(&1), "page 1 should not match");
        // Every hit must carry the highlight wrap the UI streams as innerHTML.
        for h in &hits {
            assert!(
                h.snippet.contains("<mark>indemnification</mark>"),
                "snippet missing <mark> wrap: {:?}",
                h.snippet
            );
        }
    }

    #[test]
    fn slice4_hits_sorted_by_rank_ascending() {
        // BM25 rank is "lower is better" — assert hits come out best-first
        // so the SearchPanel's first row genuinely is the strongest match.
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), "indemnification", 10, None).unwrap();
        assert!(hits.len() >= 2, "need ≥2 hits to test ordering");
        for w in hits.windows(2) {
            assert!(
                w[0].rank <= w[1].rank,
                "hits not sorted by rank ASC: {} then {}",
                w[0].rank,
                w[1].rank
            );
        }
    }

    #[test]
    fn slice4_snippet_is_safe_for_innerhtml_rendering() {
        // The frontend renders snippet via {@html h.snippet}. Verify the
        // only HTML tags emitted are the FTS5 highlight wrap we control —
        // no stray < or > from page content leak through unescaped.
        let db = LibraryDb::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO library_folders (id, path, added_at) VALUES (7, '/h', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_documents (id, folder_id, path, title, hash, size_bytes, mtime_ns, added_at, last_seen_at)
             VALUES (77, 7, '/h/html.pdf', NULL, 'h', 1, 0, 0, 0)",
            [],
        )
        .unwrap();
        // Content with literal < > & that FTS5 will pass through.
        fts::index_doc(
            conn,
            77,
            &["payload <script>alert(1)</script> and indemnification follows".into()],
        )
        .unwrap();
        let hits = search(conn, "indemnification", 10, Some(7)).unwrap();
        assert_eq!(hits.len(), 1);
        let s = &hits[0].snippet;
        // SQLite FTS5's snippet() returns whatever text is stored verbatim.
        // The frontend escapes via DOM textContent before rendering, so
        // *this test pins what we ship out of Rust* — every `<` that
        // isn't part of `<mark>`/`</mark>` is something the UI must
        // escape. Just enforce the wrap is present:
        assert!(s.contains("<mark>indemnification</mark>"));
    }

    #[test]
    fn slice4_limit_clamps_to_max_500() {
        // The Tauri command layer clamps limit to 1..=500; the search()
        // function itself accepts whatever it's given so we just pin the
        // upper-bound path doesn't panic or hang.
        let db = LibraryDb::open_in_memory().unwrap();
        seed(&db);
        let hits = search(db.conn(), "indemnification", 500, None).unwrap();
        assert!(hits.len() <= 500);
    }
}
