//! Library auto-tagger — Slice 6 of v0.13.0 Lens.
//!
//! Wires the existing `ai::auto_tag` module against the registry: given a
//! `doc_id`, extract the document's text, call the configured `AiProvider`
//! for tag suggestions, materialise those tags as real `library_tags` rows
//! (idempotent — `add_tag` is a find-or-create), and attach them to the doc
//! by unioning with whatever tags the user has already set.
//!
//! Design contract:
//! - **Never throws on a per-doc failure.** Like the OCR queue, every error
//!   path collapses into an `AutoTagRunResult { error: Some(...) }` so the
//!   bulk action can keep going.
//! - **Additive only.** The orchestrator never *removes* a tag the user
//!   added by hand. It computes `final = existing ∪ suggested` and writes
//!   that.
//! - **Synchronous-ish.** Tauri's async runtime wraps the call; the registry
//!   layer is sync under the hood.

use super::registry::{LibraryDb, TagRecord};
use crate::ai::auto_tag::{auto_tag_from_path, AutoTagOpts};
use crate::ai::AiProvider;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Bundled result for one auto-tag pass. `error: None` means the doc was
/// tagged successfully; `error: Some(msg)` means we tried and failed — the
/// doc's existing tags are still intact, only `tags_assigned` will be empty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoTagRunResult {
    pub doc_id: i64,
    /// Newly-resolved final tag set on the doc — names only, lower-case.
    /// Includes pre-existing tags the user had set by hand.
    pub tags_assigned: Vec<String>,
    /// `library_tags.id` for every entry in `tags_assigned`. Same order.
    pub tag_ids: Vec<i64>,
    /// Captured error message on failure. `None` on success.
    pub error: Option<String>,
}

fn err_result(doc_id: i64, msg: impl Into<String>) -> AutoTagRunResult {
    AutoTagRunResult {
        doc_id,
        tags_assigned: Vec::new(),
        tag_ids: Vec::new(),
        error: Some(msg.into()),
    }
}

/// Find a single document row by id. The registry doesn't expose this
/// helper at the type level yet, so we inline the SQL here (small + local).
fn lookup_doc_path(db: &LibraryDb, doc_id: i64) -> Result<Option<String>, rusqlite::Error> {
    let conn = db.conn();
    match conn.query_row(
        "SELECT path FROM library_documents WHERE id = ?1",
        rusqlite::params![doc_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Run auto-tag on one document. Returns an `AutoTagRunResult` — never
/// panics, never throws. Inspect `error` to detect failure.
pub async fn run_one(
    db: &mut LibraryDb,
    provider: Arc<dyn AiProvider>,
    doc_id: i64,
    opts: &AutoTagOpts,
) -> AutoTagRunResult {
    // 1. Resolve path.
    let path = match lookup_doc_path(db, doc_id) {
        Ok(Some(p)) => p,
        Ok(None) => return err_result(doc_id, format!("doc id {doc_id} not in library")),
        Err(e) => return err_result(doc_id, format!("library lookup failed: {e}")),
    };

    // 2. Ask the provider.
    let pdf_path = PathBuf::from(&path);
    let tag_call = auto_tag_from_path(provider, &pdf_path, opts).await;
    let result = match tag_call {
        Ok(r) => r,
        Err(e) => return err_result(doc_id, format!("auto-tag failed: {e}")),
    };

    // 3. Materialise tag rows (find-or-create) and collect ids.
    //    Build the union with the user's existing tags so we never remove
    //    a hand-set tag.
    let existing: Vec<TagRecord> = match db.tags_for_document(doc_id) {
        Ok(v) => v,
        Err(e) => return err_result(doc_id, format!("read existing tags: {e}")),
    };
    let mut final_names: Vec<String> = existing.iter().map(|t| t.name.clone()).collect();
    let mut final_ids: Vec<i64> = existing.iter().map(|t| t.id).collect();
    for suggested in &result.tags {
        // Skip exact-name duplicates of existing tags. Tag names are
        // lower-case so a case-insensitive compare is just `==`.
        if final_names.iter().any(|n| n == suggested) {
            continue;
        }
        match db.add_tag(suggested, None) {
            Ok(tag) => {
                final_names.push(tag.name);
                final_ids.push(tag.id);
            }
            Err(e) => {
                // One tag failing to insert shouldn't poison the rest;
                // skip it and keep going. (No structured logger wired
                // in src-tauri yet; eprintln is the convention used by
                // the rest of the crate.)
                eprintln!("auto_tagger: add_tag({suggested}) failed: {e}");
            }
        }
    }

    // 4. Set the union as the doc's tag set.
    if let Err(e) = db.set_doc_tags(doc_id, &final_ids) {
        return err_result(doc_id, format!("set_doc_tags: {e}"));
    }

    AutoTagRunResult {
        doc_id,
        tags_assigned: final_names,
        tag_ids: final_ids,
        error: None,
    }
}

/// Run auto-tag over many documents, sequentially. Continues past per-doc
/// errors — the caller can inspect the per-doc `error` field.
pub async fn run_many(
    db: &mut LibraryDb,
    provider: Arc<dyn AiProvider>,
    doc_ids: &[i64],
    opts: &AutoTagOpts,
) -> Vec<AutoTagRunResult> {
    let mut out = Vec::with_capacity(doc_ids.len());
    for id in doc_ids {
        let r = run_one(db, provider.clone(), *id, opts).await;
        out.push(r);
    }
    out
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiError, ChatMessage, ChatOpts, ChatResponse};
    use crate::pdf::library::registry::OCR_STATE_TEXT_NATIVE;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Mock AiProvider that returns a canned reply for `chat()`. Tests
    /// can also opt in to an `extract_failure` mode where it isn't called
    /// because we deliberately wire a missing PDF path.
    struct MockProvider {
        reply: String,
        called: Mutex<u32>,
    }
    impl MockProvider {
        fn new(reply: impl Into<String>) -> Self {
            Self {
                reply: reply.into(),
                called: Mutex::new(0),
            }
        }
    }
    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            _msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            *self.called.lock().unwrap() += 1;
            Ok(ChatResponse {
                content: self.reply.clone(),
                model: "mock-tag:test".into(),
            })
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            unimplemented!()
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    /// Create an on-disk PDF (real lopdf) at `dir/name` and register it in
    /// the library DB. Returns `doc_id`.
    fn seed_real_pdf(db: &mut LibraryDb, dir: &std::path::Path, name: &str) -> (i64, PathBuf) {
        let pdf_path = dir.join(name);
        crate::pdf::test_fixtures::make_n_page_pdf(&pdf_path, 1);
        let folder = db.add_folder(&dir.to_string_lossy()).unwrap();
        let doc = db
            .upsert_document(
                Some(folder.id),
                &pdf_path.to_string_lossy(),
                Some(name),
                "h",
                1,
                1,
                Some(1),
                Some(OCR_STATE_TEXT_NATIVE),
            )
            .unwrap();
        (doc.id, pdf_path)
    }

    #[tokio::test]
    async fn run_one_assigns_tags_to_existing_doc() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db = LibraryDb::open_in_memory().unwrap();
        let (doc_id, _pdf) = seed_real_pdf(&mut db, tmp.path(), "a.pdf");

        let provider: Arc<dyn AiProvider> = Arc::new(MockProvider::new("a, b"));
        let r = run_one(&mut db, provider, doc_id, &AutoTagOpts::default()).await;

        assert!(r.error.is_none(), "expected success, got {:?}", r.error);
        assert_eq!(r.tags_assigned, vec!["a", "b"]);
        assert_eq!(r.tag_ids.len(), 2);
        // Sanity-check the registry round-tripped.
        let attached = db.tags_for_document(doc_id).unwrap();
        let names: Vec<String> = attached.iter().map(|t| t.name.clone()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn run_one_preserves_existing_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db = LibraryDb::open_in_memory().unwrap();
        let (doc_id, _pdf) = seed_real_pdf(&mut db, tmp.path(), "b.pdf");
        // Pre-attach an existing tag X.
        let x = db.add_tag("x", None).unwrap();
        db.set_doc_tags(doc_id, &[x.id]).unwrap();

        let provider: Arc<dyn AiProvider> = Arc::new(MockProvider::new("y"));
        let r = run_one(&mut db, provider, doc_id, &AutoTagOpts::default()).await;
        assert!(r.error.is_none());
        // Both X (existing) and y (suggested) should be on the doc.
        let mut names: Vec<String> = r.tags_assigned.to_vec();
        names.sort();
        assert_eq!(names, vec!["x".to_string(), "y".to_string()]);
    }

    #[tokio::test]
    async fn run_one_dedupes_existing_tag_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db = LibraryDb::open_in_memory().unwrap();
        let (doc_id, _pdf) = seed_real_pdf(&mut db, tmp.path(), "c.pdf");
        let a = db.add_tag("a", None).unwrap();
        db.set_doc_tags(doc_id, &[a.id]).unwrap();

        let provider: Arc<dyn AiProvider> = Arc::new(MockProvider::new("a, b"));
        let r = run_one(&mut db, provider, doc_id, &AutoTagOpts::default()).await;
        assert!(r.error.is_none());
        // a should NOT appear twice; final set should be {a, b}.
        let mut names = r.tags_assigned.clone();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(r.tag_ids.len(), 2);
    }

    #[tokio::test]
    async fn run_one_missing_doc_errors_via_result() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let provider: Arc<dyn AiProvider> = Arc::new(MockProvider::new("x"));
        let r = run_one(&mut db, provider.clone(), 9999, &AutoTagOpts::default()).await;
        assert!(r.error.is_some(), "expected error for missing doc id");
        let msg = r.error.unwrap();
        assert!(msg.contains("9999") && msg.contains("not in library"));
        assert!(r.tags_assigned.is_empty());
    }

    #[tokio::test]
    async fn run_one_extract_failure_returns_error() {
        // Seed a DB row pointing at a path that doesn't exist on disk.
        let mut db = LibraryDb::open_in_memory().unwrap();
        let folder = db.add_folder("/tmp/nope").unwrap();
        let doc = db
            .upsert_document(
                Some(folder.id),
                "/tmp/nope/missing.pdf",
                None,
                "h",
                1,
                1,
                None,
                Some(OCR_STATE_TEXT_NATIVE),
            )
            .unwrap();
        let provider: Arc<dyn AiProvider> = Arc::new(MockProvider::new("x"));
        let r = run_one(&mut db, provider, doc.id, &AutoTagOpts::default()).await;
        assert!(r.error.is_some(), "extract should fail on missing file");
        assert!(r.error.unwrap().contains("auto-tag failed"));
    }

    #[tokio::test]
    async fn run_many_continues_past_per_doc_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db = LibraryDb::open_in_memory().unwrap();
        let (ok1, _) = seed_real_pdf(&mut db, tmp.path(), "ok1.pdf");
        // Middle doc has a path that doesn't exist on disk → extract fails.
        let folder = db
            .find_folder_by_path(&tmp.path().to_string_lossy())
            .unwrap()
            .unwrap();
        let bad = db
            .upsert_document(
                Some(folder.id),
                &tmp.path().join("nope.pdf").to_string_lossy(),
                None,
                "h",
                1,
                1,
                None,
                Some(OCR_STATE_TEXT_NATIVE),
            )
            .unwrap();
        let (ok2, _) = seed_real_pdf(&mut db, tmp.path(), "ok2.pdf");

        let provider: Arc<dyn AiProvider> = Arc::new(MockProvider::new("alpha, beta"));
        let results = run_many(
            &mut db,
            provider,
            &[ok1, bad.id, ok2],
            &AutoTagOpts::default(),
        )
        .await;

        assert_eq!(results.len(), 3);
        assert!(results[0].error.is_none(), "ok1 should succeed");
        assert!(results[1].error.is_some(), "bad should fail");
        assert!(results[2].error.is_none(), "ok2 should succeed");
        // Both ok docs should have alpha + beta attached.
        for ok_id in [ok1, ok2] {
            let names: Vec<String> = db
                .tags_for_document(ok_id)
                .unwrap()
                .into_iter()
                .map(|t| t.name)
                .collect();
            assert!(names.contains(&"alpha".to_string()));
            assert!(names.contains(&"beta".to_string()));
        }
    }
}
