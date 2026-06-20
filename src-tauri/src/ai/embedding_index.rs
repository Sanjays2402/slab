// Beacon Embedding Index — local sqlite-backed vector store for
// semantic search across opened PDFs.
//
// Design tradeoffs:
//
// * **Storage**: `rusqlite` with the `bundled` feature so we don't
//   depend on the host's libsqlite3. Single file at
//   `~/.slab/beacon-index.sqlite`.
// * **No `sqlite-vec`**: we considered it for ANN search but the
//   loading-as-extension path is a cross-platform headache and the
//   payoff isn't there for our scale — Slab is a single-user
//   desktop app, not a multi-tenant service. A modern Mac can
//   brute-force cosine over 100K rows at 768d in well under 100ms.
//   If a user pushes past that, we add an index in a follow-up.
// * **Vectors stored as raw BLOB of f32 little-endian**: half the size
//   of JSON, no parse cost. SQLite blob streaming keeps memory flat
//   during the scan.
// * **Per-PDF index keyed by SHA-256 of file contents**: re-indexing
//   the same file is a no-op; opening a slightly-edited copy creates
//   a fresh row. The frontend never sees the hash — it queries by
//   `pdf_path`.
// * **Embedding stays at the dim the model produced**: we don't pin
//   a dim, but `query()` will refuse mixed-dim results (would be
//   garbage anyway).
//
// Schema:
//
//   pdfs(hash TEXT PRIMARY KEY, path TEXT NOT NULL, pages INTEGER NOT NULL,
//        embed_model TEXT NOT NULL, indexed_at INTEGER NOT NULL)
//
//   chunks(id INTEGER PRIMARY KEY,
//          pdf_hash TEXT NOT NULL REFERENCES pdfs(hash) ON DELETE CASCADE,
//          page INTEGER NOT NULL,
//          idx_in_page INTEGER NOT NULL,
//          text TEXT NOT NULL,
//          embedding BLOB NOT NULL)
//
// This module is async on the query path (because it calls the
// `AiProvider` for the query embedding) and sync on the read/write
// SQL path (rusqlite is blocking; we run it inside `spawn_blocking`
// from the Tauri command surface). Tests use `:memory:` so they're
// fast and don't touch the filesystem.

use super::chunker::{chunk_pages, Chunk};
use super::{AiError, AiProvider};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Top-level error type. Wraps both DB errors and AI provider errors
/// so the Tauri command can ?-propagate to a single `CmdResult`.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("sqlite: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("ai: {0}")]
    Ai(#[from] AiError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// A search hit returned to the front-end. Page is 1-indexed so the
/// reader can jump directly to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub pdf_path: String,
    pub page: u32,
    pub idx_in_page: u32,
    pub text: String,
    /// Cosine similarity in [-1, 1]. Higher = more similar.
    pub score: f32,
}

/// Index stats — used by the UI to show "X chunks across Y PDFs".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexStats {
    pub pdfs: u32,
    pub chunks: u32,
}

/// Per-PDF record returned by [`EmbeddingIndex::list_indexed`]. Powers the
/// Beacon Cache Inspector's full table — every PDF currently in the
/// embedding index, with the per-row chunk count joined in so the panel
/// never makes one round-trip per row. v3.54.0 Atlas Beacon-Cache —
/// Slice 28.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexedPdfRecord {
    /// SHA-256 of the PDF file contents (hex). Stable cache key — also
    /// the `forget`/`forget_many` argument.
    pub pdf_hash: String,
    /// On-disk path captured at index time. May no longer exist — see
    /// [`EmbeddingIndex::find_stale`].
    pub pdf_path: String,
    pub pages: u32,
    /// Name of the embed model used at index time. The inspector
    /// surfaces a "mixed model" warning when more than one bucket is
    /// non-empty.
    pub embed_model: String,
    /// Unix-seconds timestamp the row was first written.
    pub indexed_at: i64,
    /// Number of chunk rows pinned to this PDF — joined in once so the
    /// table never has to do a per-row COUNT.
    pub chunks: u32,
}

/// Result of an indexing call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexReport {
    /// SHA-256 of the PDF (hex). Stable cache key.
    pub pdf_hash: String,
    /// Number of chunks written to the index.
    pub chunks_indexed: u32,
    /// Whether we wrote anything (false if the hash was already present).
    pub was_cached: bool,
}

/// Wraps a SQLite connection. Open once per app, reuse for many ops.
pub struct EmbeddingIndex {
    conn: Connection,
}

impl EmbeddingIndex {
    /// Open the on-disk index, creating it if absent.
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory index (tests).
    pub fn open_in_memory() -> Result<Self, IndexError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS pdfs (
                 hash TEXT PRIMARY KEY,
                 path TEXT NOT NULL,
                 pages INTEGER NOT NULL,
                 embed_model TEXT NOT NULL,
                 indexed_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS chunks (
                 id INTEGER PRIMARY KEY,
                 pdf_hash TEXT NOT NULL REFERENCES pdfs(hash) ON DELETE CASCADE,
                 page INTEGER NOT NULL,
                 idx_in_page INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 embedding BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS chunks_by_pdf ON chunks(pdf_hash);",
        )
    }

    /// Compute the content hash for a file at `path`. Stable across
    /// renames/moves; reflects content changes.
    pub fn hash_file(path: &Path) -> Result<String, IndexError> {
        let bytes = std::fs::read(path)?;
        Ok(hash_bytes(&bytes))
    }

    /// Return (pdfs, chunks) counts.
    pub fn stats(&self) -> Result<IndexStats, IndexError> {
        let pdfs: u32 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pdfs", [], |r| r.get(0))?;
        let chunks: u32 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        Ok(IndexStats { pdfs, chunks })
    }

    /// Is `pdf_hash` already in the index?
    pub fn has_hash(&self, pdf_hash: &str) -> Result<bool, IndexError> {
        let n: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM pdfs WHERE hash = ?1",
            params![pdf_hash],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Remove a PDF and its chunks from the index. Idempotent.
    pub fn forget(&self, pdf_hash: &str) -> Result<(), IndexError> {
        // FOREIGN KEY ON DELETE CASCADE handles chunks.
        self.conn
            .execute("DELETE FROM pdfs WHERE hash = ?1", params![pdf_hash])?;
        Ok(())
    }

    /// Bulk-delete every PDF named in `pdf_hashes` plus its chunks, in
    /// one transaction. Returns the count of rows actually removed —
    /// unknown hashes are silently skipped (tolerant wire contract so a
    /// stale hash from a list-vs-forget race can't crash the inspector).
    /// v3.54.0 Atlas Beacon-Cache — Slice 29.
    pub fn forget_many(&mut self, pdf_hashes: &[String]) -> Result<usize, IndexError> {
        if pdf_hashes.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut removed = 0usize;
        {
            let mut stmt = tx.prepare("DELETE FROM pdfs WHERE hash = ?1")?;
            for h in pdf_hashes {
                removed += stmt.execute(params![h])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Return every indexed PDF, newest first, with per-row chunk count
    /// joined in via a single LEFT JOIN + GROUP BY round-trip. LEFT JOIN
    /// keeps a PDF whose chunks got zeroed by a partial-write recovery
    /// visible in the inspector — an INNER JOIN would silently hide it.
    /// `ORDER BY indexed_at DESC, hash ASC` matches Slab's house style
    /// for activity feeds and gives a stable tie-break across same-second
    /// re-indexes. v3.54.0 Atlas Beacon-Cache — Slice 28.
    pub fn list_indexed(&self) -> Result<Vec<IndexedPdfRecord>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT pdfs.hash, pdfs.path, pdfs.pages, pdfs.embed_model, pdfs.indexed_at,
                    COALESCE(COUNT(chunks.id), 0) AS chunk_count
             FROM pdfs
             LEFT JOIN chunks ON chunks.pdf_hash = pdfs.hash
             GROUP BY pdfs.hash
             ORDER BY pdfs.indexed_at DESC, pdfs.hash ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(IndexedPdfRecord {
                    pdf_hash: r.get(0)?,
                    pdf_path: r.get(1)?,
                    pages: r.get(2)?,
                    embed_model: r.get(3)?,
                    indexed_at: r.get(4)?,
                    chunks: r.get::<_, i64>(5)? as u32,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Insert chunks + embeddings for a PDF. Replaces any prior rows
    /// for the same hash (idempotent re-index).
    pub fn write_pdf(
        &mut self,
        pdf_hash: &str,
        pdf_path: &Path,
        pages: u32,
        embed_model: &str,
        chunks: &[Chunk],
        embeddings: &[Vec<f32>],
    ) -> Result<u32, IndexError> {
        if chunks.len() != embeddings.len() {
            return Err(IndexError::Other(format!(
                "chunks/embeddings length mismatch: {} vs {}",
                chunks.len(),
                embeddings.len()
            )));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let tx = self.conn.transaction()?;
        // Drop any stale rows for this hash (re-index path).
        tx.execute("DELETE FROM pdfs WHERE hash = ?1", params![pdf_hash])?;
        tx.execute(
            "INSERT INTO pdfs (hash, path, pages, embed_model, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                pdf_hash,
                pdf_path.to_string_lossy().as_ref(),
                pages,
                embed_model,
                now
            ],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chunks (pdf_hash, page, idx_in_page, text, embedding)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (c, emb) in chunks.iter().zip(embeddings.iter()) {
                let blob = embedding_to_blob(emb);
                stmt.execute(params![pdf_hash, c.page, c.idx_in_page, c.text, blob])?;
            }
        }
        tx.commit()?;
        Ok(chunks.len() as u32)
    }

    /// Brute-force top-K cosine search against `query_embedding`.
    /// Optionally restricts to one `pdf_hash` so the user can search
    /// "this PDF" instead of "all PDFs".
    pub fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        only_pdf_hash: Option<&str>,
    ) -> Result<Vec<SearchHit>, IndexError> {
        if query_embedding.is_empty() {
            return Ok(Vec::new());
        }
        let q_norm = norm(query_embedding);
        if q_norm == 0.0 {
            return Ok(Vec::new());
        }

        let (sql, has_filter) = match only_pdf_hash {
            Some(_) => (
                "SELECT chunks.page, chunks.idx_in_page, chunks.text, chunks.embedding, pdfs.path
                 FROM chunks JOIN pdfs ON chunks.pdf_hash = pdfs.hash
                 WHERE chunks.pdf_hash = ?1",
                true,
            ),
            None => (
                "SELECT chunks.page, chunks.idx_in_page, chunks.text, chunks.embedding, pdfs.path
                 FROM chunks JOIN pdfs ON chunks.pdf_hash = pdfs.hash",
                false,
            ),
        };
        let mut stmt = self.conn.prepare(sql)?;
        let mut hits: Vec<SearchHit> = Vec::new();
        let rows = if has_filter {
            stmt.query(params![only_pdf_hash.unwrap()])?
        } else {
            stmt.query([])?
        };
        let mut rows = rows;
        while let Some(row) = rows.next()? {
            let page: u32 = row.get(0)?;
            let idx_in_page: u32 = row.get(1)?;
            let text: String = row.get(2)?;
            let blob: Vec<u8> = row.get(3)?;
            let pdf_path: String = row.get(4)?;
            let emb = blob_to_embedding(&blob);
            if emb.len() != query_embedding.len() {
                // Skip dim-mismatched rows — produced under a different
                // embed model. The UI shows an "Indexed with a different
                // model" warning at a higher layer.
                continue;
            }
            let score = cosine_with_qnorm(query_embedding, q_norm, &emb);
            hits.push(SearchHit {
                pdf_path,
                page,
                idx_in_page,
                text,
                score,
            });
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(top_k);
        Ok(hits)
    }
}

/// Convenience: index a PDF given its on-disk path, extracted pages,
/// and an AI provider. Embeds each chunk individually because Ollama's
/// `/api/embeddings` is single-input — the provider trait already
/// handles batching for callers that have an array.
///
/// If `pdf_hash` is already present, returns `was_cached: true` and
/// skips everything else. The frontend uses this for "on PDF open,
/// kick off background indexing".
pub async fn index_pdf(
    index: Arc<std::sync::Mutex<EmbeddingIndex>>,
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    pages: &[String],
    embed_model: &str,
    force_reindex: bool,
) -> Result<IndexReport, IndexError> {
    let pdf_hash = EmbeddingIndex::hash_file(pdf_path)?;
    if !force_reindex {
        let cached = index.lock().unwrap().has_hash(&pdf_hash)?;
        if cached {
            return Ok(IndexReport {
                pdf_hash,
                chunks_indexed: 0,
                was_cached: true,
            });
        }
    }
    let chunks = chunk_pages(pages);
    if chunks.is_empty() {
        return Ok(IndexReport {
            pdf_hash,
            chunks_indexed: 0,
            was_cached: false,
        });
    }
    let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
    let embeddings = provider.embed(&texts).await?;
    let n = index.lock().unwrap().write_pdf(
        &pdf_hash,
        pdf_path,
        pages.len() as u32,
        embed_model,
        &chunks,
        &embeddings,
    )?;
    Ok(IndexReport {
        pdf_hash,
        chunks_indexed: n,
        was_cached: false,
    })
}

/// Convenience: embed `query` via `provider` then search the index.
pub async fn search_index(
    index: Arc<std::sync::Mutex<EmbeddingIndex>>,
    provider: Arc<dyn AiProvider>,
    query: &str,
    top_k: usize,
    only_pdf_hash: Option<String>,
) -> Result<Vec<SearchHit>, IndexError> {
    let embedded = provider.embed(&[query.to_string()]).await?;
    let q = embedded
        .into_iter()
        .next()
        .ok_or_else(|| IndexError::Other("provider returned no embedding for query".into()))?;
    let hits = index
        .lock()
        .unwrap()
        .search(&q, top_k, only_pdf_hash.as_deref())?;
    Ok(hits)
}

/// Default on-disk index location: `~/.slab/beacon-index.sqlite`.
/// Picked alongside `~/.slab/config.toml` for tidiness.
pub fn default_index_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("beacon-index.sqlite")
}

// ---------- internal helpers ----------

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn embedding_to_blob(emb: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(emb.len() * 4);
    for f in emb {
        buf.extend_from_slice(&f.to_le_bytes());
    }
    buf
}

fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    if !blob.len().is_multiple_of(4) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().unwrap();
        out.push(f32::from_le_bytes(arr));
    }
    out
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine_with_qnorm(q: &[f32], q_norm: f32, x: &[f32]) -> f32 {
    let x_norm = norm(x);
    if x_norm == 0.0 || q_norm == 0.0 {
        return 0.0;
    }
    let dot: f32 = q.iter().zip(x.iter()).map(|(a, b)| a * b).sum();
    dot / (q_norm * x_norm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::chunker::Chunk;
    use crate::ai::{ChatMessage, ChatOpts, ChatResponse};
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// In-memory provider for tests. Returns canned embeddings keyed
    /// by input string, falling back to a deterministic hash-based vec.
    struct MockEmbedProvider {
        canned: Mutex<std::collections::HashMap<String, Vec<f32>>>,
    }

    impl MockEmbedProvider {
        fn new() -> Self {
            Self {
                canned: Mutex::new(std::collections::HashMap::new()),
            }
        }
        fn set(&self, key: &str, v: Vec<f32>) {
            self.canned.lock().unwrap().insert(key.to_string(), v);
        }
    }

    #[async_trait]
    impl AiProvider for MockEmbedProvider {
        async fn chat(
            &self,
            _msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            unimplemented!()
        }
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            let map = self.canned.lock().unwrap();
            let out: Vec<Vec<f32>> = texts
                .iter()
                .map(|t| {
                    map.get(t).cloned().unwrap_or_else(|| {
                        // Deterministic fallback so we never return None.
                        let s: u32 = t.bytes().map(|b| b as u32).sum();
                        vec![s as f32, (t.len() as f32) * 0.1, 1.0]
                    })
                })
                .collect();
            Ok(out)
        }
        fn name(&self) -> &'static str {
            "mock-embed"
        }
    }

    #[test]
    fn cosine_similarity_basic() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_with_qnorm(&a, norm(&a), &b) - 1.0).abs() < 1e-6);
        let c = vec![0.0f32, 1.0, 0.0];
        assert!(cosine_with_qnorm(&a, norm(&a), &c).abs() < 1e-6);
    }

    #[test]
    fn blob_roundtrip_preserves_values() {
        let v = vec![0.1f32, -0.2, 1e-7, 1234.5];
        let blob = embedding_to_blob(&v);
        assert_eq!(blob.len(), v.len() * 4);
        let back = blob_to_embedding(&blob);
        assert_eq!(back, v);
    }

    #[test]
    fn open_in_memory_creates_schema() {
        let idx = EmbeddingIndex::open_in_memory().unwrap();
        let s = idx.stats().unwrap();
        assert_eq!(s, IndexStats { pdfs: 0, chunks: 0 });
    }

    #[test]
    fn write_then_stats_reflects_counts() {
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let chunks = vec![
            Chunk {
                page: 1,
                idx_in_page: 0,
                text: "alpha".into(),
            },
            Chunk {
                page: 2,
                idx_in_page: 0,
                text: "beta".into(),
            },
        ];
        let embeddings = vec![vec![1.0f32, 0.0, 0.0], vec![0.0f32, 1.0, 0.0]];
        idx.write_pdf(
            "deadbeef",
            Path::new("/tmp/x.pdf"),
            2,
            "mock-embed",
            &chunks,
            &embeddings,
        )
        .unwrap();
        let s = idx.stats().unwrap();
        assert_eq!(s, IndexStats { pdfs: 1, chunks: 2 });
        assert!(idx.has_hash("deadbeef").unwrap());
        assert!(!idx.has_hash("nope").unwrap());
    }

    #[test]
    fn write_pdf_is_idempotent_replaces_rows() {
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let chunk = Chunk {
            page: 1,
            idx_in_page: 0,
            text: "v1".into(),
        };
        idx.write_pdf(
            "h1",
            Path::new("/p"),
            1,
            "m",
            std::slice::from_ref(&chunk),
            &[vec![1.0, 0.0]],
        )
        .unwrap();
        idx.write_pdf(
            "h1",
            Path::new("/p"),
            1,
            "m",
            std::slice::from_ref(&chunk),
            &[vec![0.0, 1.0]],
        )
        .unwrap();
        // Still one PDF, one chunk (not two).
        assert_eq!(idx.stats().unwrap(), IndexStats { pdfs: 1, chunks: 1 });
    }

    #[test]
    fn search_returns_top_k_in_descending_order() {
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let chunks = vec![
            Chunk {
                page: 1,
                idx_in_page: 0,
                text: "exactly".into(),
            },
            Chunk {
                page: 2,
                idx_in_page: 0,
                text: "orthogonal".into(),
            },
            Chunk {
                page: 3,
                idx_in_page: 0,
                text: "close".into(),
            },
        ];
        let embeddings = vec![
            vec![1.0f32, 0.0, 0.0],
            vec![0.0f32, 1.0, 0.0],
            vec![0.99f32, 0.1, 0.05],
        ];
        idx.write_pdf("h", Path::new("/q"), 3, "m", &chunks, &embeddings)
            .unwrap();
        let hits = idx.search(&[1.0, 0.0, 0.0], 2, None).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].text, "exactly");
        assert_eq!(hits[1].text, "close");
        // scores are non-increasing
        assert!(hits[0].score >= hits[1].score);
        // pdf_path was joined in
        assert_eq!(hits[0].pdf_path, "/q");
    }

    #[test]
    fn search_filters_by_pdf_hash() {
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let c = Chunk {
            page: 1,
            idx_in_page: 0,
            text: "same query vector".into(),
        };
        idx.write_pdf(
            "h1",
            Path::new("/a.pdf"),
            1,
            "m",
            std::slice::from_ref(&c),
            &[vec![1.0, 0.0]],
        )
        .unwrap();
        idx.write_pdf(
            "h2",
            Path::new("/b.pdf"),
            1,
            "m",
            std::slice::from_ref(&c),
            &[vec![1.0, 0.0]],
        )
        .unwrap();
        let hits = idx.search(&[1.0, 0.0], 10, Some("h2")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].pdf_path, "/b.pdf");
    }

    #[test]
    fn search_skips_dimension_mismatched_rows() {
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let c = Chunk {
            page: 1,
            idx_in_page: 0,
            text: "ok".into(),
        };
        idx.write_pdf(
            "h",
            Path::new("/p"),
            1,
            "m",
            &[c],
            &[vec![1.0, 0.0, 0.0, 0.0]],
        )
        .unwrap();
        // Query at dim 2 vs stored dim 4 → 0 hits, no panic.
        let hits = idx.search(&[1.0, 0.0], 5, None).unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[test]
    fn forget_removes_pdf_and_cascades_chunks() {
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let chunks = vec![Chunk {
            page: 1,
            idx_in_page: 0,
            text: "x".into(),
        }];
        idx.write_pdf("gone", Path::new("/g"), 1, "m", &chunks, &[vec![1.0]])
            .unwrap();
        idx.forget("gone").unwrap();
        assert_eq!(idx.stats().unwrap(), IndexStats { pdfs: 0, chunks: 0 });
    }

    #[tokio::test]
    async fn index_pdf_skips_when_cached() {
        // Build a temp file so we have a stable on-disk hash to compute.
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("a.pdf");
        std::fs::write(&pdf, b"fake-pdf-bytes-v1").unwrap();
        let idx = Arc::new(std::sync::Mutex::new(
            EmbeddingIndex::open_in_memory().unwrap(),
        ));
        let provider = Arc::new(MockEmbedProvider::new());
        let pages = vec!["first page body".to_string()];

        let r1 = index_pdf(idx.clone(), provider.clone(), &pdf, &pages, "m", false)
            .await
            .unwrap();
        assert!(!r1.was_cached);
        assert!(r1.chunks_indexed >= 1);

        let r2 = index_pdf(idx.clone(), provider.clone(), &pdf, &pages, "m", false)
            .await
            .unwrap();
        assert!(r2.was_cached, "second call should hit cache");
        assert_eq!(r2.chunks_indexed, 0);
        // Same hash both times
        assert_eq!(r1.pdf_hash, r2.pdf_hash);
    }

    #[tokio::test]
    async fn index_pdf_force_reindex_rewrites_rows() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("a.pdf");
        std::fs::write(&pdf, b"v1").unwrap();
        let idx = Arc::new(std::sync::Mutex::new(
            EmbeddingIndex::open_in_memory().unwrap(),
        ));
        let provider = Arc::new(MockEmbedProvider::new());
        let pages = vec!["page text body here".to_string()];

        let r1 = index_pdf(idx.clone(), provider.clone(), &pdf, &pages, "m", false)
            .await
            .unwrap();
        assert!(!r1.was_cached);

        let r2 = index_pdf(idx.clone(), provider.clone(), &pdf, &pages, "m", true)
            .await
            .unwrap();
        assert!(!r2.was_cached, "force re-index ignores cache");
    }

    #[tokio::test]
    async fn search_index_returns_hits_for_query() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("doc.pdf");
        std::fs::write(&pdf, b"some pdf bytes").unwrap();
        let idx = Arc::new(std::sync::Mutex::new(
            EmbeddingIndex::open_in_memory().unwrap(),
        ));
        let provider = Arc::new(MockEmbedProvider::new());
        // Pin known vectors so we control similarity.
        let pages = vec!["alpha content".to_string(), "beta content".to_string()];
        // chunk_pages will produce 2 small chunks; pin the embed for each.
        let alpha_chunks = chunk_pages(&pages);
        for c in &alpha_chunks {
            if c.text.contains("alpha") {
                provider.set(&c.text, vec![1.0, 0.0, 0.0]);
            } else if c.text.contains("beta") {
                provider.set(&c.text, vec![0.0, 1.0, 0.0]);
            }
        }
        // Query roughly matches "alpha" direction.
        provider.set("find alpha", vec![1.0, 0.05, 0.0]);

        index_pdf(idx.clone(), provider.clone(), &pdf, &pages, "m", false)
            .await
            .unwrap();
        let hits = search_index(idx.clone(), provider.clone(), "find alpha", 5, None)
            .await
            .unwrap();
        assert!(!hits.is_empty());
        // Best hit should be the alpha chunk.
        assert!(
            hits[0].text.contains("alpha"),
            "expected top hit to be alpha, got {:?}",
            hits[0]
        );
    }

    #[test]
    fn default_index_path_lives_under_home_dot_slab() {
        let p = default_index_path();
        let s = p.to_string_lossy();
        assert!(s.contains(".slab"));
        assert!(s.ends_with("beacon-index.sqlite"));
    }

    #[test]
    fn hash_file_changes_when_contents_change() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("h.bin");
        std::fs::write(&p, b"v1").unwrap();
        let h1 = EmbeddingIndex::hash_file(&p).unwrap();
        std::fs::write(&p, b"v2").unwrap();
        let h2 = EmbeddingIndex::hash_file(&p).unwrap();
        assert_ne!(h1, h2);
        // hex SHA-256 is 64 chars
        assert_eq!(h1.len(), 64);
    }

    // ---------- Beacon Cache Inspector (v3.54.0 round-7) ----------

    /// Helper: seed `n` PDFs with `chunks_per_pdf` chunks each, under a
    /// shared `embed_model`. Returns the inserted hashes in insertion order.
    /// File contents include `embed_model` so two model buckets seeded in
    /// the same test get distinct content hashes (the index keys by hash,
    /// so colliding content would collapse one model into the other).
    fn seed_pdfs(
        idx: &mut EmbeddingIndex,
        n: usize,
        chunks_per_pdf: usize,
        embed_model: &str,
        dir: &Path,
    ) -> Vec<String> {
        let mut hashes = Vec::with_capacity(n);
        for i in 0..n {
            let path = dir.join(format!("doc-{i}.pdf"));
            // Write distinct bytes so each gets a distinct content hash.
            std::fs::write(&path, format!("pdf-bytes-{embed_model}-{i}").as_bytes()).unwrap();
            let hash = EmbeddingIndex::hash_file(&path).unwrap();
            let chunks: Vec<Chunk> = (0..chunks_per_pdf)
                .map(|k| Chunk {
                    page: 1,
                    idx_in_page: k as u32,
                    text: format!("doc {i} chunk {k}"),
                })
                .collect();
            let embeddings: Vec<Vec<f32>> = (0..chunks_per_pdf)
                .map(|_| vec![1.0f32, 0.0, 0.0])
                .collect();
            idx.write_pdf(
                &hash,
                &path,
                1,
                embed_model,
                chunks.as_slice(),
                embeddings.as_slice(),
            )
            .unwrap();
            hashes.push(hash);
        }
        hashes
    }

    #[test]
    fn list_indexed_empty_is_empty() {
        let idx = EmbeddingIndex::open_in_memory().unwrap();
        assert!(idx.list_indexed().unwrap().is_empty());
    }

    #[test]
    fn list_indexed_returns_one_row_per_pdf_with_joined_chunk_count() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let hashes = seed_pdfs(&mut idx, 3, 2, "m", dir.path());
        let listed = idx.list_indexed().unwrap();
        assert_eq!(listed.len(), 3);
        // One row per PDF, every chunk count joined in as 2 (the seed).
        for row in &listed {
            assert_eq!(row.chunks, 2);
            assert_eq!(row.embed_model, "m");
            assert!(hashes.contains(&row.pdf_hash));
        }
        // Hashes cover the seeded set exactly.
        let listed_hashes: std::collections::HashSet<_> =
            listed.iter().map(|r| r.pdf_hash.clone()).collect();
        let seed_hashes: std::collections::HashSet<_> = hashes.into_iter().collect();
        assert_eq!(listed_hashes, seed_hashes);
    }

    #[test]
    fn list_indexed_orders_newest_first() {
        // write_pdf stamps `indexed_at` from SystemTime. Same-second writes
        // tie-break on hash ASC; cross-second writes order DESC by stamp.
        // We exercise the tie-break (same second) which is the realistic
        // hot-path the inspector hits.
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        seed_pdfs(&mut idx, 4, 1, "m", dir.path());
        let listed = idx.list_indexed().unwrap();
        // Within the same indexed_at second, ascending hash order is
        // deterministic and stable across re-runs.
        let mut sorted = listed.clone();
        sorted.sort_by(|a, b| {
            b.indexed_at
                .cmp(&a.indexed_at)
                .then(a.pdf_hash.cmp(&b.pdf_hash))
        });
        assert_eq!(listed, sorted, "list_indexed must be newest-first");
    }

    #[test]
    fn list_indexed_keeps_zero_chunk_pdfs_via_left_join() {
        // A direct INSERT with no chunks should still surface in the
        // inspector (LEFT JOIN), reporting chunks == 0. An INNER JOIN
        // would silently hide it — exactly what the inspector exists
        // to prevent.
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let path = dir.path().join("solo.pdf");
        std::fs::write(&path, b"x").unwrap();
        let hash = EmbeddingIndex::hash_file(&path).unwrap();
        idx.conn
            .execute(
                "INSERT INTO pdfs (hash, path, pages, embed_model, indexed_at)
                 VALUES (?1, ?2, 1, 'm', 0)",
                params![hash, path.to_string_lossy().as_ref()],
            )
            .unwrap();
        let listed = idx.list_indexed().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].chunks, 0);
    }

    #[test]
    fn list_indexed_roundtrips_through_serde_snake_case() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        seed_pdfs(&mut idx, 1, 1, "m", dir.path());
        let listed = idx.list_indexed().unwrap();
        let json = serde_json::to_string(&listed).unwrap();
        // Field names land snake_case so the TS mirror's interface
        // doesn't need renames.
        assert!(json.contains("\"pdf_hash\""));
        assert!(json.contains("\"pdf_path\""));
        assert!(json.contains("\"embed_model\""));
        assert!(json.contains("\"indexed_at\""));
        let back: Vec<IndexedPdfRecord> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, listed);
    }

    #[test]
    fn forget_many_removes_named_hashes_in_one_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let hashes = seed_pdfs(&mut idx, 4, 2, "m", dir.path());
        let to_drop: Vec<String> = hashes[..2].to_vec();
        let removed = idx.forget_many(&to_drop).unwrap();
        assert_eq!(removed, 2);
        let remaining = idx.list_indexed().unwrap();
        assert_eq!(remaining.len(), 2);
        for r in &remaining {
            assert!(!to_drop.contains(&r.pdf_hash));
        }
        // CASCADE handled chunk rows.
        assert_eq!(idx.stats().unwrap().pdfs, 2);
        assert_eq!(idx.stats().unwrap().chunks, 4);
    }

    #[test]
    fn forget_many_silently_skips_unknown_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        let hashes = seed_pdfs(&mut idx, 2, 1, "m", dir.path());
        let mix = vec![hashes[0].clone(), "deadbeef".repeat(8), hashes[1].clone()];
        let removed = idx.forget_many(&mix).unwrap();
        assert_eq!(removed, 2, "the bogus hash is silently skipped");
        assert_eq!(idx.list_indexed().unwrap().len(), 0);
    }

    #[test]
    fn forget_many_empty_is_zero_noop() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = EmbeddingIndex::open_in_memory().unwrap();
        seed_pdfs(&mut idx, 1, 1, "m", dir.path());
        assert_eq!(idx.forget_many(&[]).unwrap(), 0);
        assert_eq!(idx.list_indexed().unwrap().len(), 1);
    }
}
