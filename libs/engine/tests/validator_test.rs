// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Validator integration tests — Article III + IV enforcement.
//!
//! Each test constructs a `KnowledgeService` over an in-memory
//! `LocalSchemaStore`, seeds it with one synthetic `ProviderSchema`,
//! then runs `Validator::validate` against hand-crafted `MappingPlan`s.
//!
//! Note on the file-level allow: clippy's `allow-unwrap-in-tests` heuristic
//! covers `#[test]`-annotated functions and `#[cfg(test)]` modules, but
//! NOT plain `async fn` helpers in integration test files. Since this
//! whole file is test-context (it lives in `tests/`), allowing unwrap +
//! expect file-wide matches the spirit of the workspace clippy.toml
//! relaxation without spreading per-fn `#[allow]`s.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

use terrashift_engine::mapper::{AttributeValue, MappedResource, MappingPlan};
use terrashift_engine::validator::{ValidationError, ValidationWarning, Validator};
use terrashift_knowledge::embedding::StubEmbeddingService;
use terrashift_knowledge::local_schema_store::LocalSchemaStore;
use terrashift_knowledge::registry_client::StubSchemaFetcher;
use terrashift_knowledge::schema_store::SchemaStore;
use terrashift_knowledge::types::{AttributeSchema, ProviderSchema, ResourceSchema};
use terrashift_knowledge::vector_store::InMemoryVectorStore;
use terrashift_knowledge::KnowledgeService;

// ─────────────────────────────────────────────────────────────────────────
// Test fixture helpers
// ─────────────────────────────────────────────────────────────────────────

/// Build a synthetic `aws` provider schema with one `aws_vpc` resource.
/// Required: `cidr_block`. Optional: `instance_tenancy`. Computed:
/// `arn`. Deprecated: `enable_classiclink` (since "5.0.0").
fn make_aws_test_schema() -> ProviderSchema {
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "cidr_block".to_string(),
        AttributeSchema {
            name: "cidr_block".to_string(),
            attribute_type: "string".to_string(),
            required: true,
            optional: false,
            computed: false,
            sensitive: false,
            deprecated: None,
            description: Some("IPv4 CIDR".to_string()),
        },
    );
    attrs.insert(
        "instance_tenancy".to_string(),
        AttributeSchema {
            name: "instance_tenancy".to_string(),
            attribute_type: "string".to_string(),
            required: false,
            optional: true,
            computed: false,
            sensitive: false,
            deprecated: None,
            description: None,
        },
    );
    attrs.insert(
        "arn".to_string(),
        AttributeSchema {
            name: "arn".to_string(),
            attribute_type: "string".to_string(),
            required: false,
            optional: false,
            computed: true,
            sensitive: false,
            deprecated: None,
            description: Some("Read-only ARN".to_string()),
        },
    );
    attrs.insert(
        "enable_classiclink".to_string(),
        AttributeSchema {
            name: "enable_classiclink".to_string(),
            attribute_type: "bool".to_string(),
            required: false,
            optional: true,
            computed: false,
            sensitive: false,
            deprecated: Some("5.0.0".to_string()),
            description: Some("Removed by AWS in 2022".to_string()),
        },
    );
    let mut resources = BTreeMap::new();
    resources.insert(
        "aws_vpc".to_string(),
        ResourceSchema {
            name: "aws_vpc".to_string(),
            description: Some("AWS Virtual Private Cloud".to_string()),
            attributes: attrs,
        },
    );

    ProviderSchema {
        provider: "aws".to_string(),
        version: "5.30.0".to_string(),
        resources,
        data_sources: BTreeMap::new(),
        fetched_at: Utc::now(),
    }
}

async fn make_knowledge_with_aws_schema() -> Arc<KnowledgeService> {
    let store = LocalSchemaStore::in_memory().await.unwrap();
    let schema = make_aws_test_schema();
    store.cache_schema(&schema).await.unwrap();

    Arc::new(KnowledgeService::new(
        Arc::new(store),
        Arc::new(InMemoryVectorStore::new(1024)),
        Arc::new(StubEmbeddingService::new()),
        Arc::new(StubSchemaFetcher::new()),
    ))
}

fn vpc_resource(name: &str, attrs: BTreeMap<String, AttributeValue>) -> MappedResource {
    MappedResource {
        source_addr: format!("google_compute_network.{}", name),
        target_addr: format!("aws_vpc.{}", name),
        target_type: "aws_vpc".to_string(),
        target_name: name.to_string(),
        attributes: attrs,
        dependencies: vec![],
    }
}

fn make_plan(resources: Vec<MappedResource>) -> MappingPlan {
    MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "google".to_string(),
        target_provider: "aws".to_string(),
        resources,
    }
}

fn s(v: &str) -> AttributeValue {
    AttributeValue::String(v.to_string())
}

// ─────────────────────────────────────────────────────────────────────────
// Test 1 — Valid plan passes (criterion #1)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn valid_plan_passes() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), s("10.0.0.0/16"));
    attrs.insert("instance_tenancy".to_string(), s("default"));

    let plan = make_plan(vec![vpc_resource("main", attrs)]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(
        report.passed,
        "valid plan must pass: errors={:?}",
        report.errors
    );
    assert!(report.errors.is_empty());
    assert!(
        report.warnings.is_empty(),
        "no deprecated/computed attrs set; warnings should be empty: {:?}",
        report.warnings
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 2 — Hallucinated attribute fails (Article III, criterion #2)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn hallucinated_attribute_fails_with_article_iii_message() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), s("10.0.0.0/16"));
    // "cidr_blocks" (plural) — Mapper hallucinated this.
    attrs.insert("cidr_blocks".to_string(), s("10.0.0.0/16"));

    let plan = make_plan(vec![vpc_resource("main", attrs)]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(!report.passed, "hallucinated attr must block");
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::UnknownAttribute { attr, .. } if attr == "cidr_blocks"
    )));
    let msg = format!("{}", report.errors[0]);
    assert!(
        msg.contains("Article III"),
        "error message cites Article III: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Test 3 — Missing required attribute fails (Article IV, criterion #3)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn missing_required_attribute_fails() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    // No cidr_block — but the schema marks it required.
    let mut attrs = BTreeMap::new();
    attrs.insert("instance_tenancy".to_string(), s("default"));

    let plan = make_plan(vec![vpc_resource("main", attrs)]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(!report.passed);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::MissingRequiredAttribute { attr, .. } if attr == "cidr_block"
    )));
}

// ─────────────────────────────────────────────────────────────────────────
// Test 4 — Unknown resource type fails (criterion #4)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn unknown_resource_type_fails() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    let mut attrs = BTreeMap::new();
    attrs.insert("foo".to_string(), s("bar"));
    let bogus = MappedResource {
        source_addr: "x.y".to_string(),
        target_addr: "aws_buckets.main".to_string(), // hallucinated type
        target_type: "aws_buckets".to_string(),
        target_name: "main".to_string(),
        attributes: attrs,
        dependencies: vec![],
    };

    let plan = make_plan(vec![bogus]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(!report.passed);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::UnknownResourceType { target_type, .. } if target_type == "aws_buckets"
    )));
    // Per the early-continue branch in validate(): no per-attr errors
    // for unknown types (we can't tell what's valid without a schema).
    assert!(!report
        .errors
        .iter()
        .any(|e| matches!(e, ValidationError::UnknownAttribute { .. })));
}

// ─────────────────────────────────────────────────────────────────────────
// Test 5 — Deprecated attribute warns, doesn't block (criterion #5)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn deprecated_attribute_warns_not_blocks() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), s("10.0.0.0/16"));
    attrs.insert(
        "enable_classiclink".to_string(),
        AttributeValue::Bool(true), // deprecated since 5.0.0 in our test schema
    );

    let plan = make_plan(vec![vpc_resource("main", attrs)]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(report.passed, "deprecated attr is non-blocking");
    assert!(report.errors.is_empty());
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        ValidationWarning::DeprecatedAttribute { attr, since, .. }
            if attr == "enable_classiclink" && since == "5.0.0"
    )));
}

// ─────────────────────────────────────────────────────────────────────────
// Test 6 — Multiple errors aggregate (criterion #6)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn multiple_errors_aggregate_not_fail_fast() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    // Resource 1: hallucinated attribute
    let mut a1 = BTreeMap::new();
    a1.insert("cidr_block".to_string(), s("10.0.0.0/16"));
    a1.insert("hallucinated_field_one".to_string(), s("x"));

    // Resource 2: missing required
    let mut a2 = BTreeMap::new();
    a2.insert("instance_tenancy".to_string(), s("default")); // missing cidr_block

    // Resource 3: unknown type
    let bogus = MappedResource {
        source_addr: "x.z".to_string(),
        target_addr: "aws_widgets.foo".to_string(),
        target_type: "aws_widgets".to_string(),
        target_name: "foo".to_string(),
        attributes: BTreeMap::new(),
        dependencies: vec![],
    };

    let plan = make_plan(vec![vpc_resource("a", a1), vpc_resource("b", a2), bogus]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(!report.passed);
    assert!(
        report.errors.len() >= 3,
        "aggregate at least 3 errors (one per resource): got {}",
        report.errors.len()
    );

    // Every error type appears.
    let mut saw_unknown_type = false;
    let mut saw_unknown_attr = false;
    let mut saw_missing_required = false;
    for e in &report.errors {
        match e {
            ValidationError::UnknownResourceType { .. } => saw_unknown_type = true,
            ValidationError::UnknownAttribute { .. } => saw_unknown_attr = true,
            ValidationError::MissingRequiredAttribute { .. } => saw_missing_required = true,
        }
    }
    assert!(saw_unknown_type, "expect UnknownResourceType");
    assert!(saw_unknown_attr, "expect UnknownAttribute");
    assert!(saw_missing_required, "expect MissingRequiredAttribute");
}

// ─────────────────────────────────────────────────────────────────────────
// Test 7 — Setting a computed attribute warns (criterion #5 cont.)
// ─────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn setting_computed_attribute_warns() {
    let knowledge = make_knowledge_with_aws_schema().await;
    let validator = Validator::new(knowledge);

    let mut attrs = BTreeMap::new();
    attrs.insert("cidr_block".to_string(), s("10.0.0.0/16"));
    // arn is computed in our test schema → setting it should warn.
    attrs.insert("arn".to_string(), s("arn:aws:ec2:us-east-1::vpc/xxx"));

    let plan = make_plan(vec![vpc_resource("main", attrs)]);
    let report = validator.validate(&plan, "5.30.0").await.unwrap();

    assert!(report.passed, "setting computed is non-blocking");
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        ValidationWarning::SetComputedAttribute { attr, .. } if attr == "arn"
    )));
}
