// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `bedrock` provider — AWS Bedrock-hosted models.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md) Full impl
//! deferred to S3+ (multi-cloud stage; AWS Bedrock makes most sense
//! once we have AWS-as-target migrations producing volume that
//! benefits from the Bedrock free-tier credits). Stage 1: the seam
//! exists with the canonical `Provider` trait impl returning
//! `AiError::UnsupportedProviderType` so the registry compiles.

use crate::errors::AiError;
use crate::metadata::CompletionMetadata;
use crate::provider::Provider;
use crate::resolver::ResolvedModel;
use crate::tier::Tier;
use async_trait::async_trait;

pub struct Bedrock;

impl Default for Bedrock {
    fn default() -> Self {
        Self::new()
    }
}

impl Bedrock {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for Bedrock {
    async fn complete(
        &self,
        _tier: Tier,
        _prompt: &str,
        _resolved: &ResolvedModel,
    ) -> Result<(String, CompletionMetadata), AiError> {
        Err(AiError::UnsupportedProviderType("bedrock".to_string()))
    }
}
