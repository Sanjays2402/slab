// OpenAiCompatibleProvider — talks to any OpenAI-compatible HTTP API.
//
// Works against:
//   - OpenAI itself (base_url = "https://api.openai.com/v1")
//   - GitHub Copilot's OAuth-gated proxy
//   - Anthropic via openrouter or a litellm proxy
//   - llama.cpp's built-in `server` (base_url = "http://localhost:8080/v1")
//   - vLLM, LM Studio, oobabooga, etc.
//
// Endpoint contracts (OpenAI Chat Completions API):
//   POST {base_url}/chat/completions
//     body: { "model": "...", "messages": [{"role":"...", "content":"..."}],
//             "stream": false, "temperature": ?, "max_tokens": ? }
//     response: { "model":"...", "choices":[{"message":{"role":"assistant","content":"..."}, ...}] }
//   POST {base_url}/embeddings
//     body: { "model": "...", "input": ["..."] }       (input may be string or array)
//     response: { "data":[{"embedding":[f32,...]}, ...], "model":"..." }
//
// Auth is `Authorization: Bearer <api_key>`. We never log the key.

use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatResponse};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// A reasonable cheap default that exists on most providers.
const DEFAULT_CHAT_MODEL: &str = "gpt-4o-mini";
/// 1536-dim embedding model — small, fast, and widely available.
const DEFAULT_EMBED_MODEL: &str = "text-embedding-3-small";

pub struct OpenAiCompatibleProvider {
    /// Without trailing slash. Examples: `https://api.openai.com/v1`,
    /// `http://localhost:8080/v1`, `https://api.githubcopilot.com`.
    base_url: String,
    api_key: String,
    chat_model: String,
    embed_model: String,
    /// Extra HTTP headers attached to every chat/embed request.
    /// Populated by plugin-contributed providers (Foundry Slice 8) so
    /// plugin authors can carry e.g. `X-Org-Id` or a custom auth scheme
    /// without modifying provider code.
    extra_headers: HashMap<String, String>,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    /// Construct with an explicit base URL and API key. Trailing slashes
    /// on `base_url` are stripped so callers don't have to care.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let mut url: String = base_url.into();
        while url.ends_with('/') {
            url.pop();
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client builder");
        Self {
            base_url: url,
            api_key: api_key.into(),
            chat_model: DEFAULT_CHAT_MODEL.to_string(),
            embed_model: DEFAULT_EMBED_MODEL.to_string(),
            extra_headers: HashMap::new(),
            client,
        }
    }

    pub fn with_chat_model(mut self, model: impl Into<String>) -> Self {
        self.chat_model = model.into();
        self
    }

    pub fn with_embed_model(mut self, model: impl Into<String>) -> Self {
        self.embed_model = model.into();
        self
    }

    /// Attach extra HTTP headers (e.g. `X-Organization`, `X-Custom-Auth`)
    /// to every request. Replaces any previously-set extras. Used by
    /// plugin-contributed providers (Foundry Slice 8) that need a
    /// non-Bearer auth scheme or organisation routing.
    ///
    /// Header **values** may use the `$VAR_NAME` syntax to interpolate
    /// an environment variable at construction time (see
    /// [`resolve_header_value`]).
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.extra_headers = headers;
        self
    }
}

/// Resolve a header value, expanding a leading `$VAR_NAME` to the
/// corresponding env var. The whole value must be `$NAME` (no
/// concatenation, no braces) — keeps the contract simple and
/// predictable. Bare values (no `$` prefix) pass through verbatim.
///
/// Returns `Err` when the value starts with `$` but the env var is
/// unset or empty.
pub fn resolve_header_value(value: &str) -> Result<String, AiError> {
    if let Some(name) = value.strip_prefix('$') {
        let v = std::env::var(name).map_err(|_| {
            AiError::ProviderUnavailable(format!("header references missing env var ${name}"))
        })?;
        if v.trim().is_empty() {
            return Err(AiError::ProviderUnavailable(format!(
                "header env var ${name} is empty"
            )));
        }
        Ok(v)
    } else {
        Ok(value.to_string())
    }
}

/// Apply [`OpenAiCompatibleProvider::extra_headers`] to a reqwest
/// request builder, performing `$VAR` substitution per header value.
fn apply_extra_headers(
    mut req: reqwest::RequestBuilder,
    extras: &HashMap<String, String>,
) -> Result<reqwest::RequestBuilder, AiError> {
    for (k, v) in extras {
        let resolved = resolve_header_value(v)?;
        req = req.header(k, resolved);
    }
    Ok(req)
}

// ---------- Wire types ----------

#[derive(Serialize)]
struct OaiChatRequest<'a> {
    model: &'a str,
    messages: &'a [OaiMessage<'a>],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct OaiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OaiChatResponse {
    model: String,
    choices: Vec<OaiChoice>,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: OaiResponseMessage,
}

#[derive(Deserialize)]
struct OaiResponseMessage {
    content: String,
}

#[derive(Serialize)]
struct OaiEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct OaiEmbedResponse {
    data: Vec<OaiEmbedItem>,
}

#[derive(Deserialize)]
struct OaiEmbedItem {
    embedding: Vec<f32>,
}

fn role_str(r: super::ChatRole) -> &'static str {
    match r {
        super::ChatRole::System => "system",
        super::ChatRole::User => "user",
        super::ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl AiProvider for OpenAiCompatibleProvider {
    async fn chat(&self, msgs: &[ChatMessage], opts: &ChatOpts) -> Result<ChatResponse, AiError> {
        let model = opts.model.as_deref().unwrap_or(&self.chat_model);
        let wire_msgs: Vec<OaiMessage> = msgs
            .iter()
            .map(|m| OaiMessage {
                role: role_str(m.role),
                content: &m.content,
            })
            .collect();
        let body = OaiChatRequest {
            model,
            messages: &wire_msgs,
            stream: false,
            temperature: opts.temperature,
            max_tokens: opts.max_tokens,
        };
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let req = req.json(&body);
        let req = apply_extra_headers(req, &self.extra_headers)?;
        let res = req.send().await?;
        let status = res.status();
        if status.as_u16() == 429 {
            return Err(AiError::RateLimited);
        }
        if !status.is_success() {
            let txt = res.text().await.unwrap_or_default();
            return Err(AiError::InvalidResponse(format!(
                "openai /chat/completions returned HTTP {status}: {txt}"
            )));
        }
        let parsed: OaiChatResponse = res.json().await.map_err(|e| {
            AiError::InvalidResponse(format!("decoding /chat/completions body: {e}"))
        })?;
        let first = parsed.choices.into_iter().next().ok_or_else(|| {
            AiError::InvalidResponse("openai response had no choices".to_string())
        })?;
        Ok(ChatResponse {
            content: first.message.content,
            model: parsed.model,
        })
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = OaiEmbedRequest {
            model: &self.embed_model,
            input: texts,
        };
        let url = format!("{}/embeddings", self.base_url);
        let mut req = self.client.post(&url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let req = req.json(&body);
        let req = apply_extra_headers(req, &self.extra_headers)?;
        let res = req.send().await?;
        let status = res.status();
        if status.as_u16() == 429 {
            return Err(AiError::RateLimited);
        }
        if !status.is_success() {
            let txt = res.text().await.unwrap_or_default();
            return Err(AiError::InvalidResponse(format!(
                "openai /embeddings returned HTTP {status}: {txt}"
            )));
        }
        let parsed: OaiEmbedResponse = res
            .json()
            .await
            .map_err(|e| AiError::InvalidResponse(format!("decoding /embeddings body: {e}")))?;
        if parsed.data.len() != texts.len() {
            return Err(AiError::InvalidResponse(format!(
                "expected {} embeddings, got {}",
                texts.len(),
                parsed.data.len()
            )));
        }
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }

    fn name(&self) -> &'static str {
        "openai-compatible"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatRole;

    /// Happy-path chat: server returns the OpenAI shape, provider decodes
    /// the first choice's content.
    #[tokio::test]
    async fn chat_round_trip_buffered() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer sk-test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"Yes, 42."}}]}"#,
            )
            .create_async()
            .await;

        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test");
        let res = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "Is the answer 42?".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap();
        assert_eq!(res.model, "gpt-4o-mini");
        assert_eq!(res.content, "Yes, 42.");
        mock.assert_async().await;
    }

    /// 429 from server → RateLimited (UI shows "wait a moment / switch
    /// provider" instead of a stack-trace-looking error).
    #[tokio::test]
    async fn chat_429_maps_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(429)
            .with_body(r#"{"error":{"message":"slow down"}}"#)
            .create_async()
            .await;
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test");
        let err = provider.chat(&[], &ChatOpts::default()).await.unwrap_err();
        match err {
            AiError::RateLimited => {}
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    /// Empty choices array → InvalidResponse. Pathological response
    /// from a misconfigured proxy that hands back HTTP 200 with no body.
    #[tokio::test]
    async fn chat_empty_choices_is_invalid_response() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"model":"gpt-4o-mini","choices":[]}"#)
            .create_async()
            .await;
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test");
        let err = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(m.contains("no choices"), "got {m}"),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    /// Embeddings: two inputs → two vectors in the same order.
    #[tokio::test]
    async fn embed_returns_one_vector_per_input() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"text-embedding-3-small","data":[
                    {"index":0,"embedding":[0.1,0.2]},
                    {"index":1,"embedding":[0.3,0.4]}
                ]}"#,
            )
            .create_async()
            .await;
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test");
        let vecs = provider
            .embed(&["alpha".to_string(), "beta".to_string()])
            .await
            .unwrap();
        assert_eq!(vecs, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
        mock.assert_async().await;
    }

    /// Embeddings: server returns wrong count → InvalidResponse. Catches
    /// misconfigured proxies that silently truncate.
    #[tokio::test]
    async fn embed_rejects_count_mismatch() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"embedding":[0.1]}]}"#)
            .create_async()
            .await;
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test");
        let err = provider
            .embed(&["one".to_string(), "two".to_string()])
            .await
            .unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(m.contains("expected 2"), "got {m}"),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    /// Empty input slice → empty output, no HTTP call. Lets call sites
    /// be lazy about pre-filtering.
    #[tokio::test]
    async fn embed_empty_input_short_circuits() {
        let provider =
            OpenAiCompatibleProvider::new("http://127.0.0.1:1/v1".to_string(), "sk-test");
        let vecs = provider.embed(&[]).await.unwrap();
        assert!(vecs.is_empty());
    }

    /// Base URL trailing slashes are normalised so config files like
    /// `base_url = "https://api.openai.com/v1/"` still work.
    #[test]
    fn trailing_slashes_stripped() {
        let p =
            OpenAiCompatibleProvider::new("https://api.openai.com/v1////".to_string(), "sk-test");
        assert_eq!(p.base_url, "https://api.openai.com/v1");
    }

    /// Default-impl of `chat_with_images` on `AiProvider` returns the
    /// "vision unsupported" error so OpenAI-compat (which doesn't yet
    /// override it for v0.13.0) fails loudly + cleanly. The UI maps
    /// this to a "switch to Ollama for vision" hint.
    #[tokio::test]
    async fn openai_provider_rejects_vision_by_default() {
        let p = OpenAiCompatibleProvider::new("http://127.0.0.1:1/v1".to_string(), "sk-test");
        let err = p
            .chat_with_images(&[], &[], &ChatOpts::default())
            .await
            .unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(
                m.contains("vision unsupported") && m.contains("openai-compatible"),
                "got {m}"
            ),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    // ---------- Slice 8: header injection (plugin-contributed providers) ----------

    /// Bare values pass through unchanged. No `$` prefix → no env-var lookup.
    #[test]
    fn resolve_header_value_bare_passes_through() {
        let got = resolve_header_value("acme-prod").unwrap();
        assert_eq!(got, "acme-prod");
    }

    /// `$VAR_NAME` reads from env and returns the value.
    #[test]
    fn resolve_header_value_expands_env_var() {
        let name = "SLAB_TEST_HDR_VAR_PRESENT_8A";
        // SAFETY: unique name avoids collision with parallel tests.
        std::env::set_var(name, "secret-token-42");
        let got = resolve_header_value(&format!("${name}")).unwrap();
        assert_eq!(got, "secret-token-42");
        std::env::remove_var(name);
    }

    /// Missing env var → `ProviderUnavailable` with a helpful message.
    #[test]
    fn resolve_header_value_missing_env_var_errors() {
        let name = "SLAB_TEST_HDR_VAR_MISSING_8B";
        std::env::remove_var(name);
        let err = resolve_header_value(&format!("${name}")).unwrap_err();
        match err {
            AiError::ProviderUnavailable(m) => {
                assert!(
                    m.contains(name),
                    "expected error to mention var name, got: {m}"
                );
                assert!(m.contains("missing"), "expected 'missing' in {m}");
            }
            other => panic!("expected ProviderUnavailable, got {other:?}"),
        }
    }

    /// Empty env var → `ProviderUnavailable`. Treats `VAR=""` as a
    /// misconfiguration rather than silently sending an empty header.
    #[test]
    fn resolve_header_value_empty_env_var_errors() {
        let name = "SLAB_TEST_HDR_VAR_EMPTY_8C";
        std::env::set_var(name, "   ");
        let err = resolve_header_value(&format!("${name}")).unwrap_err();
        std::env::remove_var(name);
        match err {
            AiError::ProviderUnavailable(m) => assert!(m.contains("empty"), "got {m}"),
            other => panic!("expected ProviderUnavailable, got {other:?}"),
        }
    }

    /// End-to-end: a provider built `.with_headers(...)` actually sends
    /// those headers on chat requests. Uses mockito `.match_header()` to
    /// fail the mock unless the header is present.
    #[tokio::test]
    async fn chat_sends_extra_headers() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("x-org-id", "acme")
            .match_header("x-custom-tier", "gold")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"ok"}}]}"#,
            )
            .create_async()
            .await;
        let mut headers = HashMap::new();
        headers.insert("X-Org-Id".to_string(), "acme".to_string());
        headers.insert("X-Custom-Tier".to_string(), "gold".to_string());
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test")
            .with_headers(headers);
        let res = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap();
        assert_eq!(res.content, "ok");
        mock.assert_async().await;
    }

    /// End-to-end: same as above, on the embeddings endpoint. Catches
    /// regressions where only `chat()` is wired up.
    #[tokio::test]
    async fn embed_sends_extra_headers() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .match_header("x-org-id", "acme")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"embedding":[0.1,0.2]}]}"#)
            .create_async()
            .await;
        let mut headers = HashMap::new();
        headers.insert("X-Org-Id".to_string(), "acme".to_string());
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test")
            .with_headers(headers);
        let vecs = provider.embed(&["hello".to_string()]).await.unwrap();
        assert_eq!(vecs, vec![vec![0.1f32, 0.2f32]]);
        mock.assert_async().await;
    }

    /// `$VAR` expansion is applied at request time, so a header value of
    /// `$SLAB_TEST_HDR_LIVE` reads the env var and sends the resolved value.
    #[tokio::test]
    async fn chat_expands_env_var_header_at_request_time() {
        let name = "SLAB_TEST_HDR_LIVE_8D";
        std::env::set_var(name, "resolved-bearer-xyz");
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("x-extra-auth", "resolved-bearer-xyz")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"ok"}}]}"#,
            )
            .create_async()
            .await;
        let mut headers = HashMap::new();
        headers.insert("X-Extra-Auth".to_string(), format!("${name}"));
        let provider = OpenAiCompatibleProvider::new(format!("{}/v1", server.url()), "sk-test")
            .with_headers(headers);
        let res = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &ChatOpts::default(),
            )
            .await;
        std::env::remove_var(name);
        let res = res.unwrap();
        assert_eq!(res.content, "ok");
        mock.assert_async().await;
    }

    /// If `$VAR` is unset at request time, the call fails with
    /// `ProviderUnavailable` before the HTTP request is made.
    #[tokio::test]
    async fn chat_with_missing_env_var_header_fails_before_request() {
        let name = "SLAB_TEST_HDR_MISSING_LIVE_8E";
        std::env::remove_var(name);
        // No mock — if we made an HTTP request the test would hang/error.
        let mut headers = HashMap::new();
        headers.insert("X-Auth".to_string(), format!("${name}"));
        let provider =
            OpenAiCompatibleProvider::new("http://127.0.0.1:1/v1".to_string(), "sk-test")
                .with_headers(headers);
        let err = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap_err();
        match err {
            AiError::ProviderUnavailable(m) => assert!(m.contains(name), "got {m}"),
            other => panic!("expected ProviderUnavailable, got {other:?}"),
        }
    }
}
