// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Asserts the perf-50 fixture has the documented shape.
//! Spec: `docs/superpowers/specs/2026-05-10-extended-testing-design.md`.

use std::collections::BTreeMap;
use terrashift_engine::scanner::Scanner;

#[test]
fn perf_50_fixture_has_50_resources() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("perf-50");
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());

    let inv = Scanner::scan(&fixture).expect("scan perf-50");
    let total: usize = inv.files.iter().map(|f| f.resources.len()).sum();
    assert_eq!(total, 50, "expected 50 resources; found {total}");

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for file in &inv.files {
        for r in &file.resources {
            *by_type.entry(r.resource_type.clone()).or_default() += 1;
        }
    }
    assert_eq!(by_type.get("aws_vpc"), Some(&1));
    assert_eq!(by_type.get("aws_subnet"), Some(&10));
    assert_eq!(by_type.get("aws_security_group"), Some(&10));
    assert_eq!(by_type.get("aws_instance"), Some(&10));
    assert_eq!(by_type.get("aws_s3_bucket"), Some(&10));
    assert_eq!(by_type.get("aws_iam_role"), Some(&9));
}
