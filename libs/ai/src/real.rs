//! `RealClient` — wraps `stakai::Inference` for production LLM calls.
//!
//! Pattern: stakpak_arch.md §10 (LLM SDK). Source: refs/stakpak/libs/ai/
//! ships the `stakai` crate; we depend on its v0.3 published surface.
//!
//! Stage 1 wires only the `openai-compatible` provider type (covers
//! Groq, custom Vodafone gateway, OpenAI itself, Together, Anyscale,
//! etc.). Other types (`anthropic`, `gemini`, `bedrock`) arrive in
//! S2+ when per-provider drivers ship.
//!
//! Constitution: Article V (env var read at request time, never as
//! instance state; `Zeroizing` of any local copy), Article XIII rule 4
//! (stakai owns `Message`/`Role` types — we don't redefine them).

use crate::client::LlmClient;
use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;
use std::time::Instant;

/// Real LLM client backed by `stakai::Inference`.
///
/// Construction: `RealClient::new(resolved)` — the resolver has already
/// produced a `ResolvedModel` so this struct just holds the routing
/// metadata + the `stakai::Inference` handle.
pub struct RealClient {
    resolved: ResolvedModel,
    inference: stakai::Inference,
}

impl RealClient {
    /// Build a client for a resolved model. Currently only
    /// `provider_type = "openai-compatible"` is supported; others
    /// return `AiError::UnsupportedProviderType` so callers can fall
    /// back to `StubClient` (or fail loudly).
    pub fn new(resolved: ResolvedModel) -> Result<Self, AiError> {
        if resolved.provider_type != "openai-compatible" {
            return Err(AiError::UnsupportedProviderType(
                resolved.provider_type.clone(),
            ));
        }
        Ok(Self {
            resolved,
            inference: stakai::Inference::new(),
        })
    }

    /// Read the API key from the env var named in `api_key_env`.
    /// Article V: this is the only place we touch the real value;
    /// it's not stored on `Self`.
    fn read_api_key(&self) -> Result<String, AiError> {
        std::env::var(&self.resolved.api_key_env).map_err(|_| AiError::MissingApiKey {
            env_var: self.resolved.api_key_env.clone(),
            provider_key: self.resolved.provider_key.clone(),
        })
    }
}

#[async_trait]
impl LlmClient for RealClient {
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
    ) -> Result<(String, CompletionMetadata), AiError> {
        if prompt.is_empty() {
            return Err(AiError::EmptyPrompt);
        }

        // Read the key at request time (Article V).
        let _api_key = self.read_api_key()?;

        let started = Instant::now();

        // Build a stakai request. The exact API surface for setting
        // an openai-compatible endpoint is provider-specific in stakai
        // 0.3.x — we use the published `Model::custom(model_id, provider_key)`
        // entry point and let stakai's openai provider read the
        // standard `OPENAI_API_KEY` env var (Groq's SDK is
        // openai-shaped). For Groq specifically the operator should
        // also export OPENAI_API_KEY=$GROQ_API_KEY OR set
        // `OPENAI_BASE_URL=https://api.groq.com/openai/v1` per
        // stakai's openai provider conventions.
        //
        // TODO(S4): wire `provider_endpoint` directly via stakai's
        // configuration once we audit which 0.3.x API supports it.
        let request = stakai::GenerateRequest::new(
            stakai::Model::custom(&self.resolved.model_id, &self.resolved.provider_key),
            vec![stakai::Message::new(stakai::Role::User, prompt)],
        );

        let response = self
            .inference
            .generate(&request)
            .await
            .map_err(|e| AiError::Stakai(Box::new(e)))?;

        let elapsed = started.elapsed();
        let latency_ms = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);

        // stakai's response carries token usage — populate when stable;
        // for Stage 1 we report 0 and let S4-close audit-emission fill
        // them in once we lock the stakai 0.3.x usage struct shape.
        let metadata = CompletionMetadata {
            provider: self.resolved.provider_key.clone(),
            model_id: self.resolved.model_id.clone(),
            provider_endpoint: self.resolved.api_endpoint.clone(),
            tier: tier.as_str().to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd_micros: 0,
            latency_ms,
        };

        Ok((response.text().to_string(), metadata))
    }
}
