//! Marketplace index fetcher with offline cache.
//!
//! This is the network-facing side of Bench. It fetches the
//! maintainer-curated `index.json`, parses + sanity-checks the schema,
//! and caches the raw JSON to `~/.slab/marketplace-cache.json` for
//! offline use. If the network call fails (transient outage, captive
//! portal, airplane mode, GH rate-limit), callers fall back to the
//! cached copy so the Browse tab never goes completely blank.
//!
//! Signature verification of each [`IndexEntry`] is **not** done here
//! — that's the install pipeline's job (Slice 4), since the verifier
//! decides what to do with bad-sig entries (skip, warn, hard-fail).
//! This module only validates the *envelope*: `schema_version` is one
//! we understand, and `signing_key_id` matches the key we have baked
//! in. Anything else is left raw for the install layer.
//!
//! The HTTP path uses `reqwest` (the same client config Beacon uses
//! for Ollama, see `ai/ollama.rs`) — short connect timeout so airplane
//! mode falls through to cache fast, 30s read timeout so a slow CDN
//! still works.
//!
//! ## Module API
//!
//! - [`DEFAULT_INDEX_URL`] — production index URL on the maintainer
//!   `slab-plugins` repo.
//! - [`default_cache_path`] — `~/.slab/marketplace-cache.json`.
//! - [`fetch_index`] — pure HTTP GET + parse. Errors on network or
//!   schema problems.
//! - [`fetch_index_with_cache`] — production entry point. Network
//!   first, cached fallback, opportunistic cache refresh on success.
//!   Never panics; always returns a usable [`FetchOutcome`].

use crate::marketplace::index::{Index, CURRENT_SCHEMA_VERSION};
use crate::marketplace::verify::MAINTAINER_KEY_ID;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Production index URL — pinned to the curated `slab-plugins` repo.
/// Override via `slab_marketplace_index(url=…)` for staging tests.
pub const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/Sanjays2402/slab-plugins/main/index.json";

/// Filename for the offline cache under `~/.slab/`.
pub const CACHE_FILE_NAME: &str = "marketplace-cache.json";

/// Default cache path (`~/.slab/marketplace-cache.json`). Returns
/// `None` if `$HOME` is unset, in which case the caller should pick a
/// fallback location or operate cacheless.
pub fn default_cache_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".slab").join(CACHE_FILE_NAME))
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("network request failed: {0}")]
    Network(String),
    #[error("server returned HTTP {0}")]
    BadStatus(u16),
    #[error("response body is not valid UTF-8: {0}")]
    Utf8(String),
    #[error("response body is not valid index JSON: {0}")]
    Parse(String),
    #[error("index has unsupported schema_version {got} (this build understands up to {max})")]
    UnsupportedSchema { got: u32, max: u32 },
    #[error("index signing_key_id {got:?} does not match baked-in maintainer key {expected:?}")]
    UnknownKeyId { got: String, expected: String },
    #[error("cache file is missing or unreadable")]
    NoCache,
}

/// Outcome of `fetch_index_with_cache`. Callers should display the
/// index regardless of whether it's fresh or stale — they can use
/// [`FetchOutcome::is_fresh`] to decide whether to show a "showing
/// cached results" banner.
#[derive(Debug)]
pub enum FetchOutcome {
    /// Index was fetched from the network this call. Cache has been
    /// refreshed (or attempted; failures to write the cache are
    /// non-fatal and silently ignored — the in-memory index is still
    /// good).
    Fresh(Index),
    /// Network attempt failed, but a cached copy was usable. Carries
    /// both the index and the original network error so callers can
    /// show a debug-y "last refresh failed because X" hint.
    Stale {
        index: Index,
        network_error: FetchError,
    },
    /// Network failed AND there was no usable cache. The caller has
    /// nothing to show.
    Failed(FetchError),
}

impl FetchOutcome {
    pub fn is_fresh(&self) -> bool {
        matches!(self, FetchOutcome::Fresh(_))
    }

    /// Borrow the inner index if there is one (Fresh or Stale).
    pub fn index(&self) -> Option<&Index> {
        match self {
            FetchOutcome::Fresh(i) => Some(i),
            FetchOutcome::Stale { index, .. } => Some(index),
            FetchOutcome::Failed(_) => None,
        }
    }
}

/// Pure HTTP GET → parse + envelope validation. No caching. Returns
/// `Err` on any network or schema problem.
///
/// Splitting this out (rather than inlining into the cache helper)
/// makes the unit tests trivial — they spin up a `mockito::Server`,
/// hand back a 200 with a known body, and assert the parsed Index.
pub async fn fetch_index(client: &reqwest::Client, url: &str) -> Result<Index, FetchError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(FetchError::BadStatus(status.as_u16()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let body = std::str::from_utf8(&bytes).map_err(|e| FetchError::Utf8(e.to_string()))?;
    parse_index(body)
}

/// Production entry point. Tries the network with the supplied
/// [`reqwest::Client`]; on success, writes the raw JSON to
/// `cache_path` (best-effort) and returns [`FetchOutcome::Fresh`].
/// On any failure, attempts to load a usable [`Index`] from the cache
/// and returns [`FetchOutcome::Stale`] (or [`FetchOutcome::Failed`] if
/// the cache is also unusable).
///
/// `cache_path = None` disables the cache entirely (useful for tests
/// and for environments where `$HOME` is unset).
pub async fn fetch_index_with_cache(
    client: &reqwest::Client,
    url: &str,
    cache_path: Option<&Path>,
) -> FetchOutcome {
    match fetch_index(client, url).await {
        Ok(idx) => {
            if let Some(cp) = cache_path {
                let _ = persist_cache(cp, &idx);
            }
            FetchOutcome::Fresh(idx)
        }
        Err(network_error) => match cache_path.and_then(load_cache) {
            Some(idx) => FetchOutcome::Stale {
                index: idx,
                network_error,
            },
            None => FetchOutcome::Failed(network_error),
        },
    }
}

/// Build a reqwest client with marketplace-appropriate timeouts.
/// Mirrors the Ollama client config (`ai/ollama.rs`):
/// - 3s connect timeout — airplane mode falls through to cache fast.
/// - 30s total — slow CDN still works; users notice if it's slower.
pub fn default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(3))
        .user_agent(concat!(
            "slab/",
            env!("CARGO_PKG_VERSION"),
            " (marketplace)"
        ))
        .build()
        .expect("reqwest client builder")
}

/// Parse + validate index envelope. Public so unit tests in adjacent
/// modules can re-use it; also reachable via the public `fetch_index`.
pub fn parse_index(body: &str) -> Result<Index, FetchError> {
    let idx: Index = serde_json::from_str(body).map_err(|e| FetchError::Parse(e.to_string()))?;
    if idx.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(FetchError::UnsupportedSchema {
            got: idx.schema_version,
            max: CURRENT_SCHEMA_VERSION,
        });
    }
    if idx.signing_key_id != MAINTAINER_KEY_ID {
        return Err(FetchError::UnknownKeyId {
            got: idx.signing_key_id.clone(),
            expected: MAINTAINER_KEY_ID.to_string(),
        });
    }
    Ok(idx)
}

fn persist_cache(path: &Path, idx: &Index) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(idx)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, body)
}

fn load_cache(path: &Path) -> Option<Index> {
    let body = fs::read_to_string(path).ok()?;
    // Tolerate cache corruption silently — corrupt cache acts like
    // "no cache" and lets the caller surface the live error instead.
    parse_index(&body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::index::IndexEntry;
    use mockito::Server;
    use tempfile::TempDir;

    fn sample_index_json() -> String {
        let idx = Index {
            schema_version: 1,
            signing_key_id: MAINTAINER_KEY_ID.into(),
            plugins: vec![IndexEntry {
                id: "com.example.hello".into(),
                name: "Hello".into(),
                version: "0.1.0".into(),
                description: "Demo".into(),
                author: "Sanjay".into(),
                download_url: "https://example.com/hello.tar.gz".into(),
                sha256: "deadbeef".repeat(8),
                size_bytes: 1024,
                slab_compat: ">=1.4.0".into(),
                signature: "AAAA".into(),
            }],
        };
        serde_json::to_string_pretty(&idx).unwrap()
    }

    #[test]
    fn parse_index_accepts_known_envelope() {
        let idx = parse_index(&sample_index_json()).unwrap();
        assert_eq!(idx.schema_version, 1);
        assert_eq!(idx.plugins.len(), 1);
    }

    #[test]
    fn parse_index_rejects_future_schema() {
        let body = r#"{"schema_version":99,"signing_key_id":"slab-maintainer-2026","plugins":[]}"#;
        let err = parse_index(body).unwrap_err();
        assert!(matches!(err, FetchError::UnsupportedSchema { got: 99, .. }));
    }

    #[test]
    fn parse_index_rejects_unknown_key_id() {
        let body = r#"{"schema_version":1,"signing_key_id":"some-other-key","plugins":[]}"#;
        let err = parse_index(body).unwrap_err();
        assert!(matches!(err, FetchError::UnknownKeyId { .. }));
    }

    #[test]
    fn parse_index_rejects_malformed_json() {
        let err = parse_index("{not json").unwrap_err();
        assert!(matches!(err, FetchError::Parse(_)));
    }

    #[tokio::test]
    async fn fetch_index_200_ok() {
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/index.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(sample_index_json())
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/index.json", server.url());
        let idx = fetch_index(&client, &url).await.unwrap();
        assert_eq!(idx.plugins.len(), 1);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn fetch_index_propagates_http_500() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/index.json")
            .with_status(500)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/index.json", server.url());
        let err = fetch_index(&client, &url).await.unwrap_err();
        assert!(matches!(err, FetchError::BadStatus(500)));
    }

    #[tokio::test]
    async fn fetch_index_with_cache_fresh_writes_cache() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/index.json")
            .with_status(200)
            .with_body(sample_index_json())
            .create_async()
            .await;
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("marketplace-cache.json");

        let client = reqwest::Client::new();
        let url = format!("{}/index.json", server.url());
        let outcome = fetch_index_with_cache(&client, &url, Some(&cache_path)).await;

        assert!(outcome.is_fresh());
        assert!(cache_path.exists(), "cache file must be written on success");
        // And the cache should be re-parseable.
        let cached = load_cache(&cache_path).unwrap();
        assert_eq!(cached.plugins.len(), 1);
    }

    #[tokio::test]
    async fn fetch_index_with_cache_falls_back_to_cache_on_network_failure() {
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("marketplace-cache.json");
        // Prime the cache.
        fs::write(&cache_path, sample_index_json()).unwrap();

        let client = reqwest::Client::new();
        // Bogus URL → connection refused.
        let outcome =
            fetch_index_with_cache(&client, "http://127.0.0.1:1/no-such", Some(&cache_path)).await;

        match outcome {
            FetchOutcome::Stale { index, .. } => {
                assert_eq!(index.plugins.len(), 1);
            }
            other => panic!("expected Stale, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_index_with_cache_failed_when_no_cache_and_no_network() {
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("does-not-exist.json");

        let client = reqwest::Client::new();
        let outcome =
            fetch_index_with_cache(&client, "http://127.0.0.1:1/no-such", Some(&cache_path)).await;
        assert!(matches!(outcome, FetchOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn fetch_index_with_cache_corrupt_cache_acts_like_no_cache() {
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("marketplace-cache.json");
        fs::write(&cache_path, "{not json at all").unwrap();

        let client = reqwest::Client::new();
        let outcome =
            fetch_index_with_cache(&client, "http://127.0.0.1:1/no-such", Some(&cache_path)).await;
        // No usable cache → Failed.
        assert!(matches!(outcome, FetchOutcome::Failed(_)));
    }

    #[test]
    fn default_cache_path_uses_home() {
        std::env::set_var("HOME", "/tmp/cake-test-home");
        let p = default_cache_path().unwrap();
        assert_eq!(
            p,
            PathBuf::from("/tmp/cake-test-home/.slab/marketplace-cache.json")
        );
    }

    #[test]
    fn fetch_outcome_index_returns_inner() {
        let idx = parse_index(&sample_index_json()).unwrap();
        let fresh = FetchOutcome::Fresh(idx.clone());
        assert!(fresh.is_fresh());
        assert!(fresh.index().is_some());

        let stale = FetchOutcome::Stale {
            index: idx,
            network_error: FetchError::NoCache,
        };
        assert!(!stale.is_fresh());
        assert!(stale.index().is_some());

        let failed = FetchOutcome::Failed(FetchError::NoCache);
        assert!(!failed.is_fresh());
        assert!(failed.index().is_none());
    }

    #[test]
    fn default_client_builds() {
        // Smoke test — ensure builder config compiles and produces a
        // valid client at runtime.
        let _ = default_client();
    }
}
