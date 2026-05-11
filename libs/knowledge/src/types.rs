// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Provider schema types for the Terrashift knowledge layer.
//!
//! Pattern: the architecture reference §9 (typed substrate; analogous to the reference's Session/Checkpoint).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (typed Session struct shape).
//! Constitution: Article VI (knowledge layer integrity — these types carry the
//! version-pinned snapshot the Mapper + Validator depend on).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level: a single provider+version snapshot. Stored in cache as one row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSchema {
    /// Terraform provider name (e.g., "aws", "azurerm", "google").
    pub provider: String,
    /// Pinned version (e.g., "5.30.0"). Per Article VI, no "latest" in production.
    pub version: String,
    /// Provider-level metadata + per-resource schemas.
    pub resources: BTreeMap<String, ResourceSchema>,
    /// Data-source schemas. Defaults to empty so that seed JSON captured
    /// before the data_sources field was added still loads (Article VI —
    /// version-pinned snapshots are stable; the deserializer must accept
    /// older captures rather than silently dropping them).
    #[serde(default)]
    pub data_sources: BTreeMap<String, ResourceSchema>,
    pub fetched_at: DateTime<Utc>,
}

/// Per-resource (or per-data-source) schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceSchema {
    pub name: String,
    pub description: Option<String>,
    pub attributes: BTreeMap<String, AttributeSchema>,
}

/// Per-attribute schema. Mirrors the Terraform Registry API attribute shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttributeSchema {
    pub name: String,
    /// HCL type expression (e.g., "string", "list(string)", "object({...})").
    pub attribute_type: String,
    pub required: bool,
    pub optional: bool,
    pub computed: bool,
    pub sensitive: bool,
    pub deprecated: Option<String>,
    pub description: Option<String>,
}

/// Cross-cloud mapping example surfaced by `search_mappings` (Stage 3+ RAG).
/// Stage 1: type defined for API stability; `search_mappings` returns Vec::new().
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingExample {
    pub source_resource: String, // e.g., "aws_vpc"
    pub target_resource: String, // e.g., "azurerm_virtual_network"
    pub source_provider: String,
    pub target_provider: String,
    pub attribute_alignments: Vec<(String, String)>,
    pub confidence: f64,
}
