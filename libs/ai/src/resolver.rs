//! 5-layer model resolver — implements terrashift_plan.md §6.X priority chain.
//!
//! Priority (lowest → highest): operator default ← profile selection ←
//! CLI launch override ← in-conversation `/model` switch ← per-call override.
//!
//! Constitution: Article V (returns `api_key_env` reference only, never
//! the resolved value; the actual env-var read happens in `RealClient`
//! at request time).

use crate::errors::AiError;
use crate::profile::{split_model_id, Profile, ProviderConfig};
use crate::tier::Tier;

/// What the resolver returns: enough to dispatch a request without
/// peeking back at the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub provider_key: String,
    pub model_id: String,
    pub provider_type: String,
    pub api_endpoint: String,
    pub api_key_env: String,
    pub tier: Tier,
}

/// 5-layer model resolver. Stateless — pure function of inputs.
pub struct Resolver;

impl Resolver {
    /// Resolve the concrete model for a tier, applying overrides in
    /// order. Layers above the function signature:
    ///
    /// 1. Operator default — `profile.model` (or per-tier).
    /// 2. Profile selection — implicit (caller passes the active profile).
    /// 3. CLI launch override — `cli_override`, e.g., from `--model`.
    /// 4. In-conversation `/model` — same `cli_override` slot in this
    ///    iteration; Stage 5+ may split the mechanism.
    /// 5. Per-call override — `call_override` for programmatic callers
    ///    (e.g., a SaaS request payload).
    pub fn resolve_for_tier(
        profile: &Profile,
        cli_override: Option<&str>,
        call_override: Option<&str>,
        tier: Tier,
    ) -> Result<ResolvedModel, AiError> {
        // Per-call wins over CLI wins over profile.
        let combined = call_override
            .or(cli_override)
            .map(|s| s.to_string())
            .unwrap_or_else(|| profile.model_for_tier(tier));

        let (provider_key, model_id) = split_model_id(&combined)?;
        let provider: &ProviderConfig = profile
            .providers
            .get(provider_key)
            .ok_or_else(|| AiError::UnknownProvider(provider_key.to_string()))?;

        Ok(ResolvedModel {
            provider_key: provider_key.to_string(),
            model_id: model_id.to_string(),
            provider_type: provider.provider_type.clone(),
            api_endpoint: provider.api_endpoint.clone(),
            api_key_env: provider.api_key_env.clone(),
            tier,
        })
    }
}
