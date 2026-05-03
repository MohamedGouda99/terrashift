//! `Provider` trait — single seam every concrete provider implements.
//!
//! Pattern: stakpak_arch.md §39 row 1 / TERRASHIFT_MAPPING.md §A row 1
//! canonical location: `libs/ai/src/provider/mod.rs` (trait) +
//! `libs/ai/src/providers/{name}/` (concrete impls). Mirrors Stakpak's
//! `refs/stakpak/libs/ai/src/provider/` shape.
//!
//! Stage 1 narrows the Stakpak `Provider` 4-method surface to a single
//! method (`complete`). Streaming + tool-call + structured-output
//! methods land in S5+ when consumers (Recovery agent, Cost Optimizer)
//! actually need them. The narrowing is documented inline so S5+
//! widening is additive (extra methods with default impls), not breaking.
//!
//! Constitution: Article II (canonical mapping per §39 row 1),
//! Article V (api_key_env reference; concrete impls read env at
//! request time, never instance state).

use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;

/// The single trait every concrete provider implements. `RealClient`
/// (in `real.rs`) builds the right impl via `build_provider()` and
/// delegates `LlmClient::complete` calls to it.
///
/// Stage 1 = single method. Stage 5+ extends with `complete_streaming`,
/// `complete_with_tools`, `complete_with_schema` — all with default
/// impls returning `AiError::UnsupportedFeature` so Stage 1 impls
/// don't have to opt in.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Single-shot completion. Implementations:
    /// - Read the API key from `resolved.api_key_env` at request time
    ///   (Article V: never hold the resolved value as instance state)
    /// - Build the provider-specific request shape from `(tier, prompt)`
    ///   via `convert.rs` helpers
    /// - Dispatch via the underlying SDK (stakai for Stage 1)
    /// - Return the response text + `CompletionMetadata` for audit
    ///   (Article V invariant: `provider`, `model_id`, `provider_endpoint`)
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
        resolved: &ResolvedModel,
    ) -> Result<(String, CompletionMetadata), AiError>;
}

/// Registry function — picks the right `Provider` impl by
/// `resolved.provider_type`. Stage 1 wires `openai-compatible`
/// (Groq, OpenAI, custom Vodafone gateway, etc.); other types
/// return their concrete-stub which itself returns
/// `AiError::UnsupportedProviderType` until S2+ fills them in.
///
/// This is the seam that grew naturally from P-03's hard-coded
/// "if provider_type != 'openai-compatible' → Err" into the
/// canonical Stakpak shape.
pub fn build_provider(resolved: &ResolvedModel) -> Result<Box<dyn Provider>, AiError> {
    use crate::providers::openai_compat;

    // Stage 1: only openai-compatible is functional. The stubs at
    // `providers/{anthropic,gemini,bedrock}/` exist for Stakpak-shape
    // parity (so the `match` covers every type a profile might
    // declare), but `build_provider` short-circuits on those types
    // for fail-fast UX — operators learn at construction time, not
    // at first complete() call. When S2+ wires real impls, replace
    // the `Err` arm with `Ok(Box::new(<provider>::new()))`.
    match resolved.provider_type.as_str() {
        "openai-compatible" => Ok(Box::new(openai_compat::OpenAiCompat::new())),
        "anthropic" | "gemini" | "bedrock" => Err(AiError::UnsupportedProviderType(
            resolved.provider_type.clone(),
        )),
        other => Err(AiError::UnsupportedProviderType(other.to_string())),
    }
}

// Suppress dead-code warnings on the Stage 2+ stub provider structs
// — they're public surface area for Stakpak shape parity, even
// though `build_provider` doesn't construct them yet.
#[doc(hidden)]
#[allow(dead_code)]
fn _stage2_stubs_referenced() {
    let _ = crate::providers::anthropic::Anthropic::new();
    let _ = crate::providers::gemini::Gemini::new();
    let _ = crate::providers::bedrock::Bedrock::new();
}
