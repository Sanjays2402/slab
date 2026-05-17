// Slab Beacon — AI provider abstraction.
//
// The provider trait is the seam between Slab's PDF features and the
// underlying LLM service. Implementations:
// - `OllamaProvider` — talks to the local Ollama daemon at
//   `http://localhost:11434/api/chat` and `/api/embeddings`. Default
//   for local-first / offline usage. Free, private.
// - (v0.10.0 Slice 2) `OpenAiCompatibleProvider` — talks to any
//   OpenAI-compatible HTTP API: OpenAI itself, Copilot (post-OAuth),
//   Anthropic via proxy, llama.cpp's `server`, etc.
//
// The shape of the trait is deliberately narrow: chat + embed. We
// leave streaming chat for a later slice (Slice 3) — the v1 cut
// returns a fully-buffered `ChatResponse` so we can unit-test it
// against `mockito` without futures plumbing.

pub mod chat;
pub mod chunker;
pub mod config;
pub mod embedding_index;
pub mod ollama;
pub mod openai_compat;
pub mod pii;
pub mod selection_action;
pub mod summary;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single message in a multi-turn chat. Mirrors the OpenAI / Ollama
/// shape so we don't have to translate per-provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

/// Provider-agnostic chat tuning knobs. Each provider applies what it
/// can; unrecognised fields are silently ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatOpts {
    /// Model identifier (Ollama tag like `llama3.2:3b`, or OpenAI
    /// model name like `gpt-4o-mini`). `None` means "use the provider
    /// default", which each impl chooses.
    pub model: Option<String>,
    /// Sampling temperature, 0.0 = greedy. `None` means provider default.
    pub temperature: Option<f32>,
    /// Hard ceiling on response tokens. `None` means provider default.
    pub max_tokens: Option<u32>,
}

/// A single (non-streamed) chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
}

/// Provider failure modes. The UI layer maps these to user-grade text.
#[derive(Debug, Error)]
pub enum AiError {
    /// The provider's HTTP endpoint isn't reachable. For Ollama this
    /// typically means "the daemon isn't running" — the UI surfaces a
    /// "Install Ollama at ollama.com" hint.
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),

    /// 429 / quota errors from hosted providers.
    #[error("rate limited")]
    RateLimited,

    /// The response wasn't shaped the way we expected (missing fields,
    /// non-JSON, etc.). Often signals a model misconfiguration.
    #[error("invalid response: {0}")]
    InvalidResponse(String),

    /// Catch-all transport error (TLS, DNS, socket closed mid-stream…).
    #[error("network: {0}")]
    Network(String),
}

impl From<reqwest::Error> for AiError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() || e.is_timeout() {
            AiError::ProviderUnavailable(e.to_string())
        } else if e.status().map(|s| s.as_u16()) == Some(429) {
            AiError::RateLimited
        } else {
            AiError::Network(e.to_string())
        }
    }
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Run a chat completion. Buffered (non-streaming) for v0.10.0
    /// Slice 1 — streaming lands in Slice 3 once the Tauri command
    /// surface is wired.
    async fn chat(&self, msgs: &[ChatMessage], opts: &ChatOpts) -> Result<ChatResponse, AiError>;

    /// Embed `texts` into vectors. Vector dimension is model-specific;
    /// the caller should pin a model for index consistency.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError>;

    /// Stable identifier of this provider — for logging + UI display.
    /// Returns e.g. `"ollama"` or `"openai-compatible"`.
    fn name(&self) -> &'static str;
}
