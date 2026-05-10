// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Performance benches for Scanner + Generator (no LLM, deterministic).
//!
//! Spec: `docs/superpowers/specs/2026-05-10-extended-testing-design.md`.
//! Fixture: `fixtures/perf-50/`.
//! Pattern source: `libs/engine/tests/generator_test.rs` —
//! hand-curated source→target type map + schema-driven required attrs
//! (no hardcoding rule).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

use terrashift_engine::generator::Generator;
use terrashift_engine::mapper::{AttributeValue, MappedResource, MappingPlan};
use terrashift_engine::scanner::Scanner;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("perf-50")
}

fn aws_to_azurerm(aws_type: &str) -> Option<&'static str> {
    match aws_type {
        "aws_vpc" => Some("azurerm_virtual_network"),
        "aws_subnet" => Some("azurerm_subnet"),
        "aws_security_group" => Some("azurerm_network_security_group"),
        "aws_instance" => Some("azurerm_linux_virtual_machine"),
        "aws_s3_bucket" => Some("azurerm_storage_account"),
        "aws_iam_role" => None,
        _ => None,
    }
}

fn schema_required_attrs(target_type: &str) -> Vec<(String, AttributeValue)> {
    let provider = target_type
        .split('_')
        .next()
        .unwrap_or_else(|| panic!("target_type '{target_type}' has no provider prefix"));

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
        String("placeholder".to_string())
    }
}

fn build_plan() -> MappingPlan {
    let inv = Scanner::scan(&fixture_path()).expect("scan perf-50");
    let mut mapped = vec![];
    for file in &inv.files {
        for r in &file.resources {
            if let Some(target_type) = aws_to_azurerm(&r.resource_type) {
                let target_name = r.name.clone();
                let attrs: BTreeMap<String, AttributeValue> =
                    schema_required_attrs(target_type).into_iter().collect();
                mapped.push(MappedResource {
                    source_addr: format!("{}.{}", r.resource_type, r.name),
                    target_addr: format!("{target_type}.{target_name}"),
                    target_type: target_type.to_string(),
                    target_name,
                    attributes: attrs,
                    dependencies: vec![],
                });
            }
        }
    }
    MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "aws".to_string(),
        target_provider: "azurerm".to_string(),
        resources: mapped,
    }
}

fn bench_scanner(c: &mut Criterion) {
    let path = fixture_path();
    c.bench_function("scanner_50", |b| {
        b.iter(|| {
            black_box(Scanner::scan(&path).expect("scan"));
        });
    });
}

fn bench_generator(c: &mut Criterion) {
    let plan = build_plan();
    let g = Generator::new();
    c.bench_function("generator_50", |b| {
        b.iter_batched(
            || {
                (
                    tempfile::TempDir::new().expect("setup cwd"),
                    tempfile::TempDir::new().expect("setup out"),
                )
            },
            |(cwd, out)| {
                black_box(g.generate(cwd.path(), out.path(), &plan).expect("generate"));
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_scanner, bench_generator);
criterion_main!(benches);

// Canary tests — kept inline per spec so a "broken bench" surfaces without
// paying the cargo-bench wall-time cost. With `harness = false`, criterion's
// `criterion_main!` owns `main()` and `cargo test --bench pipeline` invokes
// criterion's built-in test mode (one iteration per bench group, reported as
// `Testing X / Success`). The `#[test]` functions below are kept because
// they document intent and would activate if `harness` is ever flipped to
// `true`; today they are intentionally dead and the lint is silenced.
#[cfg(test)]
#[allow(dead_code, unused_imports)]
mod canary {
    use super::*;

    #[test]
    fn canary_scanner_runs() {
        Scanner::scan(&fixture_path()).expect("scanner canary");
    }

    #[test]
    fn canary_generator_runs() {
        let plan = build_plan();
        let g = Generator::new();
        let cwd = tempfile::TempDir::new().expect("setup cwd");
        let out = tempfile::TempDir::new().expect("setup out");
        g.generate(cwd.path(), out.path(), &plan)
            .expect("generator canary");
    }

    #[test]
    fn canary_build_plan_yields_at_least_41_mapped() {
        let plan = build_plan();
        assert!(
            plan.resources.len() >= 41,
            "expected ≥41 mapped resources (50 - 9 iam_role); got {}",
            plan.resources.len()
        );
    }
}
