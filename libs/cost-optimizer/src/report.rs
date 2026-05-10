// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cost-analysis output types. Shape that the CLI/TUI render and that
//! the agent (Phase 3) extends with `Recommendation` entries.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One source/target resource pair plus its monthly USD on each side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineItem {
    pub source_addr: String,
    pub target_addr: String,
    pub source_type: String,
    pub target_type: String,
    pub region: String,
    pub source_usd_per_month: Option<f64>,
    pub target_usd_per_month: Option<f64>,
}

impl LineItem {
    /// Positive when the target costs more than the source. None if either side
    /// failed to look up.
    pub fn delta_usd_per_month(&self) -> Option<f64> {
        match (self.source_usd_per_month, self.target_usd_per_month) {
            (Some(s), Some(t)) => Some(t - s),
            _ => None,
        }
    }

    /// Positive when target is more expensive. None if either lookup failed
    /// or source cost is zero (avoid div-by-zero, surface as None).
    pub fn delta_pct(&self) -> Option<f64> {
        match (self.source_usd_per_month, self.target_usd_per_month) {
            (Some(s), Some(t)) if s > 0.0 => Some((t - s) / s * 100.0),
            _ => None,
        }
    }
}

/// Phase 2: Recommendation is a placeholder with empty list. Phase 3
/// fills it via the agent's `recommend_swap` tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recommendation {
    pub resource_addr: String,
    pub current_attrs: serde_json::Value,
    pub proposed_attrs: serde_json::Value,
    pub savings_usd_per_month: f64,
    pub tradeoff: String,
}

/// Top-level output of `CostOptimizer::analyze`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostReport {
    pub line_items: Vec<LineItem>,
    pub recommendations: Vec<Recommendation>,
    /// Resources we couldn't price; (target_addr, reason).
    pub skipped: Vec<(String, String)>,
    pub source_total_usd_per_month: f64,
    pub target_total_usd_per_month: f64,
    /// Lookup-cost telemetry — useful for Phase 5 eval baseline.
    pub stats: BTreeMap<String, u64>,
}

impl CostReport {
    pub fn delta_usd_per_month(&self) -> f64 {
        self.target_total_usd_per_month - self.source_total_usd_per_month
    }

    pub fn delta_pct(&self) -> Option<f64> {
        if self.source_total_usd_per_month > 0.0 {
            Some(
                (self.target_total_usd_per_month - self.source_total_usd_per_month)
                    / self.source_total_usd_per_month
                    * 100.0,
            )
        } else {
            None
        }
    }
}
