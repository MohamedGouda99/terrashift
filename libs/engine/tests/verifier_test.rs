// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Verifier integration tests.
//!
//! Pattern: schema-driven fixtures per `feedback_no_hardcoding_use_seed.md`.
//! Resource type names come from `libs/knowledge/seed/<provider>/<latest>/schema.json`
//! at test time, NOT embedded in test code. The terraform-show-JSON
//! representation is generated programmatically using those names so
//! that re-pinning the schema doesn't require touching tests.
//!
//! Spec: `specs/018-verifier-state-diff/spec.md` SC-001..SC-006 plus
//! the two clarify-derived edge cases (Q6 provider-mismatch, Q7
//! unsupported-version).
//!
//! Constitution: Article III (output-side AI-safety gate), Article IV
//! (every drift finding is loud + named), Article VI (no hardcoded
//! resource types — derived from seed at test time).

use std::collections::BTreeMap;
use uuid::Uuid;

use terrashift_engine::mapper::{MappedResource, MappingPlan};
use terrashift_engine::verifier::{Verifier, VerifierError, VerifierIssue, VerifierWarning};

mod common;
use common::seed_fixtures::provider_resource_types_with_required;

// ─────────────────────────────────────────────────────────────────────────
// Test-fixture builders
// ─────────────────────────────────────────────────────────────────────────

const TEST_PROVIDER: &str = "aws";
const TEST_PROVIDER_URI: &str = "registry.terraform.io/hashicorp/aws";

/// Pick the first N resource types from the seeded `aws` schema that have
/// at least one required attribute. Stable order (alphabetic) — re-runs
/// pick the same N as long as the seed doesn't change underneath.
fn pick_n_resource_types(n: usize) -> Vec<String> {
    let all = provider_resource_types_with_required(TEST_PROVIDER);
    assert!(
        all.len() >= n,
        "seeded provider '{TEST_PROVIDER}' has fewer than {n} resources with required attrs ({})",
        all.len()
    );
    all.into_iter().take(n).collect()
}

/// Build a `MappingPlan` with one `MappedResource` per `(type, name)` pair.
/// `attributes` is empty — Verifier doesn't look at attributes, only addresses
/// and types.
fn plan_with(types_and_names: &[(&str, &str)]) -> MappingPlan {
    MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "google".to_string(), // arbitrary; Verifier doesn't read source
        target_provider: TEST_PROVIDER.to_string(),
        resources: types_and_names
            .iter()
            .map(|(rtype, name)| MappedResource {
                source_addr: format!("source.{name}"),
                target_addr: format!("{rtype}.{name}"),
                target_type: (*rtype).to_string(),
                target_name: (*name).to_string(),
                attributes: BTreeMap::new(),
                dependencies: vec![],
            })
            .collect(),
    }
}

/// Render a minimal terraform-show-JSON string with the given `(type, name)`
/// resources. Mirrors the shape Terraform 1.10+ emits.
fn state_json(resources: &[(&str, &str)]) -> String {
    let resources_json: Vec<String> = resources
        .iter()
        .map(|(rtype, name)| {
            format!(
                r#"{{ "address": "{rtype}.{name}", "mode": "managed", "type": "{rtype}", "name": "{name}", "provider_name": "{TEST_PROVIDER_URI}" }}"#
            )
        })
        .collect();
    format!(
        r#"{{
            "format_version": "1.0",
            "values": {{
                "root_module": {{
                    "resources": [{}]
                }}
            }}
        }}"#,
        resources_json.join(",")
    )
}

// ─────────────────────────────────────────────────────────────────────────
// SC-001: passed=true when plan and state match exactly.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn sc001_4_resources_matching_state_passes() {
    let types = pick_n_resource_types(4);
    let pairs: Vec<(&str, &str)> = types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            // Static names per index keep the assertion deterministic.
            (t.as_str(), ["alpha", "bravo", "charlie", "delta"][i])
        })
        .collect();

    let plan = plan_with(&pairs);
    let json = state_json(&pairs);

    let report = Verifier::new()
        .verify(&plan, &json)
        .expect("verify should succeed on matching plan/state");

    assert!(report.passed, "expected passed=true; got {:?}", report);
    assert_eq!(report.errors.len(), 0);
    assert_eq!(report.warnings.len(), 0);
    assert_eq!(report.expected_count, 4);
    assert_eq!(report.actual_count, 4);
}

// ─────────────────────────────────────────────────────────────────────────
// SC-002: MissingResource error when state is short by one resource.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn sc002_4_expected_3_actual_reports_one_missing() {
    let types = pick_n_resource_types(4);
    let plan_pairs: Vec<(&str, &str)> = types
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), ["a", "b", "c", "d"][i]))
        .collect();
    let state_pairs = &plan_pairs[..3]; // drop the last

    let plan = plan_with(&plan_pairs);
    let json = state_json(state_pairs);

    let report = Verifier::new().verify(&plan, &json).unwrap();
    assert!(!report.passed);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.warnings.len(), 0);
    match &report.errors[0] {
        VerifierIssue::MissingResource {
            address,
            resource_type,
        } => {
            assert_eq!(address, &format!("{}.{}", types[3], "d"));
            assert_eq!(resource_type, &types[3]);
        }
        other => panic!("expected MissingResource; got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// SC-003: ExtraResource warning when state has more than plan.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn sc003_3_expected_4_actual_warns_one_extra() {
    let types = pick_n_resource_types(4);
    let plan_pairs: Vec<(&str, &str)> = types
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, t)| (t.as_str(), ["a", "b", "c"][i]))
        .collect();
    let state_pairs: Vec<(&str, &str)> = types
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), ["a", "b", "c", "d"][i]))
        .collect();

    let plan = plan_with(&plan_pairs);
    let json = state_json(&state_pairs);

    let report = Verifier::new().verify(&plan, &json).unwrap();
    assert!(report.passed); // warnings don't flip passed
    assert_eq!(report.errors.len(), 0);
    assert_eq!(report.warnings.len(), 1);
    match &report.warnings[0] {
        VerifierWarning::ExtraResource {
            address,
            resource_type,
        } => {
            assert_eq!(address, &format!("{}.{}", types[3], "d"));
            assert_eq!(resource_type, &types[3]);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// SC-004: TypeMismatch error when same address has different type in state.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn sc004_same_address_different_type_reports_mismatch() {
    let types = pick_n_resource_types(4);
    // Plan has type[0] at "alpha"; state has type[1] at "alpha".
    let plan_pairs = vec![(types[0].as_str(), "alpha")];
    let state_pairs = vec![(types[1].as_str(), "alpha")];

    let plan = plan_with(&plan_pairs);
    let json = state_json(&state_pairs);

    let report = Verifier::new().verify(&plan, &json).unwrap();
    // Different addresses overall (type[0].alpha vs type[1].alpha) → both
    // a missing-from-state and an extra-in-state. The Verifier reports
    // one MissingResource + one ExtraResource, not a TypeMismatch, because
    // the addresses differ. To force TypeMismatch we have to keep the
    // address identical, which means same resource_type prefix. That's
    // not achievable across two different schema-derived types — so
    // we synthesize the state JSON with a deliberately-swapped type while
    // keeping the address aligned with the plan.
    drop(report); // discard; this case isn't TypeMismatch-shaped after all.

    // Build a state where address says "type[0].alpha" but the type field
    // is type[1] — simulating a hand-edited state file.
    let json_swapped = format!(
        r#"{{
            "format_version": "1.0",
            "values": {{
                "root_module": {{
                    "resources": [
                        {{ "address": "{}.alpha", "mode": "managed", "type": "{}", "name": "alpha", "provider_name": "{}" }}
                    ]
                }}
            }}
        }}"#,
        types[0], types[1], TEST_PROVIDER_URI
    );
    let report = Verifier::new().verify(&plan, &json_swapped).unwrap();
    assert!(!report.passed);
    assert_eq!(
        report.errors.len(),
        1,
        "expected exactly one TypeMismatch error; got {:?}",
        report.errors
    );
    match &report.errors[0] {
        VerifierIssue::TypeMismatch {
            address,
            expected_type,
            actual_type,
        } => {
            assert_eq!(address, &format!("{}.alpha", types[0]));
            assert_eq!(expected_type, &types[0]);
            assert_eq!(actual_type, &types[1]);
        }
        other => panic!("expected TypeMismatch; got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// SC-005: garbage non-JSON input returns ParseFailed error.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn sc005_garbage_input_returns_parse_failed() {
    let plan = plan_with(&[]);
    let result = Verifier::new().verify(&plan, "{not json");
    match result {
        Err(VerifierError::ParseFailed(_)) => (),
        other => panic!("expected ParseFailed; got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Q6 (clarify): provider mismatch — plan targets aws, state contains azurerm.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn q6_provider_mismatch_returns_typed_error() {
    // Pick from azurerm — note this IS the wrong provider for the plan
    // which always uses aws (TEST_PROVIDER above).
    let azurerm_types = provider_resource_types_with_required("azurerm");
    let azurerm_first = azurerm_types
        .first()
        .expect("seeded azurerm should have ≥1 required-attr resource");
    let plan = plan_with(&[(azurerm_first.as_str(), "x")]); // plan claims aws target
    let azurerm_uri = "registry.terraform.io/hashicorp/azurerm";
    let json = format!(
        r#"{{
            "format_version": "1.0",
            "values": {{
                "root_module": {{
                    "resources": [
                        {{ "address": "{azurerm_first}.x", "mode": "managed", "type": "{azurerm_first}", "name": "x", "provider_name": "{azurerm_uri}" }}
                    ]
                }}
            }}
        }}"#
    );

    let result = Verifier::new().verify(&plan, &json);
    match result {
        Err(VerifierError::ProviderMismatch { expected, actual }) => {
            assert_eq!(expected, "aws");
            assert_eq!(actual, "azurerm");
        }
        other => panic!("expected ProviderMismatch; got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Q7 (clarify): unsupported format_version → typed error, no comparison run.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn q7_unsupported_format_version_returns_typed_error() {
    let plan = plan_with(&[]);
    let json = r#"{"format_version": "0.12", "values": {"root_module": {"resources": []}}}"#;
    let result = Verifier::new().verify(&plan, json);
    match result {
        Err(VerifierError::UnsupportedFormatVersion(v)) => {
            assert_eq!(v, "0.12");
        }
        other => panic!("expected UnsupportedFormatVersion; got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Edge case: empty state (no resources yet — e.g., right after `terraform init`).
// Verifier reports all plan resources as Missing, no warnings.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn empty_state_after_init_reports_all_plan_resources_as_missing() {
    let types = pick_n_resource_types(2);
    let plan_pairs: Vec<(&str, &str)> = types
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), ["a", "b"][i]))
        .collect();
    let plan = plan_with(&plan_pairs);
    // `values: null` is the shape after `terraform init` but before any apply.
    let json = r#"{"format_version": "1.0", "values": null}"#;

    let report = Verifier::new().verify(&plan, json).unwrap();
    assert!(!report.passed);
    assert_eq!(report.errors.len(), 2);
    assert_eq!(report.warnings.len(), 0);
    assert_eq!(report.actual_count, 0);
    for err in &report.errors {
        assert!(matches!(err, VerifierIssue::MissingResource { .. }));
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Edge case: data-source-only state (Stage 1 ignores data sources entirely).
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn data_sources_in_state_dont_count_as_extras() {
    let types = pick_n_resource_types(1);
    let plan = plan_with(&[(types[0].as_str(), "x")]);

    // State has the plan's managed resource AND a data source the plan
    // doesn't know about. The data source must NOT trigger an
    // ExtraResource warning.
    let json = format!(
        r#"{{
            "format_version": "1.0",
            "values": {{
                "root_module": {{
                    "resources": [
                        {{ "address": "{}.x", "mode": "managed", "type": "{}", "name": "x", "provider_name": "{}" }},
                        {{ "address": "data.{}.runtime", "mode": "data", "type": "{}", "name": "runtime", "provider_name": "{}" }}
                    ]
                }}
            }}
        }}"#,
        types[0], types[0], TEST_PROVIDER_URI, types[0], types[0], TEST_PROVIDER_URI
    );

    let report = Verifier::new().verify(&plan, &json).unwrap();
    assert!(report.passed);
    assert_eq!(report.errors.len(), 0);
    assert_eq!(
        report.warnings.len(),
        0,
        "data sources must not count as extras; got {:?}",
        report.warnings
    );
}
