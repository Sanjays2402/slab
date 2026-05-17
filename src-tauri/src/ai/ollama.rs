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
/// Default vision model. llava:7b is the Ollama-recommended vision-capable
/// model at the time of v0.13.0: ~4.5 GB, runs comfortably on a Mac mini.
/// Users override via `with_vision_model` or `ChatOpts::model`.
const DEFAULT_VISION_MODEL: &str = "llava:7b";

pub struct OllamaProvider {
    base_url: String,
    chat_model: String,
    embed_model: String,
    /// Vision model used by `chat_with_images` when `ChatOpts::model`
    /// is `None`. Defaults to `llava:7b` (Slice 5).
    vision_model: String,
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
            vision_model: DEFAULT_VISION_MODEL.to_string(),
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

    /// Override the vision model (e.g. `"llava:13b"`, `"bakllava"`).
    pub fn with_vision_model(mut self, model: impl Into<String>) -> Self {
        self.vision_model = model.into();
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
    /// Base64-encoded PNGs attached to this message. `None` for text-only
    /// turns; skip_serializing_if keeps the wire shape backward-compatible
    /// with the existing text chat endpoint (no empty `"images": []`).
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<&'a [String]>,
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
                images: None,
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

    /// Vision Q&A — `images_b64` rides on the last user message. Default
    /// model is `llava:7b` (override via `ChatOpts::model` or
    /// `with_vision_model`). Wire shape matches Ollama's documented
    /// `/api/chat` multimodal contract.
    async fn chat_with_images(
        &self,
        msgs: &[ChatMessage],
        images_b64: &[String],
        opts: &ChatOpts,
    ) -> Result<ChatResponse, AiError> {
        if msgs.is_empty() {
            return Err(AiError::InvalidResponse(
                "chat_with_images called with empty msgs".into(),
            ));
        }
        let model = opts.model.as_deref().unwrap_or(&self.vision_model);
        let last_idx = msgs.len() - 1;
        let wire_msgs: Vec<OllamaMessage> = msgs
            .iter()
            .enumerate()
            .map(|(i, m)| OllamaMessage {
                role: role_str(m.role),
                content: &m.content,
                images: if i == last_idx && !images_b64.is_empty() {
                    Some(images_b64)
                } else {
                    None
                },
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
                "ollama /api/chat (vision) returned HTTP {status}: {txt}"
            )));
        }
        let parsed: OllamaChatResponse = res.json().await.map_err(|e| {
            AiError::InvalidResponse(format!("decoding /api/chat (vision) body: {e}"))
        })?;
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

    // -------- Vision (Slice 5) ----------------------------------------

    /// Happy path: chat_with_images attaches `images: [b64]` to the last
    /// user message, hits /api/chat, parses the reply, returns the
    /// declared model.
    #[tokio::test]
    async fn ollama_chat_with_images_sends_b64_on_last_message() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/api/chat")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "model": "llava:7b",
                "stream": false,
                "messages": [
                    { "role": "user", "content": "what is this?",
                      "images": ["AAA="] }
                ]
            })))
            .with_status(200)
            .with_body(r#"{"message":{"content":"a chart"},"model":"llava:7b"}"#)
            .create_async()
            .await;

        let p = OllamaProvider::with_base_url(srv.url());
        let resp = p
            .chat_with_images(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "what is this?".into(),
                }],
                &["AAA=".to_string()],
                &ChatOpts::default(),
            )
            .await
            .unwrap();
        assert_eq!(resp.content, "a chart");
        assert_eq!(resp.model, "llava:7b");
        m.assert_async().await;
    }

    /// Multi-turn history: images attach to the *last* user message only;
    /// intermediate turns are text-only on the wire.
    #[tokio::test]
    async fn ollama_chat_with_images_only_attaches_to_last_turn() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/api/chat")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "messages": [
                    { "role": "user", "content": "first" },
                    { "role": "assistant", "content": "ok" },
                    { "role": "user", "content": "what about now?",
                      "images": ["IMG="] }
                ]
            })))
            .with_status(200)
            .with_body(r#"{"message":{"content":"ok"},"model":"llava:7b"}"#)
            .create_async()
            .await;

        let p = OllamaProvider::with_base_url(srv.url());
        let history = vec![
            ChatMessage {
                role: ChatRole::User,
                content: "first".into(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "ok".into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "what about now?".into(),
            },
        ];
        p.chat_with_images(&history, &["IMG=".to_string()], &ChatOpts::default())
            .await
            .unwrap();
        m.assert_async().await;
    }

    /// Empty msgs is a programmer error — we surface a clear message
    /// instead of letting the server reject it with a cryptic 400.
    #[tokio::test]
    async fn ollama_chat_with_images_empty_msgs_errors() {
        let p = OllamaProvider::with_base_url("http://127.0.0.1:1");
        let err = p
            .chat_with_images(&[], &["AAA=".to_string()], &ChatOpts::default())
            .await
            .unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(m.contains("empty msgs"), "got {m}"),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    /// `ChatOpts::model` overrides the default `llava:7b` so power users
    /// can ship `llava:13b` or `bakllava` without rebuilding.
    #[tokio::test]
    async fn ollama_chat_with_images_honors_opts_model() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/api/chat")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "model": "llava:13b"
            })))
            .with_status(200)
            .with_body(r#"{"message":{"content":"ok"},"model":"llava:13b"}"#)
            .create_async()
            .await;
        let p = OllamaProvider::with_base_url(srv.url());
        let mut opts = ChatOpts::default();
        opts.model = Some("llava:13b".into());
        p.chat_with_images(
            &[ChatMessage {
                role: ChatRole::User,
                content: "?".into(),
            }],
            &["AAA=".to_string()],
            &opts,
        )
        .await
        .unwrap();
        m.assert_async().await;
    }
}
