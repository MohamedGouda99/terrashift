//! Integration tests for the knowledge schema cache.

use chrono::Utc;
use std::collections::BTreeMap;
use terrashift_knowledge::{
    AttributeSchema, LocalSchemaStore, ProviderSchema, ResourceSchema, SchemaError, SchemaStore,
};

fn aws_vpc_schema(version: &str) -> ProviderSchema {
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
            description: Some("CIDR block for the VPC".to_string()),
        },
    );

    let mut resources = BTreeMap::new();
    resources.insert(
        "aws_vpc".to_string(),
        ResourceSchema {
            name: "aws_vpc".to_string(),
            description: Some("Provides a VPC resource.".to_string()),
            attributes: attrs,
        },
    );

    ProviderSchema {
        provider: "aws".to_string(),
        version: version.to_string(),
        resources,
        data_sources: BTreeMap::new(),
        fetched_at: Utc::now(),
    }
}

#[tokio::test]
async fn cache_and_retrieve_round_trip() {
    let store = LocalSchemaStore::in_memory()
        .await
        .unwrap_or_else(|e| panic!("open: {e}"));
    let schema = aws_vpc_schema("5.30.0");

    let inserted = store
        .cache_schema(&schema)
        .await
        .unwrap_or_else(|e| panic!("cache: {e}"));
    assert!(inserted, "first insert should be true");

    let got = store
        .fetch_provider_schema("aws", "5.30.0")
        .await
        .unwrap_or_else(|e| panic!("fetch: {e}"));

    assert_eq!(got.provider, "aws");
    assert_eq!(got.version, "5.30.0");
    assert!(got.resources.contains_key("aws_vpc"));
    assert!(got.resources["aws_vpc"].attributes["cidr_block"].required);
}

#[tokio::test]
async fn cache_miss_returns_typed_not_found() {
    let store = LocalSchemaStore::in_memory()
        .await
        .unwrap_or_else(|e| panic!("open: {e}"));

    let err = store
        .fetch_provider_schema("aws", "999.0.0")
        .await
        .err()
        .unwrap_or_else(|| panic!("expected NotFound"));

    match err {
        SchemaError::NotFound { provider, version } => {
            assert_eq!(provider, "aws");
            assert_eq!(version, "999.0.0");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn list_versions_sorted_desc() {
    let store = LocalSchemaStore::in_memory()
        .await
        .unwrap_or_else(|e| panic!("open: {e}"));

    for v in ["5.20.0", "5.30.0", "5.10.0"] {
        store
            .cache_schema(&aws_vpc_schema(v))
            .await
            .unwrap_or_else(|e| panic!("cache {v}: {e}"));
    }

    let versions = store
        .list_versions("aws")
        .await
        .unwrap_or_else(|e| panic!("list: {e}"));
    // Lexicographic descending — works for semver-like strings of equal width.
    assert_eq!(versions, vec!["5.30.0", "5.20.0", "5.10.0"]);
}

#[tokio::test]
async fn pinned_version_is_immutable() {
    let store = LocalSchemaStore::in_memory()
        .await
        .unwrap_or_else(|e| panic!("open: {e}"));

    let first = aws_vpc_schema("5.30.0");
    let inserted = store
        .cache_schema(&first)
        .await
        .unwrap_or_else(|e| panic!("first cache: {e}"));
    assert!(inserted);

    // Try to cache the same (provider, version) — should silently no-op
    let second = aws_vpc_schema("5.30.0");
    let inserted_again = store
        .cache_schema(&second)
        .await
        .unwrap_or_else(|e| panic!("second cache: {e}"));
    assert!(!inserted_again, "Article VI: pinned version is immutable");

    // First version's data still wins
    let got = store
        .fetch_provider_schema("aws", "5.30.0")
        .await
        .unwrap_or_else(|e| panic!("fetch: {e}"));
    assert_eq!(got.fetched_at, first.fetched_at);
}

#[tokio::test]
async fn search_mappings_returns_empty_stage1_stub() {
    let store = LocalSchemaStore::in_memory()
        .await
        .unwrap_or_else(|e| panic!("open: {e}"));

    let results = store
        .search_mappings("aws_vpc to azurerm", 5)
        .await
        .unwrap_or_else(|e| panic!("search: {e}"));
    assert!(
        results.is_empty(),
        "Stage 1 stub returns empty Vec; RAG arrives in S17"
    );
}
