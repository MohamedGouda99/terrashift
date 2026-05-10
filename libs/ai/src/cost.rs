// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Token-cost telemetry — `TierCosts`, aggregator, operator-facing summary.
//!
//! Pattern: terrashift_plan.md §6.5 (token economy) + Article XII rules 2/4.
//! Constitution: Article IV (unknown-tier metas surface in `by_tier`, not
//! silently dropped); Article XII rule 4 (deterministic — same metadata +
//! same rates → same total); Article XIII rule 3 (no unwrap/expect/string
//! slice in production paths).
//!
//! ## Where this fits
//!
//! `LlmClient::complete` returns a `(text, CompletionMetadata)` tuple.
//! Long-running pipelines (Mapper, Recovery, Cost Optimizer) accumulate
//! these into a `Vec<CompletionMetadata>`. At end-of-run, the operator-
//! facing summary calls `aggregate(&metas, &tier_costs)` and prints the
//! result via `CostSummary::format_human()`.
//!
//! Stage 1 surfaces `Token cost: $0.00 (0 LLM calls)` because the migrate
//! happy path doesn't yet make real LLM calls. The infrastructure is in
//! place for S4-close, when Mapper's real LLM call gets wired.

use crate::metadata::CompletionMetadata;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Per-tier USD cost rates. Loaded from
/// `[profiles.<name>.tier_costs.<tier>]` in the operator's profile.toml.
///
/// Defaulting to zero is intentional: an absent or partial config means
/// "we don't have the rate; surface tokens but report $0.00 cost." This
/// is preferable to crashing the migration just because the profile
/// hasn't been updated with current model pricing.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct TierCosts {
    /// USD per million input tokens. e.g., 0.25 for Haiku 4.5 input.
    #[serde(default)]
    pub input_usd_per_mtok: f64,
    /// USD per million output tokens. e.g., 1.25 for Haiku 4.5 output.
    #[serde(default)]
    pub output_usd_per_mtok: f64,
}

/// Output of `aggregate()`. The operator-facing summary calls
/// `format_human()`; the audit log + eval framework can also key off the
/// raw fields directly.
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    pub total_usd: f64,
    /// Per-tier breakdown: tier name → (USD subtotal, call count). Tier
    /// names with no cost-table entry still appear here (with USD = 0)
    /// so the gap is visible in operator output (Article IV).
    pub by_tier: BTreeMap<String, (f64, usize)>,
    pub call_count: usize,
}

impl CostSummary {
    /// One-line operator-facing summary.
    ///
    /// Empty case: `"Token cost: $0.00 (0 LLM calls)"`.
    /// Non-empty:  `"Token cost: $0.0009 (eco: $0.0009 [1 calls], smart: $0.00 [0 calls])"`.
    pub fn format_human(&self) -> String {
        if self.call_count == 0 {
            return "Token cost: $0.00 (0 LLM calls)".to_string();
        }
        let parts: Vec<String> = self
            .by_tier
            .iter()
            .map(|(tier, (usd, n))| format!("{tier}: ${usd:.4} [{n} calls]"))
            .collect();
        format!("Token cost: ${:.4} ({})", self.total_usd, parts.join(", "))
    }
}

/// Sum per-tier USD totals + call counts across `metas`. Tier names not
/// present in `costs_by_tier` contribute zero USD but still appear in
/// `result.by_tier` with their call count. That preserves the Article IV
/// invariant: gaps surface, never silently drop.
pub fn aggregate(
    metas: &[CompletionMetadata],
    costs_by_tier: &BTreeMap<String, TierCosts>,
) -> CostSummary {
    let mut result = CostSummary {
        total_usd: 0.0,
        by_tier: BTreeMap::new(),
        call_count: metas.len(),
    };

    for meta in metas {
        let tier = meta.tier.clone();
        let cost_default = TierCosts::default();
        let cost = costs_by_tier.get(&tier).unwrap_or(&cost_default);

        let usd = (meta.input_tokens as f64 / 1_000_000.0) * cost.input_usd_per_mtok
            + (meta.output_tokens as f64 / 1_000_000.0) * cost.output_usd_per_mtok;

        let entry = result.by_tier.entry(tier).or_insert((0.0, 0));
        entry.0 += usd;
        entry.1 += 1;
        result.total_usd += usd;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tier::Tier;

    fn make_meta(tier: Tier, input: u32, output: u32) -> CompletionMetadata {
        CompletionMetadata {
            provider: "test".to_string(),
            model_id: "test-model".to_string(),
            provider_endpoint: "https://example.invalid".to_string(),
            tier: tier.as_str().to_string(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd_micros: 0,
            latency_ms: 0,
        }
    }

    #[test]
    fn sc002_empty_metas_format_to_zero_calls_line() {
        let summary = aggregate(&[], &BTreeMap::new());
        assert_eq!(summary.format_human(), "Token cost: $0.00 (0 LLM calls)");
        assert_eq!(summary.total_usd, 0.0);
    }

    #[test]
    fn sc001_one_eco_call_with_rates_yields_expected_total() {
        let mut costs = BTreeMap::new();
        costs.insert(
            "eco".to_string(),
            TierCosts {
                input_usd_per_mtok: 0.25,
                output_usd_per_mtok: 1.25,
            },
        );
        // 1000 input * 0.25/M = 0.00025; 500 output * 1.25/M = 0.000625; total 0.000875.
        let metas = vec![make_meta(Tier::Eco, 1000, 500)];
        let summary = aggregate(&metas, &costs);
        assert!((summary.total_usd - 0.000875).abs() < 1e-9);
        assert_eq!(summary.call_count, 1);
        let (eco_usd, eco_n) = summary.by_tier.get("eco").copied().expect("eco present");
        assert!((eco_usd - 0.000875).abs() < 1e-9);
        assert_eq!(eco_n, 1);
    }

    #[test]
    fn sc003_two_metas_same_tier_sum() {
        let mut costs = BTreeMap::new();
        costs.insert(
            "eco".to_string(),
            TierCosts {
                input_usd_per_mtok: 1.0,
                output_usd_per_mtok: 2.0,
            },
        );
        let metas = vec![
            make_meta(Tier::Eco, 500_000, 100_000), // 0.5 + 0.2 = 0.7
            make_meta(Tier::Eco, 500_000, 100_000), // 0.5 + 0.2 = 0.7
        ];
        let summary = aggregate(&metas, &costs);
        assert!((summary.total_usd - 1.4).abs() < 1e-9);
        let (_, n) = summary.by_tier["eco"];
        assert_eq!(n, 2);
        assert_eq!(summary.call_count, 2);
    }

    #[test]
    fn sc004_unknown_tier_contributes_zero_but_surfaces_in_breakdown() {
        // Cost table has `eco` but not the tier the meta carries.
        let mut costs = BTreeMap::new();
        costs.insert(
            "eco".to_string(),
            TierCosts {
                input_usd_per_mtok: 1.0,
                output_usd_per_mtok: 1.0,
            },
        );
        let metas = vec![make_meta(Tier::Smart, 1000, 1000)];
        let summary = aggregate(&metas, &costs);
        assert_eq!(summary.total_usd, 0.0); // no rate → no USD
        assert_eq!(summary.call_count, 1);
        // But the call count IS recorded under "smart" — gap is visible.
        let (smart_usd, smart_n) = summary
            .by_tier
            .get("smart")
            .copied()
            .expect("smart present");
        assert_eq!(smart_usd, 0.0);
        assert_eq!(smart_n, 1);
    }

    #[test]
    fn format_human_renders_breakdown_when_calls_present() {
        let mut costs = BTreeMap::new();
        costs.insert(
            "eco".to_string(),
            TierCosts {
                input_usd_per_mtok: 0.25,
                output_usd_per_mtok: 1.25,
            },
        );
        let metas = vec![make_meta(Tier::Eco, 1000, 500)];
        let summary = aggregate(&metas, &costs);
        let s = summary.format_human();
        assert!(s.starts_with("Token cost: $"));
        assert!(s.contains("eco:"));
        assert!(s.contains("1 calls"));
    }
}
