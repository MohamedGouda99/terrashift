//! `RealClient` — `LlmClient` facade that dispatches to the right
//! `Provider` impl per the resolved model's `provider_type`.
//!
//! Pattern: thin facade. The actual stakai dispatch lives in
//! `providers/openai_compat/provider.rs`; analogous sibling impls
//! land in `providers/{anthropic,gemini,bedrock}/` in S2+.
//!
//! Per stakpak_arch.md §39 row 1 / TERRASHIFT_MAPPING.md §A row 1:
//! `libs/ai/src/providers/{name}/{convert,mod,provider,stream,types}.rs`
//! is the canonical shape. P-03 commit `e648dea` shipped the dispatch
//! logic flat in this file as a Stage-1 narrowing; this commit
//! relocates the logic to `providers/openai_compat/` and reduces
//! `RealClient` to the canonical facade.
//!
//! Constitution: Article II (canonical mapping per §39 row 1),
//! Article XIII rule 4 (stakai owns Message/Role; we don't redefine).

use crate::client::LlmClient;
use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::provider::{build_provider, Provider};
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;

/// `LlmClient` facade. Constructed from a `ResolvedModel`; internally
/// holds a boxed `Provider` chosen by the registry. All
/// `LlmClient::complete` calls delegate to that provider's
/// `Provider::complete`.
pub struct RealClient {
    resolved: ResolvedModel,
    provider: Box<dyn Provider>,
}

impl RealClient {
    /// Construct a `RealClient` for a resolved model. Returns
    /// `Err(AiError::UnsupportedProviderType)` when `resolved.provider_type`
    /// isn't yet wired (Stage 2+ stubs in `providers/{anthropic,gemini,
    /// bedrock}/` exist for shape parity but `build_provider` short-
    /// circuits on them).
    pub fn new(resolved: ResolvedModel) -> Result<Self, AiError> {
        let provider = build_provider(&resolved)?;
        Ok(Self { resolved, provider })
    }
}

#[async_trait]
impl LlmClient for RealClient {
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
    ) -> Result<(String, CompletionMetadata), AiError> {
        self.provider.complete(tier, prompt, &self.resolved).await
    }
}
