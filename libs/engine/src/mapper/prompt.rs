// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Mapper prompt assembly — system + user templates.
//!
//! Pattern: terrashift_plan.md §6.3 (LLM Router design — components
//! bind to a tier; Mapper uses `Eco`).
//! Constitution: Article III (system prompt grounds the LLM with the
//! schema requirement; Validator P-06 enforces the actual schema
//! check downstream).
//!
//! Stage 1 deviation: stakai 0.3 has no `response_format` field, so
//! we instruct via system prompt + parse the response as JSON. If
//! eval signal shows malformed-JSON rates are problematic, S5+ adopts
//! the tool-call channel pattern from
//! the reference codebase (see ATTRIBUTIONS.md)
//!
//! Cache invariant: this prompt is the cache key's content half (the
//! input half is the inventory hash). Article XIII rule 2 — once
//! shipped, changes to `SYSTEM_PROMPT` invalidate every cached
//! mapping. Bump the embedded version string when intentionally
//! changing semantics.

use crate::scanner::EstateInventory;
use std::fmt::Write as _;
use terrashift_knowledge::{MappingExample, ProviderSchema, ResourceMatch};

/// Static header — the rules + schema portion. Composed with two dynamic
/// blocks (VALID TARGET TYPES + REQUIRED ATTRIBUTES) into the full system
/// prompt by `build_system_prompt`.
///
/// The version string at the head is the cache key half — ANY change here
/// invalidates every cached `MappingPlan`. Article XII rule 2: bump
/// intentionally. r07-mvp-closure: bumped `v1 → v2` to introduce the
/// dynamic context blocks.
pub const SYSTEM_PROMPT_HEADER: &str = "Terrashift Mapper v2.\n\
You translate Terraform resources from one cloud provider to another.\n\
\n\
Rules:\n\
1. Output ONLY a single JSON object conforming to the MappingPlan schema below. \
No prose, no markdown fences, no commentary.\n\
2. Every source resource MUST have exactly one MappedResource entry.\n\
3. For every required attribute on the target resource type, set a string, \
number, bool, or Reference value. Refer to the REQUIRED ATTRIBUTES section \
below for the per-target-type list. NEVER omit a required attribute.\n\
4. References (e.g., aws_vpc.main.id) use {\"reference\": \"aws_vpc.main.id\"} — \
NOT a quoted string.\n\
5. CIDR ranges, AMI ids, regions are STRINGS — wrap them with \
{\"string\": \"...\"}. Lists wrap with {\"list\": [...]}. Maps wrap with \
{\"map\": {...}}. Never emit a raw scalar where an AttributeValue is expected.\n\
6. The `target_type` MUST be one of the names listed in the VALID TARGET \
TYPES section below. If a source resource has no listed equivalent, omit it \
entirely (NOT an entry with empty target_type — the operator's downstream \
report will surface the gap). Do NOT invent a hallucinated target type.\n\
7. `source_addr`, `target_addr`, `target_type`, `target_name`, and entries \
inside `dependencies` are PLAIN JSON STRINGS, NOT envelope-tagged. \
For example: \"dependencies\": [\"azurerm_virtual_network.main.id\"] — \
NOT [{\"reference\": \"azurerm_virtual_network.main.id\"}]. \
Only values inside `attributes` use the envelope tags.\n\
\n\
MappingPlan schema (JSON):\n\
{\n\
  \"run_id\": \"uuid-v4\",\n\
  \"source_provider\": \"google|aws|azurerm\",\n\
  \"target_provider\": \"google|aws|azurerm\",\n\
  \"resources\": [\n\
    {\n\
      \"source_addr\": \"google_compute_network.main\",\n\
      \"target_addr\": \"aws_vpc.main\",\n\
      \"target_type\": \"aws_vpc\",\n\
      \"target_name\": \"main\",\n\
      \"attributes\": { \"cidr_block\": { \"string\": \"10.0.0.0/16\" } },\n\
      \"dependencies\": [\"aws_vpc.main.id\"]\n\
    }\n\
  ]\n\
}\n";

/// Build the full system prompt by composing the static header with two
/// dynamic blocks injected from the runtime context:
///
/// 1. **VALID TARGET TYPES** — sorted list of `TemplateRegistry` keys
///    filtered by target-provider prefix. Constrains the LLM to types the
///    Generator can actually serialize (FR-2). Always present.
///
/// 2. **REQUIRED ATTRIBUTES** — per-target-type list of attribute names
///    flagged `required: true` in `target_schema`. Source: bundled or
///    runtime schema cache via `KnowledgeService`. Optional — when
///    `target_schema` is `None`, the block is omitted with a comment.
///
/// `r07-mvp-closure` / FR-1, FR-2, FR-6.
pub fn build_system_prompt(
    target_provider: &str,
    target_schema: Option<&ProviderSchema>,
    supported_types: &[String],
) -> String {
    let mut out = String::with_capacity(SYSTEM_PROMPT_HEADER.len() + 2_048);
    out.push_str(SYSTEM_PROMPT_HEADER);
    out.push('\n');

    // Block 1 — VALID TARGET TYPES (always present).
    let _ = writeln!(out, "=== VALID TARGET TYPES ({target_provider}) ===");
    if supported_types.is_empty() {
        out.push_str("(no templates registered for this target provider — Mapper will reject every output)\n");
    } else {
        let mut sorted = supported_types.to_vec();
        sorted.sort();
        for t in &sorted {
            let _ = writeln!(out, "- {t}");
        }
    }
    out.push('\n');

    // Block 2 — REQUIRED ATTRIBUTES (per target type, if schema cached).
    if let Some(schema) = target_schema {
        let _ = writeln!(out, "=== REQUIRED ATTRIBUTES (per target type) ===");
        let mut sorted_supported = supported_types.to_vec();
        sorted_supported.sort();
        for t in &sorted_supported {
            if let Some(rs) = schema.resources.get(t) {
                let mut required: Vec<&str> = rs
                    .attributes
                    .iter()
                    .filter(|(_, a)| a.required)
                    .map(|(name, _)| name.as_str())
                    .collect();
                if required.is_empty() {
                    continue;
                }
                required.sort();
                let _ = writeln!(out, "{t}: MUST set {{ {} }}", required.join(", "));
            }
        }
        out.push('\n');
    } else {
        out.push_str(
            "(REQUIRED ATTRIBUTES block omitted — no target schema cached. \
             Use general knowledge of the provider; Validator will catch \
             missing required attributes downstream.)\n\n",
        );
    }

    out
}

/// Build the user prompt body — source + target providers, the
/// resource list, and the knowledge-service top-K target candidates
/// per source resource type.
pub fn build_user_prompt(
    source_provider: &str,
    target_provider: &str,
    inventory: &EstateInventory,
    knowledge_hits: &[(String, Vec<ResourceMatch>)],
    curated: &[&MappingExample],
) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "Map the following Terraform estate from `{source_provider}` to `{target_provider}`.\n\n",
    ));

    s.push_str("=== SOURCE INVENTORY ===\n");
    for file in &inventory.files {
        for resource in &file.resources {
            s.push_str(&format!("- {}.{}\n", resource.resource_type, resource.name));
        }
    }
    if inventory.files.iter().all(|f| f.resources.is_empty()) {
        s.push_str("(no resources)\n");
    }
    s.push('\n');

    if !curated.is_empty() {
        s.push_str("=== PROVEN EQUIVALENCES (authoritative — prefer these) ===\n");
        for m in curated {
            let _ = writeln!(
                s,
                "{} → {}  (confidence {:.2})",
                m.source_resource, m.target_resource, m.confidence
            );
            if !m.attribute_alignments.is_empty() {
                let pairs = m
                    .attribute_alignments
                    .iter()
                    .map(|(src, tgt)| format!("{src}={tgt}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(s, "  attrs: {pairs}");
            }
        }
        s.push('\n');
    }

    s.push_str("=== KNOWLEDGE HITS (top-K target candidates per source type) ===\n");
    if knowledge_hits.is_empty() {
        s.push_str("(no knowledge hits — proceed with general knowledge of the target provider)\n");
    } else {
        for (source_type, hits) in knowledge_hits {
            if hits.is_empty() {
                continue;
            }
            s.push_str(&format!(
                "{source_type} → candidates: {}\n",
                hits.iter()
                    .map(|h| h.resource_type.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    s.push('\n');

    s.push_str("=== OUTPUT ===\nReturn ONLY the MappingPlan JSON object.\n");
    s
}

#[cfg(test)]
mod tests {
    //! r07-mvp-closure tests: prompt builder context-injection.
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use chrono::Utc;
    use std::collections::BTreeMap;
    use terrashift_knowledge::{AttributeSchema, ResourceSchema};

    fn schema_with_required(target_type: &str, required: &[&str]) -> ProviderSchema {
        let mut attrs = BTreeMap::new();
        for name in required {
            attrs.insert(
                name.to_string(),
                AttributeSchema {
                    name: name.to_string(),
                    attribute_type: "string".to_string(),
                    required: true,
                    optional: false,
                    computed: false,
                    sensitive: false,
                    deprecated: None,
                    description: None,
                },
            );
        }
        // Add one optional attribute to ensure the filter only picks required.
        attrs.insert(
            "tags".to_string(),
            AttributeSchema {
                name: "tags".to_string(),
                attribute_type: "map(string)".to_string(),
                required: false,
                optional: true,
                computed: false,
                sensitive: false,
                deprecated: None,
                description: None,
            },
        );
        let mut resources = BTreeMap::new();
        resources.insert(
            target_type.to_string(),
            ResourceSchema {
                name: target_type.to_string(),
                description: None,
                attributes: attrs,
            },
        );
        ProviderSchema {
            provider: "azurerm".to_string(),
            version: "test".to_string(),
            resources,
            data_sources: BTreeMap::new(),
            fetched_at: Utc::now(),
        }
    }

    #[test]
    fn header_carries_v2_version_string() {
        // Article XII rule 2: cache key invalidates on intentional bump.
        // Bumping v1 → v2 in r07-mvp-closure is the documented goalpost
        // move. This test pins the version so accidental edits don't
        // silently invalidate every operator's cache.
        assert!(SYSTEM_PROMPT_HEADER.starts_with("Terrashift Mapper v2."));
    }

    #[test]
    fn build_system_prompt_includes_valid_target_types_block() {
        let supported = vec![
            "azurerm_virtual_network".to_string(),
            "azurerm_subnet".to_string(),
        ];
        let prompt = build_system_prompt("azurerm", None, &supported);
        assert!(prompt.contains("=== VALID TARGET TYPES (azurerm) ==="));
        assert!(prompt.contains("- azurerm_virtual_network"));
        assert!(prompt.contains("- azurerm_subnet"));
    }

    #[test]
    fn build_system_prompt_handles_empty_supported_types_loudly() {
        let prompt = build_system_prompt("tencentcloud", None, &[]);
        assert!(prompt.contains("no templates registered"));
    }

    #[test]
    fn build_system_prompt_with_schema_includes_required_attributes() {
        let schema = schema_with_required(
            "azurerm_virtual_network",
            &["name", "resource_group_name", "address_space", "location"],
        );
        let supported = vec!["azurerm_virtual_network".to_string()];
        let prompt = build_system_prompt("azurerm", Some(&schema), &supported);
        assert!(prompt.contains("=== REQUIRED ATTRIBUTES (per target type) ==="));

        // Find the per-resource MUST-set line and inspect it precisely.
        // (Word "tags" might appear in the static header in some future
        // edit; the assertion has to be scoped to the line we generated.)
        let must_line = prompt
            .lines()
            .find(|l| l.starts_with("azurerm_virtual_network: MUST set"))
            .expect("MUST set line for azurerm_virtual_network missing from prompt");

        // All four required must show up on this line.
        for attr in &["name", "resource_group_name", "address_space", "location"] {
            assert!(
                must_line.contains(attr),
                "expected '{attr}' in MUST set line, got: {must_line}"
            );
        }
        // Optional `tags` must NOT show up on the required line.
        assert!(
            !must_line.contains("tags"),
            "tags is optional, should not be on the MUST set line: {must_line}"
        );
    }

    #[test]
    fn build_system_prompt_without_schema_omits_required_block_with_explanation() {
        let supported = vec!["azurerm_virtual_network".to_string()];
        let prompt = build_system_prompt("azurerm", None, &supported);
        assert!(!prompt.contains("=== REQUIRED ATTRIBUTES"));
        assert!(prompt.contains("REQUIRED ATTRIBUTES block omitted"));
    }
}
