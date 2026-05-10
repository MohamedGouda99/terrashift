// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cost Optimizer infrastructure.
//!
//! Phase 1: Infracost client + cache + errors.
//! Phase 2 (this commit): deterministic CostOptimizer::analyze.
//! Phase 3 (later): agent layer with 3 tools.
//!
//! Constitution:
//! - Article IV (loud — Infracost rate limit / network failures surface
//!   as typed CostOptimizerError, never silent default zero).
//! - Article V (no PII to Infracost — only provider, type, region; never
//!   attribute values; the analyzer enforces this at the query
//!   construction site).
//! - Article XII rule 4 (regression-gate plumbing — token cost will be
//!   tracked when the agent layer lands in Phase 3).

pub mod analyze;
pub mod cache;
pub mod errors;
pub mod infracost;
pub mod report;

pub use analyze::{AnalyzeConfig, CostLookup, CostOptimizer};
pub use cache::{CostCache, CostCacheError};
pub use errors::CostOptimizerError;
pub use infracost::{InfracostClient, ResourceCostQuery, ResourceCostResult};
pub use report::{CostReport, LineItem, Recommendation};
