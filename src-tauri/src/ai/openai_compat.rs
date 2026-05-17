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
        let res = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
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
        let res = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
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
}
