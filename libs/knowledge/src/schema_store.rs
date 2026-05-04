// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `SchemaStore` trait — the seam every knowledge backend implements.
//!
//! Pattern: the architecture reference §9 (SessionStorage trait shape — async methods,
//! Send+Sync, typed error).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (`pub trait SessionStorage`).
//! Constitution: Article VI (knowledge integrity), X (every method gets a tracing span).

use crate::errors::SchemaError;
use crate::types::{MappingExample, ProviderSchema};
use async_trait::async_trait;

/// Local-first store for provider schemas + mapping examples.
///
/// Stage 1 implementations: `LocalSchemaStore` (sqlx + sqlite).
/// Stage 6: `RemoteSchemaStore` (HTTP API into a managed mirror).
#[async_trait]
pub trait SchemaStore: Send + Sync {
    /// Retrieve a previously cached schema. Returns `NotFound` on miss.
    async fn fetch_provider_schema(
        &self,
        provider: &str,
        version: &str,
    ) -> Result<ProviderSchema, SchemaError>;

    /// Cache a schema. Returns `Ok(true)` if newly inserted, `Ok(false)` if
    /// already present (Article VI — pinned versions are immutable; silent
    /// no-op on re-cache attempts is the explicit signal).
    async fn cache_schema(&self, schema: &ProviderSchema) -> Result<bool, SchemaError>;

    /// List all cached versions for a provider, sorted descending (newest first).
    async fn list_versions(&self, provider: &str) -> Result<Vec<String>, SchemaError>;

    /// Search the mappings corpus by free-text query. Stage 1: returns empty
    /// (RAG implementation arrives in S17). API stable so Mapper can compile.
    async fn search_mappings(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<MappingExample>, SchemaError>;
}
