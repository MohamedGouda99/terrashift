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

use crate::generator::templates::TemplateRegistry;
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

        // r07-mvp-closure / FR-1, FR-2: prompt context-injection.
        // Build the per-target-provider supported-target-type set and
        // (when cached) the per-target-type required-attributes hint.
        // Order matters: fetch target_schema FIRST so supported_types can
        // include schema-derived types alongside hand-curated ones. The
        // Mapper's prompt then advertises every type the Generator can
        // actually emit, not just the small hand-curated subset.
        let target_schema = lookup_target_schema(knowledge, &self.target_provider).await;
        let supported_types = supported_types_for(&self.target_provider, target_schema.as_ref());

        // Build the prompt + route through the reducer. Article XIII
        // rule 1: the `reducer.reduce` call is non-optional on this
        // path; the type system makes bypass impossible.
        let system_prompt = prompt::build_system_prompt(
            &self.target_provider,
            target_schema.as_ref(),
            &supported_types,
        );
        let user_prompt = prompt::build_user_prompt(
            &self.source_provider,
            &self.target_provider,
            estate,
            &knowledge_hits,
        );
        let messages = vec![
            Message {
                role: Role::System,
                content: system_prompt,
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

        // LLMs often wrap JSON in ```json ... ``` fences despite explicit
        // instructions otherwise. Strip them before parsing.
        let cleaned = strip_markdown_fences(&response);
        let plan: MappingPlan = serde_json::from_str(cleaned).map_err(|e| {
            let sample = truncate_utf8(&response, 500);
            MapperError::MalformedResponse {
                reason: e.to_string(),
                sample,
            }
        })?;

        // r07-mvp-closure / FR-3, FR-4: post-parse validation. Reject empty
        // and unsupported target_types at the Mapper boundary so users get
        // a single named error per offending source resource, not a confusing
        // `template miss for ''` two layers downstream.
        validate_plan(&plan, &supported_types)?;

        cache.insert(key, plan.clone());
        Ok(plan)
    }
}

/// Build the supported-target-type set for the Mapper prompt.
/// Hand-curated only (~10-15 types per provider) — schema-derived emission
/// is the Generator's job, not the Mapper's. Showing the LLM 900+ candidate
/// types confused it severely (Llama 3.3 70B, observed 2026-05-11). RAG
/// knowledge_hits in the user prompt expose schema-derived candidates per
/// source type. r07-mvp-closure / FR-2.
fn supported_types_for(
    target_provider: &str,
    _target_schema: Option<&terrashift_knowledge::ProviderSchema>,
) -> Vec<String> {
    let prefix = format!("{target_provider}_");
    TemplateRegistry::stage1()
        .registered_types()
        .filter(|k| k.starts_with(&prefix))
        .map(|k| k.to_string())
        .collect()
}

/// Best-effort lookup of the target provider's schema from the cache.
/// Returns `None` (graceful degradation per r07-mvp-closure clarify Q2)
/// when the cache has no entry — the prompt builder omits the
/// REQUIRED ATTRIBUTES block in that case.
async fn lookup_target_schema(
    knowledge: &KnowledgeService,
    target_provider: &str,
) -> Option<terrashift_knowledge::ProviderSchema> {
    // Method calls on `Arc<dyn SchemaStore>` resolve through the vtable,
    // so the trait doesn't need to be in scope here.
    let versions = knowledge
        .schema_store
        .list_versions(target_provider)
        .await
        .ok()?;
    let version = versions.into_iter().next()?;
    knowledge
        .schema_store
        .fetch_provider_schema(target_provider, &version)
        .await
        .ok()
}

/// Strip leading/trailing Markdown code fences (``` or ```json) plus
/// surrounding whitespace. LLMs add them despite "JSON only" instructions.
fn strip_markdown_fences(s: &str) -> &str {
    let s = s.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s)
        .trim_start();
    s.strip_suffix("```").unwrap_or(s).trim()
}

/// Lenient UUID deserializer for `MappingPlan.run_id`.
///
/// The LLM is asked for a UUID but reliably emits invalid hex digits in
/// the run_id slot (observed: `5c2a3d4e5f6g` — trailing `g`, against
/// meta-llama/Llama-3.3-70B via HF auto-router, 2026-05-11). Run IDs are
/// operational metadata — they don't *need* to come from the model. So
/// we accept any string, parse if valid, and fall back to `Uuid::new_v4`
/// when invalid. Article XIII rule 5 (LLM-unreliability tolerance).
fn deserialize_lenient_run_id<'de, D>(d: D) -> Result<Uuid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(d)?;
    Ok(Uuid::parse_str(&s).unwrap_or_else(|_| {
        tracing::debug!(emitted = %s, "LLM emitted invalid UUID for run_id; synthesizing");
        Uuid::new_v4()
    }))
}

/// Validate a parsed `MappingPlan` against the supported-target-type set.
/// Returns the first error encountered (no aggregation — Mapper is
/// fail-fast at this boundary; Recovery agent S10 handles per-resource
/// retries). r07-mvp-closure / FR-3, FR-4.
fn validate_plan(plan: &MappingPlan, supported_types: &[String]) -> Result<(), MapperError> {
    for r in &plan.resources {
        if r.target_type.is_empty() {
            return Err(MapperError::EmptyTargetType {
                source_addr: r.source_addr.clone(),
                supported: supported_types.to_vec(),
            });
        }
        if !supported_types.iter().any(|t| t == &r.target_type) {
            return Err(MapperError::UnsupportedTargetType {
                source_addr: r.source_addr.clone(),
                target_type: r.target_type.clone(),
                supported: supported_types.to_vec(),
            });
        }
    }
    Ok(())
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
#[serde(rename_all = "snake_case")]
pub struct MappingPlan {
    /// The migration run this plan belongs to. Used to scope backups
    /// (`.terrashift/runs/{run_id}/backups/`) and audit entries.
    /// Deserialization is lenient: invalid UUIDs from LLM responses are
    /// silently replaced with `Uuid::new_v4` (Article XIII rule 5 —
    /// LLM-unreliability tolerance).
    #[serde(deserialize_with = "deserialize_lenient_run_id")]
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
    /// `list` is the canonical variant name; `array` is accepted as a
    /// deserialization alias because LLMs (verified against Llama 3.3
    /// 70B via HF auto-router, 2026-05-11) reliably emit the English
    /// word `array` for list-typed Terraform attributes. Without this
    /// alias the Mapper JSON round-trip fails at deserialization —
    /// *before* Validator + Recovery get a chance to repair — and the
    /// migration aborts on a parse error. Article XIII rule 5
    /// (LLM-unreliability tolerance).
    #[serde(alias = "array")]
    List(Vec<AttributeValue>),
    /// `map` is canonical; `object` accepted as a deserialization alias
    /// for the same reason as `list`/`array` — LLMs default to "object"
    /// for keyed-value JSON regardless of the target schema's language.
    #[serde(alias = "object")]
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

#[cfg(test)]
mod tests {
    //! r07-mvp-closure tests: validate_plan + supported_types_for + the
    //! Mapper-side rejection of empty / unsupported target_types.
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    fn synthetic_plan(resources: Vec<MappedResource>) -> MappingPlan {
        MappingPlan {
            run_id: Uuid::new_v4(),
            source_provider: "aws".to_string(),
            target_provider: "azurerm".to_string(),
            resources,
        }
    }

    fn synthetic_resource(source_addr: &str, target_type: &str) -> MappedResource {
        MappedResource {
            source_addr: source_addr.to_string(),
            target_addr: format!("{target_type}.synthetic"),
            target_type: target_type.to_string(),
            target_name: "synthetic".to_string(),
            attributes: BTreeMap::new(),
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn supported_types_for_azurerm_includes_template_registry_keys() {
        let supported = supported_types_for("azurerm", None);
        // Per templates.rs:50-78, azurerm has at least 5 templates registered.
        assert!(
            supported.len() >= 5,
            "expected ≥5 azurerm templates, got {} ({:?})",
            supported.len(),
            supported
        );
        assert!(supported.iter().any(|t| t == "azurerm_virtual_network"));
        assert!(supported.iter().any(|t| t == "azurerm_subnet"));
        assert!(supported
            .iter()
            .any(|t| t == "azurerm_linux_virtual_machine"));
        assert!(supported.iter().any(|t| t == "azurerm_storage_account"));
        // Should NOT include the deprecated azurerm_virtual_machine the LLM
        // sometimes emits — that's the whole point of FR-4.
        assert!(
            !supported.iter().any(|t| t == "azurerm_virtual_machine"),
            "deprecated azurerm_virtual_machine should NOT be in the registry"
        );
    }

    #[test]
    fn supported_types_for_aws_includes_template_registry_keys() {
        let supported = supported_types_for("aws", None);
        assert!(supported.iter().any(|t| t == "aws_vpc"));
        assert!(supported.iter().any(|t| t == "aws_s3_bucket"));
    }

    #[test]
    fn supported_types_for_unknown_provider_is_empty() {
        // No templates registered for `tencentcloud_*`. Returns empty —
        // Mapper graceful behaviour: prompt will warn, validation will
        // reject every output. This is the right Stage 1 posture.
        let supported = supported_types_for("tencentcloud", None);
        assert!(supported.is_empty());
    }

    #[test]
    fn validate_plan_rejects_empty_target_type() {
        let plan = synthetic_plan(vec![synthetic_resource("aws_vpc.main", "")]);
        let supported = vec!["azurerm_virtual_network".to_string()];
        let err = validate_plan(&plan, &supported).unwrap_err();
        match err {
            MapperError::EmptyTargetType { source_addr, .. } => {
                assert_eq!(source_addr, "aws_vpc.main");
            }
            other => panic!("expected EmptyTargetType, got {other:?}"),
        }
    }

    #[test]
    fn validate_plan_rejects_unsupported_target_type() {
        // The deprecated `azurerm_virtual_machine` — exactly the failure
        // mode the e2e test surfaced.
        let plan = synthetic_plan(vec![synthetic_resource(
            "aws_instance.app",
            "azurerm_virtual_machine",
        )]);
        let supported = vec!["azurerm_linux_virtual_machine".to_string()];
        let err = validate_plan(&plan, &supported).unwrap_err();
        match err {
            MapperError::UnsupportedTargetType {
                source_addr,
                target_type,
                supported: s,
            } => {
                assert_eq!(source_addr, "aws_instance.app");
                assert_eq!(target_type, "azurerm_virtual_machine");
                assert_eq!(s, vec!["azurerm_linux_virtual_machine".to_string()]);
            }
            other => panic!("expected UnsupportedTargetType, got {other:?}"),
        }
    }

    #[test]
    fn validate_plan_accepts_supported_target_types() {
        let plan = synthetic_plan(vec![
            synthetic_resource("aws_vpc.main", "azurerm_virtual_network"),
            synthetic_resource("aws_subnet.public", "azurerm_subnet"),
        ]);
        let supported = vec![
            "azurerm_virtual_network".to_string(),
            "azurerm_subnet".to_string(),
        ];
        validate_plan(&plan, &supported).expect("supported plan must validate");
    }

    #[test]
    fn validate_plan_short_circuits_on_first_failure() {
        // First resource is empty target_type; second is unsupported.
        // Spec says we fail-fast on the first error encountered.
        let plan = synthetic_plan(vec![
            synthetic_resource("aws_vpc.first", ""),
            synthetic_resource("aws_subnet.second", "azurerm_virtual_machine"),
        ]);
        let supported = vec!["azurerm_virtual_network".to_string()];
        let err = validate_plan(&plan, &supported).unwrap_err();
        // Should be the first error, not the second.
        assert!(matches!(err, MapperError::EmptyTargetType { .. }));
    }
}
