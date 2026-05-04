// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Eval framework integration tests.
//!
//! Drives the suite at `terrashift-evals/` against the Generator from
//! P-08. The bootstrap helper (gated behind `BOOTSTRAP_GOLDENS=1`)
//! captures the actual Generator output into each golden's `expected/`
//! directory — used once per fixture to bless the output, then the real
//! tests assert byte-equality.

use std::path::{Path, PathBuf};
use tempfile::TempDir;
use terrashift_engine::generator::Generator;
use terrashift_eval::{discover_suite, load_golden, EvalRunner};

/// Resolve the workspace's `terrashift-evals/` directory from the test's
/// CARGO_MANIFEST_DIR (`libs/eval/`). Robust to where the tests are run
/// from (cargo, IDE, CI).
fn suite_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|workspace| workspace.join("terrashift-evals"))
        .unwrap_or_else(|| PathBuf::from("terrashift-evals"))
}

// ─────────────────────────────────────────────────────────────────────────
// Test 1 — discover_suite finds all 13 golden migrations.
// S3b: 3 AWS-as-target. S7 prefetch: +2 Azure-as-target. S7 close: +5
// more (suite to 10/10). R2: +1 fixture covering the Stage 1 P-16 #1
// demo scenario (15-resource GCP→AWS stack). S14: +2 Stage-2 patterns
// (aws_iam_role JSON-string emission, gcp_storage_bucket_lifecycle).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn discover_suite_finds_all_goldens() {
    let goldens = discover_suite(&suite_root()).unwrap();
    assert_eq!(
        goldens.len(),
        13,
        "Stage 2 ships 13 hand-curated goldens; suite_root = {:?}",
        suite_root()
    );

    let names: Vec<&str> = goldens.iter().map(|g| g.manifest.name.as_str()).collect();
    for expected in [
        "001_aws_vpc_minimal",
        "002_aws_subnet_with_reference",
        "003_aws_s3_bucket",
        "004_azurerm_vnet_minimal",
        "005_azurerm_storage_account",
        "006_aws_security_group",
        "007_aws_instance",
        "008_azurerm_subnet",
        "009_azurerm_nsg",
        "010_azurerm_linux_vm",
        "011_aws_iam_role",
        "012_gcp_storage_bucket_lifecycle",
        "015_aws_full_stack",
    ] {
        assert!(
            names.contains(&expected),
            "missing golden {expected}; got {:?}",
            names
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 2 — manifest TOML parses correctly (criterion #4).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn manifest_toml_parses_correctly() {
    let golden = load_golden(&suite_root().join("001_aws_vpc_minimal")).unwrap();
    assert_eq!(golden.manifest.name, "001_aws_vpc_minimal");
    assert_eq!(golden.manifest.source_provider, "google");
    assert_eq!(golden.manifest.target_provider, "aws");
    assert_eq!(golden.manifest.token_cost_ceiling_micros, 0);
    assert_eq!(golden.manifest.articles, vec![3, 6, 9]);
}

// ─────────────────────────────────────────────────────────────────────────
// Test 3 — All 3 goldens pass with the Stage 1 Generator (criterion #2).
// (Skipped automatically if `expected/` is empty; bootstrap test below
//  populates it.)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn all_thirteen_goldens_pass() {
    let runner = EvalRunner::new();
    let report = runner.run_suite(&suite_root()).unwrap();
    assert_eq!(report.total, 13);
    assert!(
        report.all_passed(),
        "{} of {} goldens failed:\n{}",
        report.failed,
        report.total,
        report
            .results
            .iter()
            .filter(|r| !r.passed)
            .map(|r| format!(
                "=== {} ===\n{}",
                r.name,
                r.diff.as_deref().unwrap_or("(no diff)")
            ))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
    assert_eq!(report.passed, 13);
    assert_eq!(report.failed, 0);
    assert_eq!(
        report.total_token_cost_micros, 0,
        "Stage 1 still 0 token cost — Mapper bypassed by P-12 design (goldens pre-curate mapping_plan.json)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 4 — A mutated expected/ produces passed=false with a diff
// (criterion #3).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn mutated_expected_produces_diff() {
    // Set up a temporary golden with deliberately-wrong expected content.
    let tmp_root = TempDir::new().unwrap();
    let fixture = tmp_root.path().join("999_mutated");
    std::fs::create_dir_all(&fixture).unwrap();
    std::fs::write(
        fixture.join("manifest.toml"),
        r#"name = "999_mutated"
source_provider = "google"
target_provider = "aws"
"#,
    )
    .unwrap();
    std::fs::write(fixture.join("source.tf"), "# placeholder\n").unwrap();
    std::fs::write(
        fixture.join("mapping_plan.json"),
        r#"{
  "run_id": "00000000-0000-0000-0000-000000000999",
  "source_provider": "google",
  "target_provider": "aws",
  "resources": [
    {
      "source_addr": "x.y",
      "target_addr": "aws_vpc.mutated",
      "target_type": "aws_vpc",
      "target_name": "mutated",
      "attributes": { "cidr_block": { "string": "10.99.0.0/16" } },
      "dependencies": []
    }
  ]
}
"#,
    )
    .unwrap();
    let expected_dir = fixture.join("expected");
    std::fs::create_dir_all(&expected_dir).unwrap();
    std::fs::write(
        expected_dir.join("aws_vpc.tf"),
        "this is wrong on purpose\n",
    )
    .unwrap();

    let golden = load_golden(&fixture).unwrap();
    let runner = EvalRunner::new();
    let result = runner.run(&golden).unwrap();
    assert!(!result.passed, "mutated expected must fail");
    assert!(result.diff.is_some(), "diff should be populated");
    assert!(
        result.diff.as_deref().unwrap().contains("aws_vpc.tf"),
        "diff names the differing file: {:?}",
        result.diff
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 5 — Determinism cross-check: running the suite twice produces
// equivalent results modulo wall_clock_ms (criterion #5).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn suite_is_deterministic_across_runs() {
    let runner = EvalRunner::new();
    let r1 = runner.run_suite(&suite_root()).unwrap();
    let r2 = runner.run_suite(&suite_root()).unwrap();

    assert_eq!(r1.total, r2.total);
    assert_eq!(r1.passed, r2.passed);
    assert_eq!(r1.failed, r2.failed);

    for (a, b) in r1.results.iter().zip(r2.results.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.passed, b.passed);
        assert_eq!(a.diff, b.diff);
        assert_eq!(a.token_cost_micros, b.token_cost_micros);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 6 — AttributeValue JSON round-trip (the externally-tagged form
// consumed by mapping_plan.json fixtures works both ways).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn attribute_value_json_round_trips() {
    use terrashift_engine::mapper::AttributeValue;

    let cases = vec![
        AttributeValue::String("10.0.0.0/16".to_string()),
        AttributeValue::Bool(true),
        AttributeValue::Number(42.0),
        AttributeValue::Reference("aws_vpc.main.id".to_string()),
    ];
    for original in cases {
        let json = serde_json::to_string(&original).unwrap();
        let back: AttributeValue = serde_json::from_str(&json).unwrap();
        assert_eq!(
            original, back,
            "round-trip failed for {:?}; json was {}",
            original, json
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Bootstrap helper — `#[ignore]`d so it doesn't show up as a passing
// test in normal runs. Invoke explicitly:
//
//     $env:BOOTSTRAP_GOLDENS=1; cargo test bootstrap_goldens -- --ignored
//
// Captures the Generator's actual output into each golden's `expected/`
// directory. After the bootstrap run, inspect captured files (and the
// `git diff`!) before committing.
//
// Article IX consideration: this DESTRUCTIVELY wipes existing
// `expected/` content — only run when fixture-authoring; never on a
// branch where you'd lose unrelated `expected/` mutations.
// ─────────────────────────────────────────────────────────────────────────
#[test]
#[ignore = "fixture-authoring tool; run with BOOTSTRAP_GOLDENS=1 cargo test bootstrap_goldens -- --ignored"]
fn bootstrap_goldens() {
    if std::env::var("BOOTSTRAP_GOLDENS").is_err() {
        panic!(
            "bootstrap_goldens requires BOOTSTRAP_GOLDENS=1 in addition to --ignored. \
             Run: $env:BOOTSTRAP_GOLDENS=1; cargo test bootstrap_goldens -- --ignored"
        );
    }

    let goldens = discover_suite(&suite_root()).unwrap();
    let generator = Generator::new();

    for golden in &goldens {
        eprintln!(
            "[bootstrap] WIPING expected/ for fixture '{}' (run `git diff` to see what changed)",
            golden.manifest.name
        );
        let cwd = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();
        generator
            .generate(cwd.path(), output_dir.path(), &golden.mapping_plan)
            .unwrap_or_else(|e| panic!("generator failed for {}: {}", golden.manifest.name, e));

        if golden.expected_target_dir.exists() {
            std::fs::remove_dir_all(&golden.expected_target_dir).unwrap();
        }
        std::fs::create_dir_all(&golden.expected_target_dir).unwrap();

        for entry in std::fs::read_dir(output_dir.path()).unwrap().flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("tf") {
                let dst = golden.expected_target_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dst).unwrap();
                eprintln!(
                    "[bootstrap] wrote {} for golden {}",
                    dst.display(),
                    golden.manifest.name
                );
            }
        }
    }
}
