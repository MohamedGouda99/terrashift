// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Generator integration tests — round-trip, backup-first, rollback,
//! template-miss, determinism, references, audit metadata.
//!
//! Pattern: tests live under `libs/engine/tests/` per Cargo conventions.
//! Per spec.md success criteria — every numbered criterion has a test below.

use std::collections::BTreeMap;
use tempfile::TempDir;
use uuid::Uuid;

use terrashift_engine::generator::Generator;
use terrashift_engine::mapper::{AttributeValue, MappedResource, MappingPlan};
use terrashift_engine::scanner::Scanner;

// ─────────────────────────────────────────────────────────────────────────
// Fixture helpers
// ─────────────────────────────────────────────────────────────────────────

fn make_attr_string(s: &str) -> AttributeValue {
    AttributeValue::String(s.to_string())
}

fn make_attr_ref(s: &str) -> AttributeValue {
    AttributeValue::Reference(s.to_string())
}

fn aws_vpc_resource(name: &str, cidr: &str) -> MappedResource {
    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), make_attr_string(cidr));
    attrs.insert("instance_tenancy".to_string(), make_attr_string("default"));
    MappedResource {
        source_addr: format!("aws_vpc.{}", name),
        target_addr: format!("aws_vpc.{}", name),
        target_type: "aws_vpc".to_string(),
        target_name: name.to_string(),
        attributes: attrs,
        dependencies: vec![],
    }
}

fn aws_subnet_resource(name: &str, cidr: &str, vpc_ref: &str) -> MappedResource {
    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), make_attr_string(cidr));
    attrs.insert("vpc_id".to_string(), make_attr_ref(vpc_ref));
    MappedResource {
        source_addr: format!("aws_subnet.{}", name),
        target_addr: format!("aws_subnet.{}", name),
        target_type: "aws_subnet".to_string(),
        target_name: name.to_string(),
        attributes: attrs,
        dependencies: vec![],
    }
}

fn plan_with(resources: Vec<MappedResource>) -> MappingPlan {
    MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "google".to_string(),
        target_provider: "aws".to_string(),
        resources,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 1 — template registry sanity (Stage 2 ships 17 templates)
// Stage 1: 10 (5 AWS + 5 Azure for the demo paths). S14: +2 (aws_iam_role,
// google_storage_bucket). Pratik-e2e expansion: +5 (azurerm_route_table,
// azurerm_public_ip, azurerm_subnet_network_security_group_association,
// azurerm_subnet_route_table_association, azurerm_resource_group).
// GCP cross-cloud parity: +4 (google_compute_network/subnetwork/firewall/instance).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn template_count_is_21() {
    let g = Generator::new();
    assert_eq!(
        g.template_count(),
        21,
        "Stage 2 + GCP parity ships 21 templates (10 Stage-1 + 2 S14 + 5 Pratik-e2e + 4 GCP)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 2 — Round-trip: emit single VPC, re-parse with Scanner, verify
// (spec.md success criterion #1)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn emit_aws_vpc_round_trips_through_scanner() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let plan = plan_with(vec![aws_vpc_resource("main", "10.0.0.0/16")]);
    let g = Generator::new();
    let artifacts = g.generate(cwd.path(), output_dir.path(), &plan).unwrap();

    assert_eq!(artifacts.files.len(), 1);
    assert!(artifacts.files[0].ends_with("aws_vpc.tf"));

    // Now re-parse via the Scanner from P-04.
    let inventory = Scanner::scan(output_dir.path()).unwrap();
    assert_eq!(inventory.files.len(), 1, "exactly one .tf file");
    let file = &inventory.files[0];

    // Should contain exactly one resource block.
    assert_eq!(
        file.resources.len(),
        1,
        "round-trip preserves exactly one resource"
    );
    let r = &file.resources[0];
    assert_eq!(r.resource_type, "aws_vpc");
    assert_eq!(r.name, "main");
}

// ─────────────────────────────────────────────────────────────────────────
// Test 3 — Multi-resource MappingPlan writes multiple files grouped by type
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn multi_resource_plan_writes_one_file_per_type() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let plan = plan_with(vec![
        aws_vpc_resource("main", "10.0.0.0/16"),
        aws_vpc_resource("backup", "10.1.0.0/16"),
        aws_subnet_resource("a", "10.0.1.0/24", "aws_vpc.main.id"),
    ]);
    let g = Generator::new();
    let artifacts = g.generate(cwd.path(), output_dir.path(), &plan).unwrap();

    // Expect 2 files: aws_vpc.tf (with both VPCs) and aws_subnet.tf.
    assert_eq!(artifacts.files.len(), 2, "two target_types → two files");

    let inventory = Scanner::scan(output_dir.path()).unwrap();
    assert_eq!(inventory.files.len(), 2);
    let total_resources: usize = inventory.files.iter().map(|f| f.resources.len()).sum();
    assert_eq!(total_resources, 3, "all 3 resources land in the output");
}

// ─────────────────────────────────────────────────────────────────────────
// Test 4 — Template miss is a loud Err (Article IV; spec criterion #4)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn template_miss_is_loud_error() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let mut attrs = BTreeMap::new();
    attrs.insert("foo".to_string(), make_attr_string("bar"));
    let unknown = MappedResource {
        source_addr: "unknown_type.x".to_string(),
        target_addr: "unknown_type.x".to_string(),
        target_type: "unknown_type".to_string(),
        target_name: "x".to_string(),
        attributes: attrs,
        dependencies: vec![],
    };

    let plan = plan_with(vec![unknown]);
    let g = Generator::new();
    let result = g.generate(cwd.path(), output_dir.path(), &plan);

    assert!(result.is_err(), "unknown target_type must error");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("unknown_type"),
        "error names the missing type: {msg}"
    );
    assert!(
        msg.contains("template miss"),
        "error says 'template miss': {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 5 — Backup-first preservation (spec criterion #2)
// Pre-existing file at output dir is moved to .terrashift/runs/.../backups/
// before overwrite; the original content is byte-identical to the backup.
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn backup_first_preserves_original_content() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Pre-existing file with hand-authored content.
    let target_path = output_dir.path().join("aws_vpc.tf");
    let original_content = b"# Original hand-authored file\nresource \"aws_vpc\" \"old\" {\n  cidr_block = \"172.16.0.0/16\"\n}\n";
    std::fs::write(&target_path, original_content).unwrap();

    let plan = plan_with(vec![aws_vpc_resource("main", "10.0.0.0/16")]);
    let g = Generator::new();
    let artifacts = g.generate(cwd.path(), output_dir.path(), &plan).unwrap();

    // Exactly one backup created (the pre-existing aws_vpc.tf).
    assert_eq!(artifacts.backups_created, 1);
    assert_eq!(artifacts.backups.len(), 1);

    // Backup lives under .terrashift/runs/{run_id}/backups/{op_uuid}/
    let backup_path = &artifacts.backups[0];
    let backup_str = backup_path.to_string_lossy();
    assert!(
        backup_str.contains(".terrashift")
            && backup_str.contains("runs")
            && backup_str.contains("backups"),
        "backup path follows .terrashift/runs/.../backups/ scheme: {backup_str}"
    );
    assert!(
        backup_str.contains(&plan.run_id.to_string()),
        "backup path includes run_id"
    );

    // Backup file is byte-identical to original.
    let backup_content = std::fs::read(backup_path).unwrap();
    assert_eq!(
        backup_content, original_content,
        "backup preserves original byte-for-byte (Article V)"
    );

    // The new aws_vpc.tf has the new content.
    let new_content = std::fs::read_to_string(&target_path).unwrap();
    assert!(new_content.contains("aws_vpc"));
    assert!(new_content.contains("main"));
    assert!(new_content.contains("10.0.0.0/16"));
}

// ─────────────────────────────────────────────────────────────────────────
// Test 6 — Greenfield write: no backup created when target file is absent
// (audit metadata correctness — spec criterion #5)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn greenfield_write_creates_no_backup() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let plan = plan_with(vec![aws_vpc_resource("main", "10.0.0.0/16")]);
    let g = Generator::new();
    let artifacts = g.generate(cwd.path(), output_dir.path(), &plan).unwrap();

    assert_eq!(artifacts.backups_created, 0, "no overwrite, no backup");
    assert_eq!(artifacts.backups.len(), 0);
    assert_eq!(artifacts.emitted.len(), 1);
    assert!(
        artifacts.emitted[0].backup_path.is_none(),
        "EmittedFile.backup_path is None for greenfield"
    );
    assert!(!artifacts.emitted[0].overwrote, "overwrote = false");
}

// ─────────────────────────────────────────────────────────────────────────
// Test 7 — Rollback restores backed-up file (spec criterion #3, Article IX)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn rollback_restores_backed_up_file() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let target_path = output_dir.path().join("aws_vpc.tf");
    let original = b"# I am the original\n";
    std::fs::write(&target_path, original).unwrap();

    let plan = plan_with(vec![aws_vpc_resource("main", "10.0.0.0/16")]);
    let g = Generator::new();
    let artifacts = g.generate(cwd.path(), output_dir.path(), &plan).unwrap();
    assert_eq!(artifacts.backups_created, 1);

    // Now roll back.
    g.rollback(&artifacts).unwrap();

    let restored = std::fs::read(&target_path).unwrap();
    assert_eq!(
        restored, original,
        "rollback restores byte-identical original (Article IX — backups archival)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 8 — Determinism: same plan twice → byte-identical output
// (Article VI; spec criterion #6)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn determinism_byte_identical_across_runs() {
    let plan = plan_with(vec![
        aws_subnet_resource("c", "10.0.3.0/24", "aws_vpc.main.id"),
        aws_vpc_resource("main", "10.0.0.0/16"),
        aws_subnet_resource("a", "10.0.1.0/24", "aws_vpc.main.id"),
    ]);

    let cwd1 = TempDir::new().unwrap();
    let out1 = TempDir::new().unwrap();
    let g = Generator::new();
    g.generate(cwd1.path(), out1.path(), &plan).unwrap();

    let cwd2 = TempDir::new().unwrap();
    let out2 = TempDir::new().unwrap();
    g.generate(cwd2.path(), out2.path(), &plan).unwrap();

    // Compare aws_vpc.tf and aws_subnet.tf across the two runs.
    for filename in &["aws_vpc.tf", "aws_subnet.tf"] {
        let content1 = std::fs::read(out1.path().join(filename)).unwrap();
        let content2 = std::fs::read(out2.path().join(filename)).unwrap();
        assert_eq!(
            content1, content2,
            "{filename} byte-identical across runs (Article VI)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 9 — References emit unquoted (raw HCL traversal, not string literal)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn references_emit_as_raw_hcl_traversals() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let plan = plan_with(vec![aws_subnet_resource(
        "a",
        "10.0.1.0/24",
        "aws_vpc.main.id",
    )]);
    let g = Generator::new();
    g.generate(cwd.path(), output_dir.path(), &plan).unwrap();

    let content = std::fs::read_to_string(output_dir.path().join("aws_subnet.tf")).unwrap();

    // Reference must emit unquoted: vpc_id = aws_vpc.main.id  (not "aws_vpc.main.id")
    assert!(
        content.contains("vpc_id = aws_vpc.main.id"),
        "reference emits as raw traversal, not quoted string. Got:\n{content}"
    );
    assert!(
        !content.contains("vpc_id = \"aws_vpc.main.id\""),
        "reference must NOT be quoted. Got:\n{content}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 10 — InvalidOutputDir error when target isn't a directory.
// Uses a TempDir-relative non-existent subpath so it's portable (M3).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn invalid_output_dir_is_loud_error() {
    let cwd = TempDir::new().unwrap();
    let nonexistent = cwd.path().join("does_not_exist_subpath");
    let plan = plan_with(vec![aws_vpc_resource("main", "10.0.0.0/16")]);
    let g = Generator::new();
    let err = g.generate(cwd.path(), &nonexistent, &plan).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("not a directory") || msg.contains("does_not_exist_subpath"),
        "invalid output dir is loud: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 11 — Empty reference is a loud error (M2 fix; Article IV)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn empty_reference_is_loud_error() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Build a resource with an empty Reference attribute.
    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), make_attr_string("10.0.1.0/24"));
    attrs.insert("vpc_id".to_string(), make_attr_ref("")); // EMPTY reference
    let bad = MappedResource {
        source_addr: "aws_subnet.bad".to_string(),
        target_addr: "aws_subnet.bad".to_string(),
        target_type: "aws_subnet".to_string(),
        target_name: "bad".to_string(),
        attributes: attrs,
        dependencies: vec![],
    };

    let plan = plan_with(vec![bad]);
    let g = Generator::new();
    let err = g
        .generate(cwd.path(), output_dir.path(), &plan)
        .unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("empty reference") || msg.contains("EmptyReference"),
        "empty reference is loud (Article IV): {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test — every registered template renders with minimal-valid input.
// Catches the "registered but required-keys list disagrees with template
// body" class of bug. Runs all 17 templates in one go.
// ─────────────────────────────────────────────────────────────────────────

/// Minimal-required attribute keys per template, derived at runtime from the
/// bundled provider schemas at `libs/knowledge/seed/<provider>/<version>/schema.json`.
///
/// Single source of truth: when a new template registers, this helper picks
/// up its required attributes automatically by reading the schema. When
/// `cargo xtask capture-schemas` re-pins to a newer provider version and the
/// upstream `required:true` set shifts, the test follows. No parallel hand-
/// maintained map to drift.
///
/// Article XIII rule 5: schemas are the source of truth — never duplicate
/// what `terraform providers schema -json` already records.
fn minimal_required_attrs(target_type: &str) -> Vec<(String, AttributeValue)> {
    let provider = target_type
        .split('_')
        .next()
        .unwrap_or_else(|| panic!("target_type '{target_type}' has no provider prefix segment"));
    let schema_path = seed_schema_path(provider);
    let raw = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("read seed schema {}: {}", schema_path.display(), e));
    let schema: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse seed schema {}: {}", schema_path.display(), e));

    let attrs = schema["resources"][target_type]["attributes"]
        .as_object()
        .unwrap_or_else(|| {
            panic!(
                "seed schema for provider '{provider}' has no resource '{target_type}' — \
                 either the template was registered for a non-existent type or the \
                 seed needs `cargo xtask capture-schemas` re-run"
            )
        });

    let mut keys: std::collections::BTreeSet<String> = attrs
        .iter()
        .filter(|(_, v)| v["required"].as_bool() == Some(true))
        .map(|(k, _)| k.clone())
        .collect();
    // Templates may enforce policy stricter than the upstream schema. Example:
    // `aws_vpc.cidr_block` is `required:false` per the schema (alternative is
    // `ipv4_ipam_pool_id`) but our template makes it mandatory because the
    // migration source we expect always carries a CIDR. These overrides live
    // adjacent to the template body in spirit; the test pulls them in here.
    for extra in template_policy_required(target_type) {
        keys.insert((*extra).to_string());
    }

    keys.into_iter()
        .map(|k| {
            let attr_type = attrs
                .get(&k)
                .and_then(|v| v["attribute_type"].as_str())
                .unwrap_or("string");
            let value = synthesize_value(&k, attr_type);
            (k, value)
        })
        .collect()
}

/// Template-policy overrides — required attrs the Generator template enforces
/// even when the schema says `required:false`. Keep this list short and
/// adjacent in spirit to the template definitions in `templates.rs`.
fn template_policy_required(target_type: &str) -> &'static [&'static str] {
    match target_type {
        "aws_vpc" => &["cidr_block"],
        "aws_subnet" => &["cidr_block"],
        "aws_security_group" => &["name"],
        "aws_instance" => &["instance_type"],
        "aws_s3_bucket" => &["bucket"],
        "azurerm_linux_virtual_machine" => &["size"],
        "azurerm_storage_account" => &["account_tier", "account_replication_type"],
        "azurerm_public_ip" => &["allocation_method"],
        "azurerm_subnet" => &["virtual_network_name"],
        "google_compute_subnetwork" => &["ip_cidr_range", "network", "region"],
        "google_compute_instance" => &["machine_type", "zone"],
        "google_compute_firewall" => &["network"],
        _ => &[],
    }
}

/// Resolve `libs/knowledge/seed/<provider>/<latest-version>/schema.json` from
/// the test crate, picking the highest-versioned subdirectory.
fn seed_schema_path(provider: &str) -> std::path::PathBuf {
    let seed_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("knowledge")
        .join("seed")
        .join(provider);
    let mut versions: Vec<_> = std::fs::read_dir(&seed_root)
        .unwrap_or_else(|e| panic!("read seed dir {}: {}", seed_root.display(), e))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    versions.sort();
    versions
        .pop()
        .unwrap_or_else(|| panic!("no seeded versions for provider '{provider}'"))
        .join("schema.json")
}

/// Synthesize a minimal-but-valid `AttributeValue` for the given (name, type)
/// pair. The placeholder values are chosen to satisfy common `terraform validate`
/// shape constraints (CIDR notation, well-known regions, etc.) so the rendered
/// HCL parses round-trip via the Scanner.
fn synthesize_value(attr_name: &str, attr_type: &str) -> AttributeValue {
    use AttributeValue::*;

    if attr_name.ends_with("_id") || matches!(attr_name, "network") {
        let stem = attr_name.trim_end_matches("_id");
        return Reference(format!("{stem}.placeholder.id"));
    }

    if attr_type.starts_with("string") {
        let placeholder = match attr_name {
            "cidr_block" | "ip_cidr_range" => "10.0.0.0/16",
            "location" => "eastus",
            "region" => "us-central1",
            "zone" => "us-central1-a",
            "machine_type" => "e2-micro",
            "size" => "Standard_F2",
            "instance_type" => "t3.micro",
            "account_tier" => "Standard",
            "account_replication_type" => "LRS",
            "allocation_method" => "Static",
            "assume_role_policy" => "{}",
            "name" => "tf-test",
            "bucket" => "tf-test-bucket",
            "virtual_network_name" => "vnet-1",
            "resource_group_name" => "rg-1",
            _ => "placeholder",
        };
        String(placeholder.to_string())
    } else if attr_type.starts_with("bool") {
        Bool(false)
    } else if attr_type.starts_with("number") {
        Number(0.0)
    } else if attr_type.starts_with("list") || attr_type.starts_with("set") {
        List(vec![String("10.0.0.0/16".to_string())])
    } else if attr_type.starts_with("map") {
        Map(BTreeMap::new())
    } else {
        panic!(
            "synthesize_value: unsupported attribute_type '{attr_type}' for attr '{attr_name}' \
             — required nested-block attrs need flat-template handling or test-side stubbing"
        );
    }
}

/// Hard-coded list of all 21 registered templates. The `template_count_is_21`
/// test catches drift in the registry size; this test catches drift in
/// per-template wiring.
const ALL_TEMPLATES: &[&str] = &[
    "aws_vpc",
    "aws_subnet",
    "aws_security_group",
    "aws_instance",
    "aws_s3_bucket",
    "azurerm_virtual_network",
    "azurerm_subnet",
    "azurerm_network_security_group",
    "azurerm_linux_virtual_machine",
    "azurerm_storage_account",
    "aws_iam_role",
    "google_storage_bucket",
    "azurerm_route_table",
    "azurerm_public_ip",
    "azurerm_subnet_network_security_group_association",
    "azurerm_subnet_route_table_association",
    "azurerm_resource_group",
    "google_compute_network",
    "google_compute_subnetwork",
    "google_compute_firewall",
    "google_compute_instance",
];

#[test]
fn every_registered_template_renders_with_minimal_required_attrs() {
    let g = Generator::new();
    assert_eq!(
        ALL_TEMPLATES.len(),
        g.template_count(),
        "ALL_TEMPLATES list ({}) drifted from registry size ({}). \
         Update both when adding/removing a template.",
        ALL_TEMPLATES.len(),
        g.template_count()
    );

    for target_type in ALL_TEMPLATES {
        let cwd = TempDir::new().unwrap();
        let output = TempDir::new().unwrap();

        let mut attrs = BTreeMap::new();
        for (k, v) in minimal_required_attrs(target_type) {
            attrs.insert(k.to_string(), v);
        }

        let resource = MappedResource {
            source_addr: format!("{target_type}.test"),
            target_addr: format!("{target_type}.test"),
            target_type: (*target_type).to_string(),
            target_name: "test".to_string(),
            attributes: attrs,
            dependencies: vec![],
        };

        let plan = plan_with(vec![resource]);
        let result = g.generate(cwd.path(), output.path(), &plan);
        assert!(
            result.is_ok(),
            "template `{target_type}` failed with minimal required attrs: {:?}",
            result.err()
        );

        let artifacts = result.unwrap();
        assert_eq!(
            artifacts.files.len(),
            1,
            "`{target_type}` should emit one file"
        );

        let content = std::fs::read_to_string(&artifacts.files[0]).unwrap();
        assert!(
            content.contains(target_type),
            "`{target_type}.tf` should contain the resource type:\n{content}"
        );
        assert!(
            content.contains("\"test\""),
            "`{target_type}.tf` should contain the resource name `test`:\n{content}"
        );
    }
}

/// Negative complement to the above — every template surfaces
/// `MapperLookupError::Missing` (wrapped as `GeneratorError::TemplateRender`)
/// when its first required attribute is omitted. Catches "template registered
/// with wrong required-keys list" and "template silently emits empty fields".
#[test]
fn every_template_loud_errors_on_missing_required_attr() {
    let g = Generator::new();
    for target_type in ALL_TEMPLATES {
        let mut required = minimal_required_attrs(target_type);
        if required.is_empty() {
            // Defensive: every template currently has at least one required
            // attr. If a future change introduces one with zero, skip it
            // here intentionally rather than fail the test.
            continue;
        }
        let dropped_key = required.remove(0).0;

        let mut attrs = BTreeMap::new();
        for (k, v) in required {
            attrs.insert(k.to_string(), v);
        }
        let resource = MappedResource {
            source_addr: format!("{target_type}.test"),
            target_addr: format!("{target_type}.test"),
            target_type: (*target_type).to_string(),
            target_name: "test".to_string(),
            attributes: attrs,
            dependencies: vec![],
        };
        let plan = plan_with(vec![resource]);

        let cwd = TempDir::new().unwrap();
        let output = TempDir::new().unwrap();
        let result = g.generate(cwd.path(), output.path(), &plan);
        assert!(
            result.is_err(),
            "`{target_type}` accepted a plan missing required attr `{dropped_key}` (Article IV: must loud-fail)"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains(&dropped_key) || msg.to_lowercase().contains("missing"),
            "error for `{target_type}` should mention missing key `{dropped_key}`. Got: {msg}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 12 — `Generator::rollback` on a fresh greenfield artifact removes
// the freshly-created file (no backup exists). Verifies M1 fix's "delete
// greenfield" branch works in isolation.
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn rollback_greenfield_deletes_created_file() {
    let cwd = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let plan = plan_with(vec![aws_vpc_resource("main", "10.0.0.0/16")]);
    let g = Generator::new();
    let artifacts = g.generate(cwd.path(), output_dir.path(), &plan).unwrap();
    let target = output_dir.path().join("aws_vpc.tf");
    assert!(target.exists(), "file written");
    assert_eq!(artifacts.backups_created, 0, "greenfield: no backup");

    g.rollback(&artifacts).unwrap();
    assert!(
        !target.exists(),
        "rollback removes greenfield file (Article V — reversibility holds even when no backup exists)"
    );
}
