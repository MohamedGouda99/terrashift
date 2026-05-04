// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Mapper — LLM with structured output (single call, then deterministic).
//!
//! P-05 (S4b structural) ships the full Mapper machinery:
//! - `Mapper::map(estate, knowledge, llm, reducer, cache)` async API
//! - `ContextReducer` trait + `PassthroughContextReducer` (Article XIII rule 1)
//! - `MapperCache` keyed by Sha256 of canonical-JSON inventory (Article XII rule 2)
//! - Prompt assembly (system + user) routed through `LlmClient::complete`
//! - JSON parse → `MappingPlan` (loud `MalformedResponse` on bad input)
//!
//! Real Groq integration is one `RealClient`-vs-`StubClient` swap away;
//! the structural code ships today, the real-LLM `#[ignore]`'d test fires
//! once `GROQ_API_KEY` is set.
//!
//! Pattern: terrashift_plan.md §5 (agentic-vs-deterministic split — Mapper
//! is single-LLM-call structured-output, NOT an agent), §6.X (strict JSON
//! conforming to schemars-derived schema). Source patterns:
//! - the reference codebase (see ATTRIBUTIONS.md) (ContextReducer trait shape)
//! - the reference codebase (see ATTRIBUTIONS.md) (canonical
//!   `reduce → generate` happy-path sequence)
//! - the reference codebase (see ATTRIBUTIONS.md) (Sha256 cache-key idiom)
//!
//! Constitution: Article I (NOT an agent; single LLM call), Article III
//! (output validated by Validator P-06 downstream), Article IV (loud
//! errors), Article XII rule 2 (cache-first), Article XIII rule 1
//! (reducer on the path), Article XIII rule 3 (no panics in production),
//! Article XIII rule 4 (stakai owns Message/Role; this module's `Message`
//! is a Stage-1 local that converges with stakai's in S5+).

pub mod cache;
pub mod errors;
pub mod prompt;

pub use cache::{estate_cache_key, MapperCache};
pub use errors::MapperError;

// Article II + TERRASHIFT_MAPPING.md §A row 5: ContextReducer is a
// kernel-level seam in `terrashift-agent-core`, not a mapper-local
// type. The previous mapper-local `mod context` was a P-05 Stage-1
// shortcut; relocated to `libs/agent-core/src/context.rs` so
// Recovery / Cost Optimizer (S10/S11) can use the same trait without
// importing libs/engine.
pub use terrashift_agent_core::{ContextReducer, Message, PassthroughContextReducer, Role};

use crate::scanner::EstateInventory;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use terrashift_ai::{LlmClient, Tier};
use terrashift_knowledge::{KnowledgeService, ResourceMatch};
use thiserror::Error;
use uuid::Uuid;

/// Stage 1 inventory size cap (100 KB of canonical JSON). Inventories
/// above this fail loudly with `MapperError::InventoryTooLarge` —
/// chunking arrives in Stage 2+. Article IV.
const MAX_INVENTORY_BYTES: usize = 100_000;

/// Top-K knowledge hits per source resource type.
const KNOWLEDGE_TOP_K: usize = 5;

/// Mapper — Stage 1 single-LLM-call structured-output orchestrator.
/// Stateless apart from the source/target provider configuration.
pub struct Mapper {
    pub source_provider: String,
    pub target_provider: String,
}

impl Mapper {
    pub fn new(source_provider: impl Into<String>, target_provider: impl Into<String>) -> Self {
        Self {
            source_provider: source_provider.into(),
            target_provider: target_provider.into(),
        }
    }

    /// Run the Mapper on an `EstateInventory`. Cache-first: hits the
    /// cache before invoking the LLM. Article XII rule 2 + Article XIII
    /// rule 1 enforced via this signature (the `reducer` parameter is
    /// non-optional, so bypassing the reducer is a compile error).
    ///
    /// Stage 1 deviation from the P-05 prompt: stakai 0.3.x has no
    /// `response_format` field, so we use prompt-instructed JSON output
    /// and parse the response string. Tool-call channel pattern from
    /// the reference codebase (see ATTRIBUTIONS.md) is the
    /// production path; deferred to S5+ pending eval signal.
    #[allow(clippy::too_many_arguments)]
    pub async fn map(
        &self,
        estate: &EstateInventory,
        knowledge: &KnowledgeService,
        llm: &dyn LlmClient,
        reducer: &dyn ContextReducer,
        cache: &mut MapperCache,
    ) -> Result<MappingPlan, MapperError> {
        // Article XII rule 2 — cache-first.
        let key = estate_cache_key(estate)?;
        if let Some(cached) = cache.get(&key) {
            tracing::debug!(cache_key = %key, "Mapper::map cache hit");
            return Ok(cached.clone());
        }

        // Article XII rule 1 — empty inventory short-circuits without
        // an LLM call. Saves tokens.
        let total_resources: usize = estate.files.iter().map(|f| f.resources.len()).sum();
        if total_resources == 0 {
            tracing::info!("Mapper::map empty inventory; emitting empty MappingPlan");
            let plan = MappingPlan {
                run_id: Uuid::new_v4(),
                source_provider: self.source_provider.clone(),
                target_provider: self.target_provider.clone(),
                resources: Vec::new(),
            };
            cache.insert(key, plan.clone());
            return Ok(plan);
        }

        // Inventory size cap (Article IV).
        let bytes = serde_json::to_vec(estate)
            .map_err(|e| MapperError::Serialize(Box::new(e)))?
            .len();
        if bytes > MAX_INVENTORY_BYTES {
            return Err(MapperError::InventoryTooLarge {
                bytes,
                limit: MAX_INVENTORY_BYTES,
            });
        }

        // RAG: knowledge hits per unique source resource type.
        let mut knowledge_hits: Vec<(String, Vec<ResourceMatch>)> = Vec::new();
        let unique_types: std::collections::BTreeSet<String> = estate
            .files
            .iter()
            .flat_map(|f| f.resources.iter().map(|r| r.resource_type.clone()))
            .collect();
        for source_type in unique_types {
            let query = format!("{source_type} {} equivalent", self.target_provider);
            let hits = knowledge
                .find_similar_in_provider(&query, &self.target_provider, KNOWLEDGE_TOP_K)
                .await
                .map_err(|e| MapperError::Knowledge(Box::new(e)))?;
            knowledge_hits.push((source_type, hits));
        }

        // Build the prompt + route through the reducer. Article XIII
        // rule 1: the `reducer.reduce` call is non-optional on this
        // path; the type system makes bypass impossible.
        let user_prompt = prompt::build_user_prompt(
            &self.source_provider,
            &self.target_provider,
            estate,
            &knowledge_hits,
        );
        let messages = vec![
            Message {
                role: Role::System,
                content: prompt::SYSTEM_PROMPT.to_string(),
            },
            Message {
                role: Role::User,
                content: user_prompt,
            },
        ];
        let reduced = reducer.reduce(messages);

        // Stage 1 LLM call: collapse the (system, user) pair into one
        // string with a separator, since `LlmClient::complete` from
        // P-03 takes a single prompt. S5+ evolves this when the
        // `LlmClient` API gains typed messages.
        let combined_prompt = reduced
            .iter()
            .map(|m| {
                format!(
                    "[{}]\n{}",
                    match m.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        // Mapper is single-shot in Stage 1 — it never emits
                        // a Tool message. Variant covered for exhaustive
                        // match per S9's Role::Tool addition.
                        Role::Tool => "tool",
                    },
                    m.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let (response, _meta) = llm.complete(Tier::Eco, &combined_prompt).await?;

        // Parse the response as JSON → MappingPlan (Article III gate is
        // P-06; here we only enforce that it parses).
        let plan: MappingPlan = serde_json::from_str(&response).map_err(|e| {
            let sample = truncate_utf8(&response, 500);
            MapperError::MalformedResponse {
                reason: e.to_string(),
                sample,
            }
        })?;

        cache.insert(key, plan.clone());
        Ok(plan)
    }
}

/// Truncate a string at a UTF-8 boundary near `max_bytes`. Matches
/// Article XIII rule 3 — no `&s[..n]` slicing (that panics at
/// non-boundary indices). Falls back to the empty string only when
/// `s` is empty.
fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or("").to_string()
}

/// Output of the Mapper. Input to Validator + Generator.
///
/// Stage 1 shape; S4 (P-05) extends with: cache_key, version-pinned
/// schemas, per-resource confidence scores, fallback annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingPlan {
    /// The migration run this plan belongs to. Used to scope backups
    /// (`.terrashift/runs/{run_id}/backups/`) and audit entries.
    pub run_id: Uuid,

    /// Source provider key, e.g., "aws", "google", "azurerm".
    pub source_provider: String,

    /// Target provider key.
    pub target_provider: String,

    /// One entry per source resource. Order is the Mapper's emission order;
    /// Generator sorts by `target_addr` for byte-stable output (Article VI).
    pub resources: Vec<MappedResource>,
}

/// One source-resource → target-resource mapping. The Generator looks up a
/// template by `target_type` and feeds it `attributes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedResource {
    /// e.g., "aws_vpc.main"
    pub source_addr: String,
    /// e.g., "azurerm_virtual_network.main"
    pub target_addr: String,
    /// e.g., "azurerm_virtual_network" — first HCL block label.
    pub target_type: String,
    /// e.g., "main" — second HCL block label.
    pub target_name: String,

    /// Target-shaped attributes. `BTreeMap` for deterministic iteration
    /// order (Article VI — same input gives same output).
    pub attributes: BTreeMap<String, AttributeValue>,

    /// Other `target_addr`s this resource depends on. Stage 1 informational
    /// only; the Planner consumes this for the apply-order DAG.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// HCL-emit-friendly attribute values. Distinct from `hcl::Value` because:
/// 1. JSON-friendly serde for the Mapper-produced wire format.
/// 2. `Reference` is a raw HCL expression (e.g., `aws_vpc.main.id`) that
///    must NOT be quoted at emit time — distinguishing data from syntax.
///
/// Serde representation: **externally tagged** (the default) — variants
/// are wrapped in a single-key map, e.g. `{ "string": "10.0.0.0/16" }`,
/// `{ "reference": "aws_vpc.main.id" }`. We can't use internally-tagged
/// (`#[serde(tag = "...")]`) because the `String`/`Reference` newtype
/// variants wrap primitives, not struct/maps, and serde rejects internal
/// tagging on primitive newtypes at deserialize time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Bool(bool),
    List(Vec<AttributeValue>),
    Map(BTreeMap<String, AttributeValue>),
    /// A raw HCL expression — emitted unquoted. Examples:
    /// `"aws_vpc.main.id"`, `"var.region"`, `"data.aws_ami.example.id"`.
    Reference(String),
}

impl MappedResource {
    /// Extract a required string attribute, or return a structured error
    /// the Generator wraps into `GeneratorError::Lookup`.
    pub fn require_string(&self, attr: &str) -> Result<&str, MapperLookupError> {
        match self.attributes.get(attr) {
            Some(AttributeValue::String(s)) => Ok(s),
            Some(_) => Err(MapperLookupError::WrongType {
                addr: self.target_addr.clone(),
                attr: attr.to_string(),
                expected: "string",
            }),
            None => Err(MapperLookupError::Missing {
                addr: self.target_addr.clone(),
                attr: attr.to_string(),
            }),
        }
    }

    /// Optional string getter. None if absent or wrong type — caller decides.
    pub fn optional_string(&self, attr: &str) -> Option<&str> {
        match self.attributes.get(attr) {
            Some(AttributeValue::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Optional bool getter.
    pub fn optional_bool(&self, attr: &str) -> Option<bool> {
        match self.attributes.get(attr) {
            Some(AttributeValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Optional reference getter — returns the raw HCL expression string
    /// without quotes (e.g., `"aws_vpc.main.id"`).
    pub fn optional_reference(&self, attr: &str) -> Option<&str> {
        match self.attributes.get(attr) {
            Some(AttributeValue::Reference(r)) => Some(r.as_str()),
            _ => None,
        }
    }

    /// Optional map getter — for nested blocks like `tags = { ... }`.
    pub fn optional_map(&self, attr: &str) -> Option<&BTreeMap<String, AttributeValue>> {
        match self.attributes.get(attr) {
            Some(AttributeValue::Map(m)) => Some(m),
            _ => None,
        }
    }
}

/// Errors from `MappedResource` lookup helpers.
#[derive(Debug, Error)]
pub enum MapperLookupError {
    #[error("missing required attribute '{attr}' on resource '{addr}'")]
    Missing { addr: String, attr: String },
    #[error("attribute '{attr}' on resource '{addr}' has wrong type (expected {expected})")]
    WrongType {
        addr: String,
        attr: String,
        expected: &'static str,
    },
}
