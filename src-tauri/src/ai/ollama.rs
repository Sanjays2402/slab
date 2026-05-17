// OllamaProvider — talks to a local Ollama daemon over HTTP.
//
// Endpoint contracts (Ollama API v0.1.x):
// - POST /api/chat
//     body: { "model": "...", "messages": [...], "stream": false, "options": {...} }
//     response: { "message": { "content": "..." }, "model": "...", "done": true }
// - POST /api/embeddings
//     body: { "model": "...", "prompt": "..." }
//     response: { "embedding": [f32, ...] }
//
// Embeddings are a per-prompt call in Ollama's API (no batch endpoint),
// so we issue them sequentially. Acceptable for v0.10.0; we can fan out
// later if Beacon indexing becomes a bottleneck.

use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatResponse};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default chat model. Small enough to run on a Mac mini, large enough
/// to be useful for the Beacon Q&A panel. Users override via config.
const DEFAULT_CHAT_MODEL: &str = "llama3.2:3b";
/// Default embedding model. 768-dim, snappy.
const DEFAULT_EMBED_MODEL: &str = "nomic-embed-text";

pub struct OllamaProvider {
    base_url: String,
    chat_model: String,
    embed_model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Construct with the default localhost endpoint and default models.
    pub fn new() -> Self {
        Self::with_base_url("http://localhost:11434")
    }

    /// Construct with a custom base URL (used by the unit tests against
    /// `mockito::Server`).
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            // Chat can be slow on a Mac mini under load; embed is fast.
            // 120s is generous for chat without being absurd.
            .timeout(Duration::from_secs(120))
            // Treat a refused TCP connection as ProviderUnavailable
            // promptly rather than waiting for the full timeout.
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client builder");
        Self {
            base_url: base_url.into(),
            chat_model: DEFAULT_CHAT_MODEL.to_string(),
            embed_model: DEFAULT_EMBED_MODEL.to_string(),
            client,
        }
    }

    /// Override the chat model (e.g. `"llama3.1:8b"`).
    pub fn with_chat_model(mut self, model: impl Into<String>) -> Self {
        self.chat_model = model.into();
        self
    }

    /// Override the embed model.
    pub fn with_embed_model(mut self, model: impl Into<String>) -> Self {
        self.embed_model = model.into();
        self
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- Wire types ----------
// We deliberately keep these private so the public ChatMessage /
// ChatResponse stay provider-agnostic.

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: &'a [OllamaMessage<'a>],
    stream: bool,
    options: OllamaChatOptions,
}

#[derive(Serialize, Default)]
struct OllamaChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(rename = "num_predict", skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
    model: String,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

#[derive(Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
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
impl AiProvider for OllamaProvider {
    async fn chat(&self, msgs: &[ChatMessage], opts: &ChatOpts) -> Result<ChatResponse, AiError> {
        let model = opts.model.as_deref().unwrap_or(&self.chat_model);
        let wire_msgs: Vec<OllamaMessage> = msgs
            .iter()
            .map(|m| OllamaMessage {
                role: role_str(m.role),
                content: &m.content,
            })
            .collect();
        let body = OllamaChatRequest {
            model,
            messages: &wire_msgs,
            stream: false,
            options: OllamaChatOptions {
                temperature: opts.temperature,
                max_tokens: opts.max_tokens,
            },
        };
        let url = format!("{}/api/chat", self.base_url);
        let res = self.client.post(&url).json(&body).send().await?;
        let status = res.status();
        if !status.is_success() {
            let txt = res.text().await.unwrap_or_default();
            return Err(AiError::InvalidResponse(format!(
                "ollama /api/chat returned HTTP {status}: {txt}"
            )));
        }
        let parsed: OllamaChatResponse = res
            .json()
            .await
            .map_err(|e| AiError::InvalidResponse(format!("decoding /api/chat body: {e}")))?;
        Ok(ChatResponse {
            content: parsed.message.content,
            model: parsed.model,
        })
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        let url = format!("{}/api/embeddings", self.base_url);
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            let body = OllamaEmbedRequest {
                model: &self.embed_model,
                prompt: t,
            };
            let res = self.client.post(&url).json(&body).send().await?;
            let status = res.status();
            if !status.is_success() {
                let txt = res.text().await.unwrap_or_default();
                return Err(AiError::InvalidResponse(format!(
                    "ollama /api/embeddings returned HTTP {status}: {txt}"
                )));
            }
            let parsed: OllamaEmbedResponse = res.json().await.map_err(|e| {
                AiError::InvalidResponse(format!("decoding /api/embeddings body: {e}"))
            })?;
            if parsed.embedding.is_empty() {
                return Err(AiError::InvalidResponse(
                    "ollama returned empty embedding vector".into(),
                ));
            }
            out.push(parsed.embedding);
        }
        Ok(out)
    }

    fn name(&self) -> &'static str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatRole;

    /// Happy-path chat: server returns a 200 with the expected body shape,
    /// provider decodes it into a `ChatResponse`.
    #[tokio::test]
    async fn chat_round_trip_buffered() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"llama3.2:3b","message":{"role":"assistant","content":"42 is the answer."},"done":true}"#,
            )
            .create_async()
            .await;

        let provider = OllamaProvider::with_base_url(server.url());
        let res = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "What is the answer?".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap();

        assert_eq!(res.model, "llama3.2:3b");
        assert_eq!(res.content, "42 is the answer.");
        mock.assert_async().await;
    }

    /// HTTP 500 from server → InvalidResponse with the body text included
    /// so the UI can surface the model's actual error message.
    #[tokio::test]
    async fn chat_surfaces_http_error_body() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/api/chat")
            .with_status(500)
            .with_body("model 'gpt-9000' not found, try `ollama pull`")
            .create_async()
            .await;

        let provider = OllamaProvider::with_base_url(server.url());
        let err = provider.chat(&[], &ChatOpts::default()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("500"), "expected status in error, got {msg}");
        assert!(
            msg.contains("model 'gpt-9000' not found"),
            "expected server body in error, got {msg}"
        );
    }

    /// Connection-refused → ProviderUnavailable (the UI hint case:
    /// "Install Ollama at ollama.com or switch provider").
    #[tokio::test]
    async fn chat_unreachable_endpoint_maps_to_provider_unavailable() {
        // Use a port that's almost certainly closed.
        let provider = OllamaProvider::with_base_url("http://127.0.0.1:1");
        let err = provider
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "ping".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap_err();
        match err {
            AiError::ProviderUnavailable(_) => {}
            other => panic!("expected ProviderUnavailable, got {other:?}"),
        }
    }

    /// Embeddings: a single batch of two prompts → two vectors.
    #[tokio::test]
    async fn embed_returns_vectors_per_input() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embeddings")
            .expect(2) // one POST per text
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"embedding":[0.1,0.2,0.3]}"#)
            .create_async()
            .await;

        let provider = OllamaProvider::with_base_url(server.url());
        let vecs = provider
            .embed(&["hello".to_string(), "world".to_string()])
            .await
            .unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(vecs[1], vec![0.1, 0.2, 0.3]);
        mock.assert_async().await;
    }

    /// Empty embedding vector in the response → InvalidResponse. This
    /// catches a misconfigured embed model (e.g. someone pointing at a
    /// chat-only model that returns `embedding: []`).
    #[tokio::test]
    async fn embed_rejects_empty_vector() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/api/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"embedding":[]}"#)
            .create_async()
            .await;

        let provider = OllamaProvider::with_base_url(server.url());
        let err = provider.embed(&["only".to_string()]).await.unwrap_err();
        match err {
            AiError::InvalidResponse(m) => {
                assert!(m.contains("empty embedding"), "got {m}");
            }
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }
}
