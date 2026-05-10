// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Deterministic cost analyzer. Iterates a `MappingPlan`, looks up
//! source + target costs via the `CostLookup` trait, and produces a
//! `CostReport`. The agent layer (Phase 3) sits on top of this and adds
//! recommendations; today this layer just reports the deltas.
//!
//! `CostLookup` is a narrow seam over `InfracostClient` so tests can
//! inject a `StubLookup` and exercise the analyzer without network.

use crate::cache::CostCache;
use crate::errors::CostOptimizerError;
use crate::infracost::{InfracostClient, ResourceCostQuery, ResourceCostResult};
use crate::report::{CostReport, LineItem};
use async_trait::async_trait;
use std::collections::BTreeMap;
use terrashift_engine::mapper::MappingPlan;

/// Narrow seam: the analyzer only needs to ask "what's a resource cost?".
/// Real impl: InfracostClient (calls upstream API). Test impl: stub
/// returning canned values.
#[async_trait]
pub trait CostLookup: Send + Sync {
    async fn lookup(
        &self,
        query: &ResourceCostQuery,
    ) -> Result<ResourceCostResult, CostOptimizerError>;
}

#[async_trait]
impl CostLookup for InfracostClient {
    async fn lookup(
        &self,
        query: &ResourceCostQuery,
    ) -> Result<ResourceCostResult, CostOptimizerError> {
        InfracostClient::lookup(self, query).await
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzeConfig {
    pub source_region: String,
    pub target_region: String,
    /// If true, a single failed lookup aborts the whole analyze run.
    /// If false, the failure is recorded in `CostReport.skipped` and
    /// the analyzer continues.
    pub strict: bool,
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            source_region: "us-east-1".to_string(),
            target_region: "eastus".to_string(),
            strict: false,
        }
    }
}

pub struct CostOptimizer<L: CostLookup> {
    lookup: L,
    cache: Option<CostCache>,
}

impl<L: CostLookup> CostOptimizer<L> {
    pub fn new(lookup: L) -> Self {
        Self {
            lookup,
            cache: None,
        }
    }

    pub fn with_cache(mut self, cache: CostCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Walk the plan, look up source + target costs per resource,
    /// return a `CostReport` with line items + totals + skipped entries.
    /// Recommendations are left empty here; Phase 3's agent fills them.
    pub async fn analyze(
        &self,
        plan: &MappingPlan,
        config: &AnalyzeConfig,
    ) -> Result<CostReport, CostOptimizerError> {
        let mut report = CostReport::default();
        let mut stats: BTreeMap<String, u64> = BTreeMap::new();

        for resource in &plan.resources {
            // Source-side query: derive source type from the source addr's
            // `<type>.<name>` shape.
            let source_type = resource
                .source_addr
                .split('.')
                .next()
                .unwrap_or("unknown")
                .to_string();
            let source_query = ResourceCostQuery {
                provider: plan.source_provider.clone(),
                resource_type: source_type.clone(),
                region: config.source_region.clone(),
                attrs: serde_json::Value::Null, // Article V — no attribute leakage today
            };
            let target_query = ResourceCostQuery {
                provider: plan.target_provider.clone(),
                resource_type: resource.target_type.clone(),
                region: config.target_region.clone(),
                attrs: serde_json::Value::Null,
            };

            let source_cost = self.cached_or_lookup(&source_query, &mut stats).await;
            let target_cost = self.cached_or_lookup(&target_query, &mut stats).await;

            let (src_usd, tgt_usd) = match (source_cost, target_cost) {
                (Ok(s), Ok(t)) => (Some(s.usd_per_month), Some(t.usd_per_month)),
                (Err(e), _) | (_, Err(e)) if config.strict => return Err(e),
                (Err(e), _) => {
                    report
                        .skipped
                        .push((resource.target_addr.clone(), format!("source lookup: {e}")));
                    *stats.entry("skipped".to_string()).or_insert(0) += 1;
                    (None, None)
                }
                (Ok(s), Err(e)) => {
                    report
                        .skipped
                        .push((resource.target_addr.clone(), format!("target lookup: {e}")));
                    *stats.entry("skipped".to_string()).or_insert(0) += 1;
                    (Some(s.usd_per_month), None)
                }
            };

            if let Some(s) = src_usd {
                report.source_total_usd_per_month += s;
            }
            if let Some(t) = tgt_usd {
                report.target_total_usd_per_month += t;
            }

            report.line_items.push(LineItem {
                source_addr: resource.source_addr.clone(),
                target_addr: resource.target_addr.clone(),
                source_type,
                target_type: resource.target_type.clone(),
                region: config.target_region.clone(),
                source_usd_per_month: src_usd,
                target_usd_per_month: tgt_usd,
            });
        }

        report.stats = stats;
        Ok(report)
    }

    async fn cached_or_lookup(
        &self,
        query: &ResourceCostQuery,
        stats: &mut BTreeMap<String, u64>,
    ) -> Result<ResourceCostResult, CostOptimizerError> {
        let cache_key = format!(
            "{}|{}|{}",
            query.provider, query.resource_type, query.region
        );
        if let Some(c) = &self.cache {
            match c.get::<ResourceCostResult>(&cache_key) {
                Ok(Some(hit)) => {
                    *stats.entry("cache_hit".to_string()).or_insert(0) += 1;
                    return Ok(hit);
                }
                Ok(None) => {
                    *stats.entry("cache_miss".to_string()).or_insert(0) += 1;
                }
                Err(e) => return Err(CostOptimizerError::Cache(e)),
            }
        }
        let result = self.lookup.lookup(query).await?;
        if let Some(c) = &self.cache {
            c.set(&cache_key, &result)
                .map_err(CostOptimizerError::Cache)?;
        }
        *stats.entry("infracost_call".to_string()).or_insert(0) += 1;
        Ok(result)
    }
}
