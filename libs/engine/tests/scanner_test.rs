// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Integration tests for the Scanner.
//!
//! Coverage per `specs/004-scanner/spec.md`:
//!   1. Single file with one resource → 1 resource in inventory
//!   2. Walk fixture's aws/modules/vpc/ → finds aws_vpc + aws_subnet (×3) + igw
//!   3. Provider block → captures type + region attribute
//!   4. Module block → captures source path
//!   5. Dynamic block → ScannerError::UnsupportedFeature
//!   6. (Conditional) full fixture walk if fixtures/aws-to-azure-real/aws/ exists

use std::path::{Path, PathBuf};
use tempfile::TempDir;
use terrashift_engine::scanner::{Scanner, ScannerError};

fn write_tf(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap_or_else(|e| panic!("write {name}: {e}"));
    path
}

#[test]
fn parses_single_resource() {
    let content = r#"
resource "aws_vpc" "default" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
}
"#;
    let scanned = Scanner::parse_file(Path::new("inline.tf"), content)
        .unwrap_or_else(|e| panic!("parse failed: {e}"));

    assert_eq!(scanned.resources.len(), 1);
    let r = &scanned.resources[0];
    assert_eq!(r.resource_type, "aws_vpc");
    assert_eq!(r.name, "default");
    assert!(
        r.attributes.contains_key("cidr_block"),
        "expected cidr_block attribute, got: {:?}",
        r.attributes.keys().collect::<Vec<_>>()
    );
}

#[test]
fn parses_provider_block() {
    let content = r#"
provider "aws" {
  region = "us-east-1"
  profile = "prod"
}
"#;
    let scanned = Scanner::parse_file(Path::new("inline.tf"), content)
        .unwrap_or_else(|e| panic!("parse failed: {e}"));

    assert_eq!(scanned.providers.len(), 1);
    let p = &scanned.providers[0];
    assert_eq!(p.provider_type, "aws");
    assert!(p.attributes.contains_key("region"));
    assert!(p.attributes.contains_key("profile"));
}

#[test]
fn parses_module_block_captures_source() {
    let content = r#"
module "vpc" {
  source = "../../modules/vpc"
  env    = var.env
}
"#;
    let scanned = Scanner::parse_file(Path::new("inline.tf"), content)
        .unwrap_or_else(|e| panic!("parse failed: {e}"));

    assert_eq!(scanned.modules.len(), 1);
    let m = &scanned.modules[0];
    assert_eq!(m.name, "vpc");
    assert_eq!(m.source.as_deref(), Some("../../modules/vpc"));
}

#[test]
fn dynamic_block_is_loud_failure() {
    let content = r#"
resource "aws_security_group" "demo" {
  name = "demo"

  dynamic "ingress" {
    for_each = var.ingress_rules
    content {
      from_port = ingress.value.from
    }
  }
}
"#;
    let err = Scanner::parse_file(Path::new("inline.tf"), content)
        .err()
        .unwrap_or_else(|| panic!("expected Err, got Ok"));

    match err {
        ScannerError::UnsupportedFeature { feature, .. } => {
            assert!(
                feature.contains("dynamic"),
                "feature should mention dynamic, got {feature}"
            );
        }
        other => panic!("expected UnsupportedFeature, got {other:?}"),
    }
}

#[test]
fn walks_directory_and_aggregates() {
    let tmp = TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    write_tf(
        tmp.path(),
        "vpc.tf",
        r#"resource "aws_vpc" "main" { cidr_block = "10.0.0.0/16" }"#,
    );
    write_tf(
        tmp.path(),
        "subnet.tf",
        r#"
resource "aws_subnet" "a" { vpc_id = aws_vpc.main.id }
resource "aws_subnet" "b" { vpc_id = aws_vpc.main.id }
"#,
    );

    let inv = Scanner::scan(tmp.path()).unwrap_or_else(|e| panic!("scan: {e}"));
    assert_eq!(inv.files.len(), 2);
    assert_eq!(inv.resource_count(), 3);
}

#[test]
fn empty_directory_yields_empty_inventory() {
    let tmp = TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let inv = Scanner::scan(tmp.path()).unwrap_or_else(|e| panic!("scan empty: {e}"));
    assert_eq!(inv.files.len(), 0);
    assert_eq!(inv.resource_count(), 0);
}

#[test]
fn directory_with_only_non_tf_files_yields_empty_inventory() {
    let tmp = TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    std::fs::write(tmp.path().join("readme.md"), "# notes").unwrap();
    std::fs::write(tmp.path().join("config.yaml"), "key: value").unwrap();
    let inv = Scanner::scan(tmp.path()).unwrap_or_else(|e| panic!("scan: {e}"));
    assert_eq!(inv.files.len(), 0, "should ignore non-.tf files");
}

#[test]
fn broken_hcl_returns_parse_error_not_panic() {
    let content = r#"
resource "aws_vpc" "broken" {
  cidr_block = "10.0.0.0/16
  this_is_unterminated_string
"#;
    let err = Scanner::parse_file(Path::new("broken.tf"), content)
        .err()
        .unwrap_or_else(|| panic!("expected parse failure, got Ok"));
    match err {
        ScannerError::Parse { message, .. } => {
            assert!(
                !message.is_empty(),
                "parse error should carry a message, got empty"
            );
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn skips_dot_terraform_and_hidden_dirs() {
    let tmp = TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    // Real tf file at root
    write_tf(
        tmp.path(),
        "main.tf",
        r#"resource "aws_vpc" "real" { cidr_block = "10.0.0.0/16" }"#,
    );
    // Should be skipped: .terraform/
    std::fs::create_dir_all(tmp.path().join(".terraform")).unwrap();
    write_tf(
        &tmp.path().join(".terraform"),
        "stale.tf",
        r#"resource "aws_vpc" "stale" { cidr_block = "9.9.9.9/32" }"#,
    );
    // Should be skipped: .git/
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    write_tf(
        &tmp.path().join(".git"),
        "weird.tf",
        r#"resource "aws_vpc" "weird" {}"#,
    );

    let inv = Scanner::scan(tmp.path()).unwrap_or_else(|e| panic!("scan: {e}"));
    assert_eq!(
        inv.resource_count(),
        1,
        "only main.tf::real should be found"
    );
    let names: Vec<&str> = inv
        .files
        .iter()
        .flat_map(|f| f.resources.iter().map(|r| r.name.as_str()))
        .collect();
    assert!(names.contains(&"real"));
    assert!(!names.contains(&"stale"));
    assert!(!names.contains(&"weird"));
}

#[test]
fn nonexistent_root_returns_io_error() {
    // Build a path inside a fresh tempdir that we never create. tempdir
    // resolves to the OS-appropriate root (so this works on Windows + Linux
    // + macOS); the joined subpath is pure path arithmetic, not OS-shaped
    // string literals (Article IV: failures must be loud REGARDLESS of
    // which OS the contributor is testing on).
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let bogus = dir.path().join("definitely-not-a-real-subdir");
    let err = Scanner::scan(&bogus)
        .err()
        .unwrap_or_else(|| panic!("expected error for nonexistent root"));
    match err {
        ScannerError::Io { source, .. } => {
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        }
        other => panic!("expected Io NotFound, got {other:?}"),
    }
}

#[test]
fn data_source_block_is_parsed() {
    let content = r#"
data "aws_caller_identity" "current" {}
"#;
    let scanned = Scanner::parse_file(Path::new("inline.tf"), content)
        .unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(scanned.data_sources.len(), 1);
    assert_eq!(scanned.data_sources[0].data_type, "aws_caller_identity");
    assert_eq!(scanned.data_sources[0].name, "current");
}

/// Fixture-conditional: only runs when `fixtures/aws-to-azure-real/` exists.
/// Per clarify Q6 + Q1 — graceful skip when fixture not checked out.
#[test]
fn fixture_walk_aws_modules_vpc() {
    let workspace_root = env!("CARGO_MANIFEST_DIR");
    // libs/engine -> ../.. -> workspace root
    let fixture = PathBuf::from(workspace_root)
        .join("..")
        .join("..")
        .join("fixtures")
        .join("aws-to-azure-real")
        .join("aws")
        .join("modules")
        .join("vpc");

    if !fixture.exists() {
        eprintln!(
            "[skip] fixture not present at {} — run scripts/setup-fixtures.ps1",
            fixture.display()
        );
        return;
    }

    let inv = Scanner::scan(&fixture).unwrap_or_else(|e| panic!("fixture scan: {e}"));

    // Pratik's vpc module has aws_vpc, aws_subnet (×3), aws_internet_gateway,
    // aws_route_table, aws_route_table_association
    assert!(
        inv.resource_count() >= 5,
        "expected ≥5 resources in fixture vpc module, got {}",
        inv.resource_count()
    );

    let types: Vec<&str> = inv
        .files
        .iter()
        .flat_map(|f| f.resources.iter().map(|r| r.resource_type.as_str()))
        .collect();
    assert!(
        types.contains(&"aws_vpc"),
        "expected aws_vpc resource, got types: {types:?}"
    );
    assert!(
        types.contains(&"aws_subnet"),
        "expected aws_subnet resource, got types: {types:?}"
    );
}
