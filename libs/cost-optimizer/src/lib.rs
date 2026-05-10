// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cost Optimizer infrastructure.
//!
//! Phase 1: Infracost client + cache + errors.
//! Phase 2: deterministic CostOptimizer::analyze.
//! Phase 3 (this commit): agent layer with 3 tools wrapping the
//! deterministic analyzer; populates `CostReport::recommendations`.
//! Phase 4 (later): CLI wiring.
//! Phase 5 (later): eval framework integration.
//!
//! Constitution:
//! - Article I (Cost Optimizer is one of 3 named agents allowed without RFC,
//!   alongside Recovery and Cutover — see plan §5).
//! - Article III (cost-optimizer fires after Validator+Generator — never
//!   bypasses earlier stages; the agent only mutates `recommendations`,
//!   never the plan itself in this phase).
//! - Article IV (loud — Infracost rate limit / network failures + agent
//!   bail-out states surface as typed errors / exhaustive enum variants,
//!   never silent default zero).
//! - Article V (no PII to Infracost — only provider, type, region; never
//!   attribute values; the analyzer enforces this at the query
//!   construction site).
//! - Article XII rule 4 (regression-gate plumbing — token cost will be
//!   measured by Phase 5 eval; the agent layer's deterministic
//!   `delta_threshold_pct` gate keeps spend bounded).
//! - Article XIII rule 6 (recommend_swap is `Ask` in production — see
//!   `agent::CostAgentConfig` doc comment for the test-mode override).

pub mod agent;
pub mod analyze;
pub mod cache;
pub mod errors;
pub mod infracost;
pub mod report;

pub use agent::{CostAgentConfig, CostAgentOutcome, CostOptimizerAgent, CostOptimizerAgentError};
pub use analyze::{AnalyzeConfig, CostLookup, CostOptimizer};
pub use cache::{CostCache, CostCacheError};
pub use errors::CostOptimizerError;
pub use infracost::{InfracostClient, ResourceCostQuery, ResourceCostResult};
pub use report::{CostReport, LineItem, Recommendation};
