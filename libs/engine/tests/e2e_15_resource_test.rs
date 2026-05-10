// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! 15-resource AWS→AzureRM E2E — closes Stage 1 MVP gate criterion #1.
//!
//! Spec: `specs/022-15-resource-fixture/spec.md`.
//! Fixture: `fixtures/e2e-aws-to-azurerm-15/`.
//! Constitution: Article III (Validator on the path), Article VI
//! (deterministic emission, schema-driven attrs), Article VIII (gate
//! criterion #1 closed by this test, not ceremony).
//!
//! Why deterministic (no LLM):
//!
//! Stage 1's Mapper LLM is gated on `HF_TOKEN` (see
//! `pratik_e2e_test.rs`). To make gate criterion #1 enforceable in CI
//! without external secrets, this test bypasses the LLM by hand-curating
//! the source→target type map (6 entries — a quick test-time table) and
//! deriving every required attribute from the bundled seed schemas at
//! `libs/knowledge/seed/azurerm/<latest>/schema.json` per the
//! no-hardcoding rule (`feedback_no_hardcoding_use_seed.md`).
//!
//! The pratik_e2e test continues to exercise the full LLM round-trip
//! against the smaller real-world fixture; this test exercises the
//! 15-resource scale.

use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

use terrashift_engine::generator::Generator;
use terrashift_engine::mapper::{AttributeValue, MappedResource, MappingPlan};
use terrashift_engine::scanner::Scanner;

const MIN_EMIT_RATIO: f64 = 12.0 / 15.0; // gate criterion #1 threshold

/// Source→target type mapping for the AWS-to-AzureRM Stage 1 path.
///
/// `None` on the right means "no Stage 1 azurerm equivalent registered"
/// — these resources are expected to skip in the migration summary's
/// "Skipped (gaps)" section, anchoring the ≥12/15 threshold.
fn aws_to_azurerm_mapping(aws_type: &str) -> Option<&'static str> {
    match aws_type {
        "aws_vpc" => Some("azurerm_virtual_network"),
        "aws_subnet" => Some("azurerm_subnet"),
        "aws_security_group" => Some("azurerm_network_security_group"),
        "aws_instance" => Some("azurerm_linux_virtual_machine"),
        "aws_s3_bucket" => Some("azurerm_storage_account"),
        // Stage 1: no azurerm_role_definition template registered.
        // Surfaces as Skipped (gap) — the gate threshold accounts for it.
        "aws_iam_role" => None,
        _ => None,
    }
}

/// Read minimal required attributes for a target type from the bundled
/// seed schema. Inline rather than depending on
/// `tests/common/seed_fixtures.rs` so this test binary is independent
/// of other branches' refactors.
fn schema_required_attrs(target_type: &str) -> Vec<(String, AttributeValue)> {
    let provider = target_type
        .split('_')
        .next()
        .unwrap_or_else(|| panic!("target_type '{target_type}' has no provider prefix"));

    // Resolve seed path via CARGO_MANIFEST_DIR (cross-platform).
    let seed_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("knowledge")
        .join("seed")
        .join(provider);
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&seed_root)
        .unwrap_or_else(|e| panic!("read {}: {e}", seed_root.display()))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    versions.sort();
    let path = versions
        .pop()
        .unwrap_or_else(|| panic!("no seeded versions for '{provider}'"))
        .join("schema.json");
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let schema: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

    let attrs = schema["resources"][target_type]["attributes"]
        .as_object()
        .unwrap_or_else(|| panic!("'{target_type}' not in {} schema", provider));

    let mut required: std::collections::BTreeSet<String> = attrs
        .iter()
        .filter(|(_, v)| v["required"].as_bool() == Some(true))
        .map(|(k, _)| k.clone())
        .collect();

    // Same template-policy overrides as `generator_test.rs` —
    // attributes the Generator template enforces beyond the upstream
    // schema's required set. Kept inline + small.
    let template_policy: &[&str] = match target_type {
        "azurerm_linux_virtual_machine" => &["size"],
        "azurerm_storage_account" => &["account_tier", "account_replication_type"],
        "azurerm_subnet" => &["virtual_network_name"],
        _ => &[],
    };
    for extra in template_policy {
        required.insert((*extra).to_string());
    }

    required
        .into_iter()
        .map(|k| {
            let attr_type = attrs
                .get(&k)
                .and_then(|v| v["attribute_type"].as_str())
                .unwrap_or("string");
            (k, synthesize_value(attr_type))
        })
        .collect()
}

fn synthesize_value(attr_type: &str) -> AttributeValue {
    use AttributeValue::*;
    if attr_type.starts_with("string") {
        String("placeholder".to_string())
    } else if attr_type.starts_with("bool") {
        Bool(false)
    } else if attr_type.starts_with("number") {
        Number(0.0)
    } else if attr_type.starts_with("list") || attr_type.starts_with("set") {
        List(vec![String("placeholder".to_string())])
    } else if attr_type.starts_with("map") {
        Map(BTreeMap::new())
    } else {
        // Fall back to string for nested-block-attr types we don't model.
        String("placeholder".to_string())
    }
}

#[test]
fn fixture_parses_to_15_source_resources() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("e2e-aws-to-azurerm-15");
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());

    let inv = Scanner::scan(&fixture).expect("scan fixture");
    let total: usize = inv.files.iter().map(|f| f.resources.len()).sum();
    assert_eq!(
        total, 15,
        "fixture must define exactly 15 resources for gate criterion #1; found {total}"
    );

    // Verify the type mix matches the README claim.
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for file in &inv.files {
        for r in &file.resources {
            *by_type.entry(r.resource_type.clone()).or_default() += 1;
        }
    }
    assert_eq!(by_type.get("aws_vpc"), Some(&1), "1 VPC");
    assert_eq!(by_type.get("aws_subnet"), Some(&3), "3 subnets");
    assert_eq!(by_type.get("aws_security_group"), Some(&3), "3 SGs");
    assert_eq!(by_type.get("aws_instance"), Some(&3), "3 instances");
    assert_eq!(by_type.get("aws_s3_bucket"), Some(&3), "3 S3 buckets");
    assert_eq!(by_type.get("aws_iam_role"), Some(&2), "2 IAM roles");
}

#[test]
fn deterministic_e2e_emits_at_least_12_of_15() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("e2e-aws-to-azurerm-15");

    // 1. Scan source.
    let inv = Scanner::scan(&fixture).expect("scan fixture");
    let total: usize = inv.files.iter().map(|f| f.resources.len()).sum();
    assert_eq!(total, 15);

    // 2. Build a hand-curated MappingPlan from the inventory + the
    //    static aws→azurerm type map. Only resources with a target
    //    type are added to the plan; the rest land in a "skipped" list.
    let mut mapped = Vec::new();
    let mut skipped = Vec::new();
    for file in &inv.files {
        for resource in &file.resources {
            let source_addr = format!("{}.{}", resource.resource_type, resource.name);
            match aws_to_azurerm_mapping(&resource.resource_type) {
                Some(target_type) => {
                    let target_name = resource.name.clone();
                    let target_addr = format!("{target_type}.{target_name}");
                    let attrs: BTreeMap<String, AttributeValue> =
                        schema_required_attrs(target_type).into_iter().collect();
                    mapped.push(MappedResource {
                        source_addr,
                        target_addr,
                        target_type: target_type.to_string(),
                        target_name,
                        attributes: attrs,
                        dependencies: vec![],
                    });
                }
                None => {
                    skipped.push(source_addr);
                }
            }
        }
    }

    // Sanity: the IAM roles should be the ones skipped.
    assert_eq!(
        skipped.len(),
        2,
        "expected 2 skipped resources (the IAM roles); got {}: {:?}",
        skipped.len(),
        skipped
    );

    let mapped_count = mapped.len();
    let plan = MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "aws".to_string(),
        target_provider: "azurerm".to_string(),
        resources: mapped,
    };

    // 3. Run Generator. Count how many resources successfully emit.
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();
    let g = Generator::new();
    let _ = g
        .generate(cwd.path(), output_dir.path(), &plan)
        .expect("generate");

    // The Generator deduplicates per target_type into one file per type,
    // so artifacts.files counts files (≤ resource count). We re-scan
    // the output to count resources actually emitted.
    let emitted_inv = Scanner::scan(output_dir.path()).expect("scan emitted");
    let emitted_count: usize = emitted_inv.files.iter().map(|f| f.resources.len()).sum();

    let ratio = emitted_count as f64 / 15.0;
    println!(
        "Gate criterion #1: emitted {emitted_count}/15 ({:.0}%) — threshold {:.0}%",
        ratio * 100.0,
        MIN_EMIT_RATIO * 100.0
    );
    println!(
        "  source resources : 15\n  mapped to target : {mapped_count}\n  skipped (gaps)   : {}",
        skipped.len()
    );

    assert!(
        ratio >= MIN_EMIT_RATIO,
        "Stage 1 MVP gate criterion #1 not met: {emitted_count}/15 emitted (need ≥{}/15)",
        (MIN_EMIT_RATIO * 15.0) as usize
    );
}
