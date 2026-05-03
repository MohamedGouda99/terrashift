//! Request-shape conversion: `(Tier, prompt)` → `stakai::GenerateRequest`.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! Stage 1: minimal — single user message, no system prompt at the
//! provider layer (the Mapper assembles its own system+user via
//! `libs/engine/src/mapper/prompt.rs`).
//!
//! S5+ this module will grow to handle streaming preferences,
//! tool definitions, structured-output schema, multi-turn message
//! history, etc.

use crate::resolver::ResolvedModel;
use crate::tier::Tier;

/// Build a stakai request for a single-turn user prompt.
///
/// `_tier` is currently informational — model is already resolved
/// upstream by the resolver. S5+ may use tier to set
/// `max_output_tokens` differently per tier (eco = small, smart =
/// large) without re-resolving the model.
pub fn build_request(
    _tier: Tier,
    prompt: &str,
    resolved: &ResolvedModel,
) -> stakai::GenerateRequest {
    stakai::GenerateRequest::new(
        stakai::Model::custom(&resolved.model_id, &resolved.provider_key),
        vec![stakai::Message::new(stakai::Role::User, prompt)],
    )
}
