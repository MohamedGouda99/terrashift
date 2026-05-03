//! `gemini` provider — Google Gemini via the Generative Language API.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md) Full impl
//! deferred to S2+. Stage 1: the seam exists with the canonical
//! `Provider` trait impl returning `AiError::UnsupportedProviderType`
//! so the registry compiles. Same stub pattern as `anthropic`.

use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::provider::Provider;
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;

pub struct Gemini;

impl Default for Gemini {
    fn default() -> Self {
        Self::new()
    }
}

impl Gemini {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for Gemini {
    async fn complete(
        &self,
        _tier: Tier,
        _prompt: &str,
        _resolved: &ResolvedModel,
    ) -> Result<(String, CompletionMetadata), AiError> {
        Err(AiError::UnsupportedProviderType("gemini".to_string()))
    }
}
