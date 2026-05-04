// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `anthropic` provider — Claude / Sonnet / Opus / Haiku via the
//! Anthropic Messages API.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md) Full
//! impl deferred to S2+ when Recovery agent + Cost Optimizer agent
//! actually use it (Article I — no new agents in Stage 1, so no
//! consumer for the smart-tier model selection that drives Anthropic
//! adoption).
//!
//! Stage 1: the seam exists (`Anthropic::new()` + impl Provider)
//! returning `AiError::UnsupportedProviderType("anthropic")` so the
//! `build_provider` registry compiles + dispatches correctly without
//! the operator knowing whether the impl is real yet. Same pattern
//! as `libs/creds/{aws,gcp,azure}.rs`.

use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::provider::Provider;
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;

pub struct Anthropic;

impl Default for Anthropic {
    fn default() -> Self {
        Self::new()
    }
}

impl Anthropic {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for Anthropic {
    async fn complete(
        &self,
        _tier: Tier,
        _prompt: &str,
        _resolved: &ResolvedModel,
    ) -> Result<(String, CompletionMetadata), AiError> {
        Err(AiError::UnsupportedProviderType("anthropic".to_string()))
    }
}
