// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `CompletionMetadata` — fields that S5 wires into `AuditPayload::LlmCall`.
//!
//! Pattern: terrashift_plan.md §6.X audit-records-the-model invariant.
//! Constitution: Article V (every LLM-touching audit entry MUST carry
//! provider + model_id + endpoint so compliance reviewers can answer
//! "which model produced this artifact?" deterministically).
//!
//! Token counts default to 0 for `StubClient` (no real LLM call); only
//! `RealClient` populates them from stakai's `Usage`.

use serde::{Deserialize, Serialize};

/// Returned alongside every completion. The exact shape needed for
/// `libs/audit/src/entry.rs::AuditPayload::LlmCall`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletionMetadata {
    /// Provider key from the profile's `[providers.<key>]` block (e.g.,
    /// `"groq"`).
    pub provider: String,
    /// Concrete model id (e.g., `"llama-3.3-70b-versatile"`).
    pub model_id: String,
    /// Resolved endpoint URL (e.g., `"https://api.groq.com/openai/v1"`).
    pub provider_endpoint: String,
    /// Tier the call was bound to (mostly informational once the model
    /// is resolved).
    pub tier: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    /// Cost in micro-USD (1e-6 USD precision). Populated from stakai's
    /// `ModelCost` when available; 0 for `StubClient`.
    pub cost_usd_micros: u64,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: u32,
}

impl CompletionMetadata {
    /// Construct a stub-friendly metadata (zero costs / tokens; only
    /// the routing fields populated). Used by `StubClient`.
    pub fn stub(
        provider: impl Into<String>,
        model_id: impl Into<String>,
        provider_endpoint: impl Into<String>,
        tier: crate::tier::Tier,
    ) -> Self {
        Self {
            provider: provider.into(),
            model_id: model_id.into(),
            provider_endpoint: provider_endpoint.into(),
            tier: tier.as_str().to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd_micros: 0,
            latency_ms: 0,
        }
    }
}
