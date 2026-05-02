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
//! refs/stakpak/libs/agent-core/src/agent.rs:284-298.
//!
//! Cache invariant: this prompt is the cache key's content half (the
//! input half is the inventory hash). Article XIII rule 2 — once
//! shipped, changes to `SYSTEM_PROMPT` invalidate every cached
//! mapping. Bump the embedded version string when intentionally
//! changing semantics.

use crate::scanner::EstateInventory;
use terrashift_knowledge::ResourceMatch;

/// Pinned by the version string at the head — change ⇒ cache miss
/// for every prior input. Article VI applies: keep it stable across
/// runs unless we explicitly intend a re-mapping.
pub const SYSTEM_PROMPT: &str = "Terrashift Mapper v1.\n\
You translate Terraform resources from one cloud provider to another.\n\
\n\
Rules:\n\
1. Output ONLY a single JSON object conforming to the MappingPlan schema below. \
No prose, no markdown fences, no commentary.\n\
2. Every source resource MUST have exactly one MappedResource entry.\n\
3. For every required attribute on the target resource type, set a string, \
number, bool, or Reference value.\n\
4. References (e.g., aws_vpc.main.id) use {\"reference\": \"aws_vpc.main.id\"} — \
NOT a quoted string.\n\
5. If a source resource has no clean target equivalent, omit it AND the \
operator will be told via the validator's downstream report. Do NOT invent \
a hallucinated target type.\n\
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
      \"dependencies\": []\n\
    }\n\
  ]\n\
}\n";

/// Build the user prompt body — source + target providers, the
/// resource list, and the knowledge-service top-K target candidates
/// per source resource type.
pub fn build_user_prompt(
    source_provider: &str,
    target_provider: &str,
    inventory: &EstateInventory,
    knowledge_hits: &[(String, Vec<ResourceMatch>)],
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
