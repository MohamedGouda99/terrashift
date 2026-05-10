// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Schema-driven test fixture helpers.
//!
//! Single source of truth: every test that needs to know "what attributes
//! does `aws_vpc` require?" reads from `libs/knowledge/seed/<provider>/
//! <version>/schema.json` at test time. When `cargo xtask capture-schemas`
//! re-pins to a newer provider version, tests follow without code edits.
//!
//! Constitution: Article VI (knowledge-layer integrity, version pinning),
//! Article XIII rule 5 (schemas are the source of truth — never duplicate
//! what `terraform providers schema -json` already records).

use std::collections::BTreeMap;
use std::path::PathBuf;

use terrashift_engine::mapper::AttributeValue;

/// Resolve `libs/knowledge/seed/<provider>/<latest-version>/schema.json` from
/// the test crate, picking the highest-versioned subdirectory.
pub fn seed_schema_path(provider: &str) -> PathBuf {
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

/// Pick the latest seeded version directory name for a provider, e.g. `"6.44.0"`.
/// Used by tests that need to embed the version in a `MappingPlan` or assertion
/// string without hardcoding it.
pub fn seed_latest_version(provider: &str) -> String {
    let path = seed_schema_path(provider);
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_else(|| panic!("malformed seed schema path for provider '{provider}'"))
        .to_string()
}

/// Minimal-required attribute keys for a given `target_type`, derived at runtime
/// from the bundled provider schemas.
///
/// When a new template registers, this helper picks up its required attributes
/// automatically by reading the schema. When `cargo xtask capture-schemas`
/// re-pins to a newer provider version and the upstream `required:true` set
/// shifts, the test follows. No parallel hand-maintained map to drift.
pub fn minimal_required_attrs(target_type: &str) -> Vec<(String, AttributeValue)> {
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
pub fn template_policy_required(target_type: &str) -> &'static [&'static str] {
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

/// Synthesize a minimal-but-valid `AttributeValue` for the given (name, type)
/// pair. The placeholder values are chosen to satisfy common `terraform validate`
/// shape constraints (CIDR notation, well-known regions, etc.) so the rendered
/// HCL parses round-trip via the Scanner.
pub fn synthesize_value(attr_name: &str, attr_type: &str) -> AttributeValue {
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

/// All resource types of a provider that have at least one required attribute.
/// Useful for "pick N resources from this provider" fixture builders.
pub fn provider_resource_types_with_required(provider: &str) -> Vec<String> {
    let schema_path = seed_schema_path(provider);
    let raw = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("read seed schema {}: {}", schema_path.display(), e));
    let schema: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse seed schema {}: {}", schema_path.display(), e));
    let resources = schema["resources"]
        .as_object()
        .unwrap_or_else(|| panic!("seed schema for '{provider}' has no `resources` object"));

    let mut out: Vec<String> = resources
        .iter()
        .filter(|(_, v)| {
            v["attributes"]
                .as_object()
                .map(|attrs| {
                    attrs
                        .values()
                        .any(|attr| attr["required"].as_bool() == Some(true))
                })
                .unwrap_or(false)
        })
        .map(|(k, _)| k.clone())
        .collect();
    out.sort();
    out
}
