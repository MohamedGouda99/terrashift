// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! R6 — Article XII rule 4 baseline + CI regression gate tests.
//!
//! All offline. The baseline file at `terrashift-evals/eval-baseline.json`
//! is the canonical Stage 1 snapshot (all-zero, no LLM yet).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use terrashift_eval::{
    compare_against_baseline, default_baseline_path, Baseline, BaselineMode, EvalResult,
    EvalRunner, FixtureBaseline, RegressionVerdict, SuiteReport,
};

// ─────────────────────────────────────────────────────────────────
// Test fixtures
// ─────────────────────────────────────────────────────────────────

fn suite_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|workspace| workspace.join("terrashift-evals"))
        .unwrap_or_else(|| PathBuf::from("terrashift-evals"))
}

fn make_report(per_fixture: &[(&str, u64)]) -> SuiteReport {
    let results: Vec<EvalResult> = per_fixture
        .iter()
        .map(|(name, cost)| EvalResult {
            name: (*name).to_string(),
            passed: true,
            token_cost_micros: *cost,
            wall_clock_ms: 1,
            diff: None,
        })
        .collect();
    let total: u64 = results.iter().map(|r| r.token_cost_micros).sum();
    let total_results = results.len();
    SuiteReport {
        results,
        total: total_results,
        passed: total_results,
        failed: 0,
        total_token_cost_micros: total,
    }
}

fn make_baseline(per_fixture: &[(&str, u64)]) -> Baseline {
    let mut bl_map = std::collections::BTreeMap::new();
    for (name, cost) in per_fixture {
        bl_map.insert(
            (*name).to_string(),
            FixtureBaseline {
                token_cost_micros: *cost,
            },
        );
    }
    let total: u64 = per_fixture.iter().map(|(_, c)| c).sum();
    Baseline {
        total_token_cost_micros: total,
        per_fixture: bl_map,
        last_refreshed_iso: "2026-05-02T00:00:00Z".to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────
// US1 — Regression detection
// ─────────────────────────────────────────────────────────────────

#[test]
fn us1_scenario1_40pct_regression_fails_strict() {
    let baseline = make_baseline(&[("a", 1_000_000)]);
    let report = make_report(&[("a", 1_400_000)]);
    let r = compare_against_baseline(&report, &baseline, BaselineMode::Strict);
    assert_eq!(r.verdict, RegressionVerdict::Fail);
    assert!(
        (r.delta_pct - 40.0).abs() < 0.001,
        "delta_pct ≈ 40, got {}",
        r.delta_pct
    );
    assert_eq!(r.per_fixture.len(), 1);
    assert_eq!(r.per_fixture[0].name, "a");
}

#[test]
fn us1_scenario2_20pct_within_threshold_passes() {
    let baseline = make_baseline(&[("a", 1_000_000)]);
    let report = make_report(&[("a", 1_200_000)]);
    let r = compare_against_baseline(&report, &baseline, BaselineMode::Strict);
    assert_eq!(r.verdict, RegressionVerdict::Pass);
    assert!((r.delta_pct - 20.0).abs() < 0.001);
}

#[test]
fn us1_scenario3_zero_baseline_zero_actual_passes_in_any_mode() {
    let baseline = make_baseline(&[("a", 0)]);
    let report = make_report(&[("a", 0)]);
    for mode in [BaselineMode::Lenient, BaselineMode::Strict] {
        let r = compare_against_baseline(&report, &baseline, mode);
        assert_eq!(r.verdict, RegressionVerdict::Pass);
        assert!(!r.first_baseline_seed_recommended);
    }
}

#[test]
fn us1_scenario4_lenient_accepts_first_nonzero_seed() {
    let baseline = make_baseline(&[("a", 0)]);
    let report = make_report(&[("a", 5_000_000)]);
    let r = compare_against_baseline(&report, &baseline, BaselineMode::Lenient);
    assert_eq!(r.verdict, RegressionVerdict::Pass);
    assert!(r.first_baseline_seed_recommended);
}

#[test]
fn us1_scenario5_strict_refuses_first_nonzero_seed() {
    let baseline = make_baseline(&[("a", 0)]);
    let report = make_report(&[("a", 5_000_000)]);
    let r = compare_against_baseline(&report, &baseline, BaselineMode::Strict);
    assert_eq!(r.verdict, RegressionVerdict::Fail);
    assert!(!r.first_baseline_seed_recommended);
}

// ─────────────────────────────────────────────────────────────────
// US2 — Per-fixture diff
// ─────────────────────────────────────────────────────────────────

#[test]
fn us2_per_fixture_sorted_worst_first() {
    let baseline = make_baseline(&[("a", 100), ("b", 200), ("c", 300)]);
    let report = make_report(&[("a", 100), ("b", 200), ("c", 600)]);
    let r = compare_against_baseline(&report, &baseline, BaselineMode::Strict);
    // Verdict still based on total: (100+200+600) vs (100+200+300) = +50% > 30% → Fail
    assert_eq!(r.verdict, RegressionVerdict::Fail);
    // First (worst) fixture should be "c" with 100% delta
    assert_eq!(r.per_fixture[0].name, "c");
    assert!((r.per_fixture[0].delta_pct - 100.0).abs() < 0.001);
}

// ─────────────────────────────────────────────────────────────────
// US3 — Stage 1 baseline file ships clean
// ─────────────────────────────────────────────────────────────────

#[test]
fn us3_committed_baseline_loads_and_has_all_fixtures() {
    let path = default_baseline_path(&suite_root());
    let baseline = Baseline::load(&path).unwrap();
    assert_eq!(
        baseline.per_fixture.len(),
        16,
        "Stage 2 ships 16 goldens at all-zero baseline (10 from S7 close + 1 R2 15-resource fixture + 2 S14 Stage-2 patterns: aws_iam_role, gcp_storage_bucket_lifecycle + 3 bidirectional cross-cloud goldens: 016 azurerm→aws_vpc, 017 google→aws_s3, 018 google→azurerm_vnet)"
    );
    assert_eq!(
        baseline.total_token_cost_micros, 0,
        "Stage 1 has no LLM calls — every fixture's cost is 0"
    );
    // Verify every cost is 0
    for (name, fb) in &baseline.per_fixture {
        assert_eq!(
            fb.token_cost_micros, 0,
            "fixture {name} should have cost 0 in Stage 1"
        );
    }
}

#[test]
fn us3_running_suite_against_stage1_baseline_passes() {
    // Full integration: run the actual eval suite + load the actual
    // baseline + assert the gate passes. This is what the CI step does.
    let runner = EvalRunner::new();
    let report = runner.run_suite(&suite_root()).unwrap();
    assert!(report.all_passed());

    let baseline = Baseline::load(&default_baseline_path(&suite_root())).unwrap();
    let regression = compare_against_baseline(&report, &baseline, BaselineMode::Lenient);
    assert_eq!(
        regression.verdict,
        RegressionVerdict::Pass,
        "Stage 1 baseline + Stage 1 report (both 0 cost) must pass the gate"
    );
    assert_eq!(regression.coverage_drift.len(), 0);
}

// ─────────────────────────────────────────────────────────────────
// Edge case — coverage drift surfaces baseline/report mismatches
// ─────────────────────────────────────────────────────────────────

#[test]
fn coverage_drift_flags_missing_fixtures() {
    let baseline = make_baseline(&[("a", 0), ("b", 0), ("c", 0)]);
    let report = make_report(&[("a", 0), ("z", 0)]); // baseline has b, c missing; report has z extra
    let r = compare_against_baseline(&report, &baseline, BaselineMode::Lenient);
    // Verdict is Pass (all zeros), but coverage_drift is populated.
    assert_eq!(r.verdict, RegressionVerdict::Pass);
    assert!(r
        .coverage_drift
        .iter()
        .any(|s| s.contains("baseline-only: b")));
    assert!(r
        .coverage_drift
        .iter()
        .any(|s| s.contains("baseline-only: c")));
    assert!(r
        .coverage_drift
        .iter()
        .any(|s| s.contains("report-only: z")));
}

#[test]
fn baseline_from_report_round_trip() {
    let report = make_report(&[("a", 1_000), ("b", 2_000)]);
    let baseline = Baseline::from_report(&report, "2026-05-02T00:00:00Z");
    assert_eq!(baseline.total_token_cost_micros, 3_000);
    assert_eq!(baseline.per_fixture.len(), 2);
    assert_eq!(baseline.per_fixture["a"].token_cost_micros, 1_000);
}
