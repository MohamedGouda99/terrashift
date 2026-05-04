// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `LlmClient` trait + `StubClient`.
//!
//! Pattern: terrashift_plan.md §6.3 (LLM Router) + the architecture reference §10
//! (provider-agnostic LLM interface). Stage 1 ships the trait + a
//! deterministic stub for tests.
//!
//! Constitution: Article XII rule 3 (tier-bound API), Article XIII
//! rule 4 (we don't redefine `ChatMessage`/`LLMMessage`; stakai's
//! typed surface owns those).

use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::tier::Tier;
use async_trait::async_trait;
use std::collections::HashMap;

/// The universal LLM contract. Stage 1: single-turn `complete`.
/// Streaming + tool-calling layer in S2+ via additional trait methods
/// (default impls return `AiError::UnsupportedFeature`).
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
    ) -> Result<(String, CompletionMetadata), AiError>;
}

/// Hermetic test stub — canned `(prompt → response)` pairs + an
/// optional default fallback. Always offline, always deterministic.
///
/// Builder usage:
///
/// ```ignore
/// let stub = StubClient::new()
///     .with_response("aws_vpc", r#"{"target_type":"azurerm_virtual_network",...}"#)
///     .with_default("UNKNOWN");
/// ```
pub struct StubClient {
    /// Substring → canned response. First match wins (BTreeMap iter
    /// is sorted by key for stability across runs — Article VI).
    canned: HashMap<String, String>,
    default: Option<String>,
    /// Provider/model_id stamped onto every `CompletionMetadata`.
    /// Defaults to `"stub" / "stub-v1"` so audit-emission paths
    /// always have valid string fields.
    provider: String,
    model_id: String,
}

impl Default for StubClient {
    fn default() -> Self {
        Self::new()
    }
}

impl StubClient {
    pub fn new() -> Self {
        Self {
            canned: HashMap::new(),
            default: None,
            provider: "stub".to_string(),
            model_id: "stub-v1".to_string(),
        }
    }

    /// First-substring-match canned response.
    pub fn with_response(mut self, prompt_substring: &str, response: &str) -> Self {
        self.canned
            .insert(prompt_substring.to_string(), response.to_string());
        self
    }

    /// Default response when no canned entry matches.
    pub fn with_default(mut self, response: &str) -> Self {
        self.default = Some(response.to_string());
        self
    }

    /// Override the provider/model_id used in `CompletionMetadata`.
    pub fn with_metadata(mut self, provider: &str, model_id: &str) -> Self {
        self.provider = provider.to_string();
        self.model_id = model_id.to_string();
        self
    }
}

#[async_trait]
impl LlmClient for StubClient {
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
    ) -> Result<(String, CompletionMetadata), AiError> {
        if prompt.is_empty() {
            return Err(AiError::EmptyPrompt);
        }
        let response = self
            .canned
            .iter()
            .find(|(needle, _)| prompt.contains(needle.as_str()))
            .map(|(_, r)| r.clone())
            .or_else(|| self.default.clone())
            .unwrap_or_else(|| "STUB:NO_RESPONSE".to_string());

        let metadata =
            CompletionMetadata::stub(&self.provider, &self.model_id, "stub://no-network", tier);
        Ok((response, metadata))
    }
}
