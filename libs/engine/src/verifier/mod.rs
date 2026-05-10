// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Verifier — post-apply state diff. Closes Stage 1 scope gap.
//!
//! Pattern: Terrashift-specific (the architecture reference document
//! has no direct counterpart — Stakpak's domain doesn't include
//! post-deployment state verification). Closest analog is §27 (single
//! redaction enforcement point), patterned in spirit: one place where
//! the post-apply invariant gets checked.
//!
//! Constitution: Article I (deterministic, not an agent — Verifier walks
//!               two sets and diffs them), Article III (output-side
//!               complement to Validator's input-side gate), Article IV
//!               (drift findings are loud, named, addressable),
//!               Article X (one tracing span per `verify()` call),
//!               Article XIII rule 3 (no unwrap/expect/string-slice).
//!
//! ## Public API
//!
//! ```ignore
//! use terrashift_engine::verifier::Verifier;
//! use terrashift_engine::mapper::MappingPlan;
//!
//! # fn doctest(plan: &MappingPlan, terraform_show_json: &str) {
//! let v = Verifier::new();
//! let report = v.verify(plan, terraform_show_json).unwrap();
//! if !report.passed {
//!     for err in &report.errors { eprintln!("BLOCK: {err}"); }
//!     std::process::exit(1);
//! }
//! for warn in &report.warnings { eprintln!("WARN: {warn}"); }
//! # }
//! ```
//!
//! ## Where Verifier sits in the pipeline
//!
//! `Generator → Executor → Verifier`. The migration is "complete" only
//! after Verifier returns `passed = true`. Stage 1 callers (the apply
//! CLI in PR #12, future `terrashift verify` subcommand) feed Verifier
//! the captured stdout of `terraform show -json`.

pub mod errors;
pub mod report;
mod state;

pub use errors::VerifierError;
pub use report::{VerifierIssue, VerifierReport, VerifierWarning};

use crate::mapper::MappingPlan;
use std::collections::BTreeMap;

/// Stateless post-apply gate.
///
/// Verifier holds no resources — it's a pure function `(plan, json) → report`.
/// Constructor exists for symmetry with `Validator::new(...)` and so the
/// type can grow state in Stage 2 without breaking call sites.
pub struct Verifier;

impl Verifier {
    pub fn new() -> Self {
        Self
    }

    /// Compare `plan`'s expected resources against `terraform_show_json`'s
    /// actual resources. Aggregate (every drift finding), not fail-fast.
    ///
    /// Returns `Err` only on infrastructure-level failures (parse error,
    /// unsupported state-file format, plan/state provider mismatch).
    /// Drift findings — missing, extra, type-mismatched — land in the
    /// `VerifierReport`.
    pub fn verify(
        &self,
        plan: &MappingPlan,
        terraform_show_json: &str,
    ) -> Result<VerifierReport, VerifierError> {
        let span = tracing::info_span!(
            "verifier::verify",
            run_id = %plan.run_id,
            target_provider = %plan.target_provider,
            expected_count = plan.resources.len(),
        );
        let _g = span.enter();

        let parsed: state::TerraformState = serde_json::from_str(terraform_show_json)?;

        if !parsed.format_version.starts_with("1.") {
            return Err(VerifierError::UnsupportedFormatVersion(
                parsed.format_version,
            ));
        }

        let actual = state::flatten_resources(&parsed);

        // Provider mismatch pre-flight: scan actual resources, find the
        // most common provider, compare against plan.target_provider.
        // Empty actual (zero resources) skips the check — degenerate
        // post-init state isn't a mismatch, just zero coverage.
        if let Some(actual_provider) = dominant_provider(&actual) {
            if actual_provider != plan.target_provider {
                return Err(VerifierError::ProviderMismatch {
                    expected: plan.target_provider.clone(),
                    actual: actual_provider.to_string(),
                });
            }
        }

        let expected_by_addr: BTreeMap<&str, &crate::mapper::MappedResource> = plan
            .resources
            .iter()
            .map(|r| (r.target_addr.as_str(), r))
            .collect();

        // Stage 1 only compares managed resources. Data sources read from
        // existing infrastructure (e.g., `data.aws_caller_identity.current`)
        // and don't represent migrated state. Per clarify.md Q3: full
        // `(mode, type, name)` tuple is the canonical match; Stage 1
        // restricts the comparison to mode == "managed" everywhere.
        let actual_by_addr: BTreeMap<&str, &state::StateResource> = actual
            .iter()
            .filter(|r| r.mode == "managed")
            .map(|r| (r.address.as_str(), *r))
            .collect();

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Missing: in plan, not in state.
        for (addr, mapped) in &expected_by_addr {
            if !actual_by_addr.contains_key(addr) {
                errors.push(VerifierIssue::MissingResource {
                    address: (*addr).to_string(),
                    resource_type: mapped.target_type.clone(),
                });
            }
        }

        // Extra: in state, not in plan.
        for (addr, sr) in &actual_by_addr {
            if !expected_by_addr.contains_key(addr) {
                warnings.push(VerifierWarning::ExtraResource {
                    address: (*addr).to_string(),
                    resource_type: sr.resource_type.clone(),
                });
            }
        }

        // Intersection: type-mismatch check.
        for (addr, mapped) in &expected_by_addr {
            if let Some(sr) = actual_by_addr.get(addr) {
                if mapped.target_type != sr.resource_type {
                    errors.push(VerifierIssue::TypeMismatch {
                        address: (*addr).to_string(),
                        expected_type: mapped.target_type.clone(),
                        actual_type: sr.resource_type.clone(),
                    });
                }
            }
        }

        let report = VerifierReport::new(
            plan.run_id,
            plan.resources.len(),
            actual.len(),
            errors,
            warnings,
        );

        tracing::info!(
            errors_count = report.errors.len(),
            warnings_count = report.warnings.len(),
            passed = report.passed,
            actual_count = report.actual_count,
            "verifier::verify complete"
        );

        Ok(report)
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Pick the dominant (most common) provider from a slice of state
/// resources. Returns `None` when the slice is empty. Used to detect
/// "operator pointed Verifier at the wrong state file".
fn dominant_provider<'a>(resources: &'a [&state::StateResource]) -> Option<&'a str> {
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for r in resources {
        *tally
            .entry(state::canonical_provider_name(&r.provider_name))
            .or_insert(0) += 1;
    }
    tally.into_iter().max_by_key(|(_, c)| *c).map(|(k, _)| k)
}
