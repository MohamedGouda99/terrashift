// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Seed bundle integrity tests — verify the FLAT-PER-VERSION layout
//! introduced by RFC schema-source-migration §4.6.
//!
//! Each test builds a synthetic seed in a fresh tempdir
//! (`seed/<provider>/<version>/schema.json`), runs `seed_from_bundle`,
//! and asserts the loader's contract:
//!
//! 1. Every well-formed `schema.json` parses and round-trips into the
//!    SchemaStore + VectorStore.
//! 2. Misformatted files are warn-skipped, never fatal.
//! 3. Idempotency: re-running on a populated cache is a no-op (vector
//!    store size unchanged; SchemaStore returns Ok(false) on duplicate
//!    `(provider, version)`).
//! 4. Provider-filter retrieval surfaces only the requested provider.
//! 5. Multi-version coexistence: two different versions of the same
//!    provider are independently cached.
//!
//! The categorised `seed/<provider>/<category>/<resource>.json` layout
//! and its CloudForge-derived contents were retired in this migration —
//! these tests no longer depend on real seed contents shipping in the
//! tree, only on the loader's behaviour against synthetic fixtures.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;
use terrashift_knowledge::{
    AttributeSchema, InMemoryVectorStore, KnowledgeService, LocalSchemaStore, ProviderSchema,
    ResourceSchema, StubEmbeddingService, StubSchemaFetcher, VectorStore, DEFAULT_EMBEDDING_DIM,
};

/// Build a `ProviderSchema` with `n` synthetic resources. Used by every
/// test below to populate a fixture without depending on real schema content.
fn synthetic_schema(provider: &str, version: &str, n: usize) -> ProviderSchema {
    let mut resources = BTreeMap::new();
    for i in 0..n {
        let name = format!("{provider}_resource_{i}");
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "id".to_string(),
            AttributeSchema {
                name: "id".to_string(),
                attribute_type: "string".to_string(),
                required: false,
                optional: false,
                computed: true,
                sensitive: false,
                deprecated: None,
                description: Some("identifier".to_string()),
            },
        );
        resources.insert(
            name.clone(),
            ResourceSchema {
                name: name.clone(),
                description: Some(format!("synthetic {i}")),
                attributes: attrs,
            },
        );
    }
    ProviderSchema {
        provider: provider.to_string(),
        version: version.to_string(),
        resources,
        data_sources: BTreeMap::new(),
        fetched_at: Utc::now(),
    }
}

/// Lay out `seed/<provider>/<version>/schema.json` under `root` containing
/// the given `ProviderSchema`. Mirrors what `cargo xtask capture-schemas`
/// produces for the production build.
fn write_seed_entry(root: &Path, schema: &ProviderSchema) {
    let dir = root.join(&schema.provider).join(&schema.version);
    std::fs::create_dir_all(&dir).expect("mkdir seed entry");
    let json = serde_json::to_string_pretty(schema).expect("serialize");
    std::fs::write(dir.join("schema.json"), json).expect("write schema.json");
}

async fn build_service() -> (KnowledgeService, Arc<InMemoryVectorStore>) {
    let store = Arc::new(LocalSchemaStore::in_memory().await.expect("schema store"));
    let vector = Arc::new(InMemoryVectorStore::new(DEFAULT_EMBEDDING_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(DEFAULT_EMBEDDING_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    let svc = KnowledgeService::new(store, vector.clone(), embedder, fetcher);
    (svc, vector)
}

#[tokio::test]
async fn empty_seed_dir_loads_zero_resources() {
    let dir = TempDir::new().expect("tempdir");
    let (svc, vector) = build_service().await;

    let total = svc
        .seed_from_bundle(dir.path())
        .await
        .expect("empty seed dir is not an error");

    assert_eq!(total, 0, "no schema.json files = 0 resources loaded");
    assert_eq!(vector.len().await, 0);
}

#[tokio::test]
async fn missing_seed_dir_is_warn_only_not_error() {
    let (svc, _vector) = build_service().await;
    let nowhere = TempDir::new().expect("tempdir");
    let nonexistent = nowhere.path().join("does-not-exist");

    let total = svc
        .seed_from_bundle(&nonexistent)
        .await
        .expect("missing dir must not be a hard error (warn-and-continue)");

    assert_eq!(total, 0);
}

#[tokio::test]
async fn flat_per_version_layout_loads_all_resources() {
    let dir = TempDir::new().expect("tempdir");
    write_seed_entry(dir.path(), &synthetic_schema("aws", "5.30.0", 7));
    write_seed_entry(dir.path(), &synthetic_schema("azurerm", "3.110.0", 5));
    write_seed_entry(dir.path(), &synthetic_schema("google", "5.40.2", 3));

    let (svc, vector) = build_service().await;
    let total = svc.seed_from_bundle(dir.path()).await.expect("seed");

    assert_eq!(total, 7 + 5 + 3, "every (provider, version) should load");
    assert_eq!(
        vector.len().await,
        15,
        "vector store should have one entry per loaded resource"
    );
}

#[tokio::test]
async fn malformed_schema_json_is_warn_skipped_not_fatal() {
    let dir = TempDir::new().expect("tempdir");
    // Valid neighbour
    write_seed_entry(dir.path(), &synthetic_schema("aws", "5.30.0", 4));
    // Malformed peer in a different version directory
    let bad_dir = dir.path().join("aws").join("99.99.99");
    std::fs::create_dir_all(&bad_dir).expect("mkdir bad");
    std::fs::write(bad_dir.join("schema.json"), b"{ this is not valid json }").expect("write bad");

    let (svc, vector) = build_service().await;
    let total = svc
        .seed_from_bundle(dir.path())
        .await
        .expect("malformed file must not abort the whole load");

    // Only the well-formed entry's resources surface.
    assert_eq!(total, 4);
    assert_eq!(vector.len().await, 4);
}

#[tokio::test]
async fn idempotent_on_rerun() {
    let dir = TempDir::new().expect("tempdir");
    write_seed_entry(dir.path(), &synthetic_schema("aws", "5.30.0", 6));

    let (svc, vector) = build_service().await;
    let n1 = svc.seed_from_bundle(dir.path()).await.expect("first seed");
    let after_first = vector.len().await;

    let _n2 = svc
        .seed_from_bundle(dir.path())
        .await
        .expect("second seed must not error");
    let after_second = vector.len().await;

    assert_eq!(after_first, n1, "vector count matches first-seed count");
    assert_eq!(
        after_first, after_second,
        "vector store is idempotent under re-seeding (upsert by id)"
    );
}

#[tokio::test]
async fn multiple_versions_of_same_provider_coexist() {
    let dir = TempDir::new().expect("tempdir");
    write_seed_entry(dir.path(), &synthetic_schema("aws", "5.20.0", 3));
    write_seed_entry(dir.path(), &synthetic_schema("aws", "5.30.0", 5));

    let (svc, _vector) = build_service().await;
    let total = svc.seed_from_bundle(dir.path()).await.expect("seed");

    assert_eq!(
        total,
        3 + 5,
        "both versions should load independently into the SchemaStore"
    );
}

#[tokio::test]
async fn find_similar_in_provider_after_seed_returns_provider_filtered_results() {
    let dir = TempDir::new().expect("tempdir");
    write_seed_entry(dir.path(), &synthetic_schema("aws", "5.30.0", 4));
    write_seed_entry(dir.path(), &synthetic_schema("azurerm", "3.110.0", 4));

    let (svc, _vector) = build_service().await;
    svc.seed_from_bundle(dir.path()).await.expect("seed");

    let hits = svc
        .find_similar_in_provider("any synthetic resource", "azurerm", 5)
        .await
        .expect("retrieve");

    assert!(!hits.is_empty(), "azurerm has 4 resources; expected hits");
    for h in &hits {
        assert_eq!(h.provider, "azurerm", "filter must exclude aws hits");
        assert!(!h.schema.attributes.is_empty(), "schema is hollow");
    }
}

#[tokio::test]
async fn non_provider_dirs_are_skipped_without_panic() {
    let dir = TempDir::new().expect("tempdir");
    // The new layout requires <provider>/<version>/schema.json. A bare file
    // at the root, a `scripts/` subdir, and a depth-1 directory without a
    // version subdir should all be ignored without erroring.
    std::fs::write(dir.path().join("manifest.toml"), b"# build pins").expect("manifest stub");
    std::fs::create_dir_all(dir.path().join("scripts")).expect("scripts");
    std::fs::create_dir_all(dir.path().join("aws").join("5.30.0")).expect("empty version dir");

    write_seed_entry(dir.path(), &synthetic_schema("google", "5.40.2", 2));

    let (svc, _vector) = build_service().await;
    let total = svc
        .seed_from_bundle(dir.path())
        .await
        .expect("non-conforming dirs must be ignored cleanly");

    assert_eq!(total, 2, "only the well-formed google entry should load");
}
