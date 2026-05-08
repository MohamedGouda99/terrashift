// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Mapper failure modes.
//!
//! Pattern: matches `libs/engine/src/scanner/errors.rs` shape — every
//! variant named, no anonymous strings.
//! Constitution: Article IV (loud failures), Article XIII rule 3
//! (no panics in production paths).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MapperError {
    /// LLM returned a string that doesn't parse as a `MappingPlan`.
    /// `sample` is the first 500 bytes of the raw response (truncated
    /// at a UTF-8 boundary) so the operator can see what was returned.
    /// Article IV — names *both* the parse reason AND a sample.
    #[error("malformed LLM response: {reason} (raw sample: {sample:?})")]
    MalformedResponse { reason: String, sample: String },

    /// Inventory is too large to fit in the LLM's context window. Stage
    /// 1 fails loudly; Stage 2+ adds chunking. Article IV.
    #[error("inventory too large for single-call mapping: {bytes} bytes (limit: {limit} bytes)")]
    InventoryTooLarge { bytes: usize, limit: usize },

    /// Wrapped error from the underlying LLM client.
    #[error("LLM client: {0}")]
    Llm(#[from] terrashift_ai::AiError),

    /// Wrapped error from the knowledge service (RAG retrieval).
    #[error("knowledge service: {0}")]
    Knowledge(Box<terrashift_knowledge::KnowledgeError>),

    /// Inventory failed to JSON-serialize when computing the cache key
    /// (rare; would only happen on a non-Serialize-friendly type
    /// sneaking into `EstateInventory` later).
    #[error("inventory serialize: {0}")]
    Serialize(Box<serde_json::Error>),

    /// LLM emitted a `MappedResource` with `target_type == ""`. Caught at
    /// the Mapper boundary so the user sees a single named source resource
    /// rather than a misleading `template miss for ''` two layers
    /// downstream. Article IV. RFC r07-mvp-closure / FR-3.
    #[error(
        "Mapper produced empty target_type for source resource '{source_addr}' — \
         rejected at parse time. Supported target types for this provider: {supported:?}"
    )]
    EmptyTargetType {
        source_addr: String,
        supported: Vec<String>,
    },

    /// LLM emitted a `target_type` outside `TemplateRegistry::stage1()`. The
    /// Generator would `TemplateMiss` downstream; we catch it here with a
    /// useful error that names the offending resource AND the supported set.
    /// Article IV. RFC r07-mvp-closure / FR-4.
    #[error(
        "Mapper produced unsupported target_type '{target_type}' for source resource \
         '{source_addr}'. Supported target types for this provider: {supported:?}"
    )]
    UnsupportedTargetType {
        source_addr: String,
        target_type: String,
        supported: Vec<String>,
    },
}
