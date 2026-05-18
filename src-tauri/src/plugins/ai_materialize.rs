//! Plugin → AI provider materialisation (Foundry Slice 8).
//!
//! Plugin authors contribute an [`AiProviderContribution`] via their
//! `plugin.toml` (kind = `openai_compat`, plus `base_url`,
//! `default_model`, optional `headers`). At runtime we lift those
//! manifest fields into a fully-constructed [`OpenAiCompatibleProvider`]
//! so the rest of Slab (chat, summary, search, redact, selection
//! actions…) can use the provider without knowing it came from a
//! plugin.
//!
//! Auth model:
//! - Bearer auth is only sent when the contribution declares an
//!   `api_key_env` field *and* that env var is set. The current
//!   v1.3.0 manifest schema has no `api_key_env` field on
//!   `AiProviderContribution`, so by default plugin-contributed
//!   providers send **no `Authorization` header from Bearer auth**.
//! - For plugins that need auth, use the `headers` map with the
//!   `$VAR_NAME` syntax — e.g. `Authorization = "Bearer $MY_TOKEN"`.
//!   That value is resolved against `std::env` at request time.
//!
//! `default_model` is wired to both chat and embeddings. v1.3.0 keeps
//! the manifest minimal; future versions can add `chat_model` /
//! `embed_model` overrides if plugin authors need them.

use crate::ai::openai_compat::OpenAiCompatibleProvider;
use crate::ai::AiError;
use crate::plugins::contributions::ActiveAiProvider;
use crate::plugins::manifest::AiProviderContribution;

/// Build an [`OpenAiCompatibleProvider`] from a plugin-contributed
/// [`ActiveAiProvider`]. Convenience wrapper around
/// [`materialize_contribution`].
pub fn materialize_active(active: &ActiveAiProvider) -> Result<OpenAiCompatibleProvider, AiError> {
    materialize_contribution(&active.provider)
}

/// Build an [`OpenAiCompatibleProvider`] from the raw
/// [`AiProviderContribution`] manifest entry. Errors are limited to
/// validation-level issues (only `openai_compat` is supported in v1).
/// Header `$VAR` resolution is deferred to request time so a missing
/// env var doesn't permanently break the provider — it just fails the
/// next call with a clear message.
pub fn materialize_contribution(
    c: &AiProviderContribution,
) -> Result<OpenAiCompatibleProvider, AiError> {
    if c.kind != "openai_compat" {
        return Err(AiError::ProviderUnavailable(format!(
            "plugin ai_provider {:?}: unsupported kind {:?} (only \"openai_compat\" in v1)",
            c.id, c.kind
        )));
    }
    let provider = OpenAiCompatibleProvider::new(c.base_url.clone(), String::new())
        .with_chat_model(c.default_model.clone())
        .with_embed_model(c.default_model.clone())
        .with_headers(c.headers.clone());
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiProvider, ChatMessage, ChatOpts, ChatRole};
    use std::collections::HashMap;

    fn contribution(headers: HashMap<String, String>) -> AiProviderContribution {
        AiProviderContribution {
            id: "local-llamacpp".into(),
            label: "Local llama.cpp".into(),
            kind: "openai_compat".into(),
            base_url: "http://127.0.0.1:8080/v1".into(),
            default_model: "qwen2.5-1.5b".into(),
            headers,
        }
    }

    /// Happy path: kind = openai_compat with an empty headers map →
    /// Ok(provider). Models default to `default_model`.
    #[test]
    fn materialize_returns_provider_for_openai_compat() {
        let c = contribution(HashMap::new());
        let p = materialize_contribution(&c).unwrap();
        assert_eq!(p.name(), "openai-compatible");
    }

    /// Unsupported kind → ProviderUnavailable with the kind name in
    /// the message so the user can fix `plugin.toml`.
    #[test]
    fn materialize_rejects_unknown_kind() {
        let mut c = contribution(HashMap::new());
        c.kind = "anthropic_native".into();
        let res = materialize_contribution(&c);
        let err = match res {
            Ok(_) => panic!("expected ProviderUnavailable, got Ok"),
            Err(e) => e,
        };
        match err {
            AiError::ProviderUnavailable(m) => {
                assert!(m.contains("anthropic_native"), "got {m}");
                assert!(m.contains("openai_compat"), "got {m}");
            }
            other => panic!("expected ProviderUnavailable, got {other:?}"),
        }
    }

    /// Custom headers from the manifest reach actual HTTP requests.
    /// End-to-end via mockito.
    #[tokio::test]
    async fn materialized_provider_sends_custom_headers() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("x-org-id", "acme")
            // Plugin-built provider has no api key → no Bearer auth.
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"qwen2.5","choices":[{"index":0,"message":{"role":"assistant","content":"hello"}}]}"#,
            )
            .create_async()
            .await;
        let mut headers = HashMap::new();
        headers.insert("X-Org-Id".to_string(), "acme".to_string());
        let mut c = contribution(headers);
        c.base_url = format!("{}/v1", server.url());
        let p = materialize_contribution(&c).unwrap();
        let res = p
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &ChatOpts::default(),
            )
            .await
            .unwrap();
        assert_eq!(res.content, "hello");
        mock.assert_async().await;
    }

    /// `Authorization` header with `$VAR` interpolation gives plugin
    /// authors a clean way to wire bearer-style auth without storing
    /// the secret in `plugin.toml`.
    #[tokio::test]
    async fn materialized_provider_uses_env_var_auth_header() {
        let var = "SLAB_TEST_PLUGIN_AUTH_VAR_8F";
        std::env::set_var(var, "tok-abc-123");
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer tok-abc-123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"qwen2.5","choices":[{"index":0,"message":{"role":"assistant","content":"yo"}}]}"#,
            )
            .create_async()
            .await;
        let mut headers = HashMap::new();
        // Plugin author writes a literal "Bearer $VAR_NAME" — but the
        // `$VAR` syntax only resolves a whole-value reference. So we
        // require the user to put the *full* bearer string in an env
        // var, or to set just the token and prefix Bearer elsewhere.
        // For this test we use the simplest path: the env var holds
        // "Bearer tok-abc-123" and the manifest header is `$VAR`.
        std::env::set_var(var, "Bearer tok-abc-123");
        headers.insert("Authorization".to_string(), format!("${var}"));
        let mut c = contribution(headers);
        c.base_url = format!("{}/v1", server.url());
        let p = materialize_contribution(&c).unwrap();
        let res = p
            .chat(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &ChatOpts::default(),
            )
            .await;
        std::env::remove_var(var);
        let res = res.unwrap();
        assert_eq!(res.content, "yo");
        mock.assert_async().await;
    }

    /// `materialize_active` wraps `materialize_contribution`; smoke
    /// test to make sure the wrapper doesn't drop fields.
    #[test]
    fn materialize_active_delegates_to_contribution() {
        let c = contribution(HashMap::new());
        let active = ActiveAiProvider {
            plugin_id: "com.example.full".into(),
            plugin_dir: std::path::PathBuf::from("/tmp/nope"),
            provider: c,
        };
        let p = materialize_active(&active).unwrap();
        assert_eq!(p.name(), "openai-compatible");
    }
}
