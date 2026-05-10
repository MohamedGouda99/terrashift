// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cost Optimizer infrastructure (S11 Phase 1).
//!
//! This crate provides the foundation for the Cost Optimizer agent: a
//! cached, rate-limited Infracost client that tells callers what a given
//! `(provider, resource_type, region, attrs)` tuple costs per month.
//!
//! Phase 1 ships ONLY the client + cache. Phase 2 adds the
//! deterministic analyze() entry point. Phase 3 adds the agent layer
//! (three tools wrapping run_agent). Until Phase 2 lands, no production
//! caller invokes anything in this crate.
//!
//! Constitution:
//! - Article IV (loud — Infracost rate limit / network failures surface
//!   as typed CostOptimizerError, never silent default zero).
//! - Article V (no PII to Infracost — we only send provider, type,
//!   region, count, size; never tag values, bucket names, IPs).
//! - Article XII rule 4 (regression-gate plumbing — token cost will be
//!   tracked when the agent layer lands in Phase 3).

pub mod cache;
pub mod errors;
pub mod infracost;
