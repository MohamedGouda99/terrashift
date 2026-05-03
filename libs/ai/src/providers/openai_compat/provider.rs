//! `OpenAiCompat` — `Provider` trait impl for OpenAI-shape endpoints.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! Source: P-03 commit `e648dea` shipped this logic in
//! `libs/ai/src/real.rs`; this commit relocates it into the
//! canonical providers/openai_compat/ subtree per §39 row 1.
//!
//! Constitution: Article V (api_key_env read at request time;
//! never held as instance state. The `_api_key` local in `complete`
//! is stack-allocated and dropped at end of scope), Article XIII
//! rule 3 (no panics in production paths).

use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::provider::Provider;
use crate::providers::openai_compat::convert::build_request;
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;
use std::time::Instant;

/// OpenAI-shape provider. Construction is trivial (no config);
/// per-call we build a stakai `Inference` configured with the
/// resolver's `api_endpoint` + the operator's API key from env.
///
/// **Why per-call config**: the operator's profile may select different
/// `api_endpoint` per provider entry (Groq vs Together vs HuggingFace
/// Inference Providers vs custom Vodafone gateway), and each may use a
/// different `api_key_env`. Stakai's `InferenceConfig` bakes both in,
/// so we build a fresh `Inference` per call. Keeps Article V clean
/// (no instance state holds creds).
pub struct OpenAiCompat;

impl Default for OpenAiCompat {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiCompat {
    pub fn new() -> Self {
        Self
    }

    /// Read the API key from the env var named in
    /// `resolved.api_key_env`. Article V: this is the only place
    /// we touch the real value; it's NOT stored on `Self`.
    fn read_api_key(resolved: &ResolvedModel) -> Result<String, AiError> {
        std::env::var(&resolved.api_key_env).map_err(|_| AiError::MissingApiKey {
            env_var: resolved.api_key_env.clone(),
            provider_key: resolved.provider_key.clone(),
        })
    }
}

#[async_trait]
impl Provider for OpenAiCompat {
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
        resolved: &ResolvedModel,
    ) -> Result<(String, CompletionMetadata), AiError> {
        if prompt.is_empty() {
            return Err(AiError::EmptyPrompt);
        }

        // Article V: read at request time, drop at end of scope.
        // The api_key local moves into stakai's InferenceConfig and
        // is dropped when the function returns.
        let api_key = Self::read_api_key(resolved)?;

        // Build a stakai Inference configured to route via the OpenAI
        // provider but pointing at the operator's openai-compatible
        // endpoint (Groq / HuggingFace Inference Providers / Together /
        // a custom enterprise gateway / etc.). The `Model::custom`
        // call in convert.rs uses provider="openai" so stakai's router
        // dispatches through the OpenAI provider — that provider then
        // uses the configured base_url instead of the default
        // https://api.openai.com/v1.
        let stakai_config =
            stakai::InferenceConfig::new().openai(api_key, Some(resolved.api_endpoint.clone()));
        let inference = stakai::Inference::with_config(stakai_config)
            .map_err(|e| AiError::Stakai(Box::new(e)))?;

        let started = Instant::now();
        let request = build_request(tier, prompt, resolved);
        let response = inference
            .generate(&request)
            .await
            .map_err(|e| AiError::Stakai(Box::new(e)))?;

        let elapsed = started.elapsed();
        let latency_ms = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);

        // Stage 1: token-count fields are 0; S4-close populates them
        // from stakai's response usage struct once the 0.3.x API for
        // it is locked in.
        let metadata = CompletionMetadata {
            provider: resolved.provider_key.clone(),
            model_id: resolved.model_id.clone(),
            provider_endpoint: resolved.api_endpoint.clone(),
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
