// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! `first_launch_sync` — integration tests covering the three paths
//! through the cache-then-fetch boot logic.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;
use terrashift_knowledge::vector_store::InMemoryVectorStore;
use terrashift_knowledge::{
    AttributeSchema, FetchOutcome, KnowledgeService, LocalSchemaStore, ProviderSchema,
    ResourceSchema, StubEmbeddingService, StubSchemaFetcher,
};

const VECTOR_DIM: usize = 384;

async fn build_service(fetcher: Arc<StubSchemaFetcher>) -> KnowledgeService {
    let store = Arc::new(LocalSchemaStore::in_memory().await.expect("schema store"));
    let vector = Arc::new(InMemoryVectorStore::new(VECTOR_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(VECTOR_DIM));
    KnowledgeService::new(store, vector, embedder, fetcher)
}

fn fake_schema(provider: &str, version: &str, resource_count: usize) -> ProviderSchema {
    let mut resources = BTreeMap::new();
    for i in 0..resource_count {
        let rname = format!("{provider}_resource_{i}");
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
                description: Some("Generated identifier".to_string()),
            },
        );
        resources.insert(
            rname.clone(),
            ResourceSchema {
                name: rname.clone(),
                description: Some(format!("Fake {provider} resource {i}")),
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

/// Build a synthetic seed directory in `<dir>/<provider>/<version>/schema.json`
/// shape so each test verifies the `seed_from_bundle` contract without
/// depending on real seed contents shipping in the tree (post RFC
/// schema-source-migration §4.6 — the categorised layout was retired).
///
/// Uses `<X>-seed` version suffixes so the seeded `(provider, version)` keys
/// never collide with the registry-fetch versions the tests subsequently
/// request — collisions would surface as `FetchOutcome::AlreadyCached`
/// instead of `FetchOutcome::Fetched`, hiding the assertion the tests are
/// trying to make.
fn populate_synthetic_seed(root: &std::path::Path) {
    let aws = fake_schema("aws", "1.0.0-seed", 4);
    let google = fake_schema("google", "1.0.0-seed", 3);
    let azurerm = fake_schema("azurerm", "1.0.0-seed", 2);

    for schema in [&aws, &google, &azurerm] {
        let dir = root.join(&schema.provider).join(&schema.version);
        std::fs::create_dir_all(&dir).expect("mkdir seed entry");
        let json = serde_json::to_string_pretty(schema).expect("serialize");
        std::fs::write(dir.join("schema.json"), json).expect("write schema.json");
    }
}

// ─────────────────────────────────────────────────────────────────────
// Path 1: cache empty → fetch succeeds → resources cached
// (also seeds a synthetic flat-per-version bundle so `seed_resource_count`
// is non-zero — pre-RFC tests asserted >0 against the real categorised
// bundle; that bundle is retired, so the assertion is now backed by a
// tempdir fixture each test populates itself.)
// ─────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn first_launch_fetches_when_cache_empty() {
    let stub =
        StubSchemaFetcher::new().with_schema("aws", "5.30.0", fake_schema("aws", "5.30.0", 7));
    let service = build_service(Arc::new(stub)).await;

    // CRITICAL: stub's "aws@5.30.0" key collides with the seed entry's
    // own "aws@5.30.0" — so `first_launch_sync` will see the seed-loaded
    // schema as "already cached" and skip the registry fetch. To exercise
    // the Fetched path, seed under different versions.
    let seed_root = TempDir::new().expect("seed root");
    {
        let s = fake_schema("aws", "1.0.0-seed", 4);
        let d = seed_root.path().join(&s.provider).join(&s.version);
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(
            d.join("schema.json"),
            serde_json::to_string_pretty(&s).unwrap(),
        )
        .expect("write");
    }

    let report = service
        .first_launch_sync(
            seed_root.path(),
            &[(
                "hashicorp".to_string(),
                "aws".to_string(),
                "5.30.0".to_string(),
            )],
        )
        .await
        .expect("sync ok");

    assert_eq!(report.seed_resource_count, 4);

    // The single requested provider was Fetched.
    assert_eq!(report.per_provider.len(), 1);
    let (name, version, count, outcome) = &report.per_provider[0];
    assert_eq!(name, "aws");
    assert_eq!(version, "5.30.0");
    assert_eq!(*count, 7, "fake schema had 7 resources");
    assert!(matches!(outcome, FetchOutcome::Fetched));
}

// ─────────────────────────────────────────────────────────────────────
// Path 2: same (provider, version) already cached → skipped on rerun
// ─────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn first_launch_skips_when_already_cached() {
    let stub = StubSchemaFetcher::new().with_schema(
        "azurerm",
        "3.116.0",
        fake_schema("azurerm", "3.116.0", 3),
    );
    let service = build_service(Arc::new(stub)).await;
    let seed_root = TempDir::new().expect("seed root");

    let providers = vec![(
        "hashicorp".to_string(),
        "azurerm".to_string(),
        "3.116.0".to_string(),
    )];

    // First call: should fetch.
    let first = service
        .first_launch_sync(seed_root.path(), &providers)
        .await
        .expect("first sync ok");
    assert!(matches!(first.per_provider[0].3, FetchOutcome::Fetched));

    // Second call: same (provider, version) is already cached → skip.
    let second = service
        .first_launch_sync(seed_root.path(), &providers)
        .await
        .expect("second sync ok");
    assert!(matches!(
        second.per_provider[0].3,
        FetchOutcome::AlreadyCached
    ));
    // Skipped fetches report 0 (not the original count); the schema is
    // still cached, which is what matters.
    assert_eq!(second.per_provider[0].2, 0);
}

// ─────────────────────────────────────────────────────────────────────
// Path 3: fetch fails → reported as FetchFailed; seed still loaded;
// other providers still attempted
// ─────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn first_launch_continues_on_per_provider_fetch_failure() {
    // Stub knows about aws but NOT google — google fetch will fail.
    let stub =
        StubSchemaFetcher::new().with_schema("aws", "5.30.0", fake_schema("aws", "5.30.0", 4));
    let service = build_service(Arc::new(stub)).await;

    let providers = vec![
        (
            "hashicorp".to_string(),
            "aws".to_string(),
            "5.30.0".to_string(),
        ),
        (
            "hashicorp".to_string(),
            "google".to_string(),
            "5.10.0".to_string(),
        ),
    ];

    let seed_root = TempDir::new().expect("seed root");
    populate_synthetic_seed(seed_root.path());

    let report = service
        .first_launch_sync(seed_root.path(), &providers)
        .await
        .expect("sync should not propagate per-provider fetch failure");

    // aws was fetched successfully.
    let aws = report
        .per_provider
        .iter()
        .find(|(n, _, _, _)| n == "aws")
        .expect("aws report present");
    assert!(matches!(aws.3, FetchOutcome::Fetched));
    assert_eq!(aws.2, 4);

    // google fetch failed but didn't kill the run.
    let google = report
        .per_provider
        .iter()
        .find(|(n, _, _, _)| n == "google")
        .expect("google report present");
    assert!(matches!(google.3, FetchOutcome::FetchFailed(_)));

    // Seed still loaded — populated by populate_synthetic_seed above.
    assert!(report.seed_resource_count > 0);
}

// ─────────────────────────────────────────────────────────────────────
// Path 4: empty providers list → seed-only path works
// ─────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn first_launch_with_empty_providers_only_loads_seed() {
    let stub = StubSchemaFetcher::new();
    let service = build_service(Arc::new(stub)).await;

    let seed_root = TempDir::new().expect("seed root");
    populate_synthetic_seed(seed_root.path());

    let report = service
        .first_launch_sync(seed_root.path(), &[])
        .await
        .expect("seed-only sync ok");

    assert!(
        report.seed_resource_count > 0,
        "synthetic seed bundle should populate"
    );
    assert!(report.per_provider.is_empty());
}

// ─────────────────────────────────────────────────────────────────────
// Path 5: missing seed dir → seed_count=0 but registry path still runs
// ─────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn first_launch_tolerates_missing_seed_dir() {
    let stub =
        StubSchemaFetcher::new().with_schema("aws", "5.30.0", fake_schema("aws", "5.30.0", 2));
    let service = build_service(Arc::new(stub)).await;

    let nowhere = TempDir::new().expect("tmp");
    let nonexistent = nowhere.path().join("does-not-exist");

    let report = service
        .first_launch_sync(
            &nonexistent,
            &[(
                "hashicorp".to_string(),
                "aws".to_string(),
                "5.30.0".to_string(),
            )],
        )
        .await
        .expect("sync ok even without seed dir");

    assert_eq!(report.seed_resource_count, 0);
    assert!(matches!(report.per_provider[0].3, FetchOutcome::Fetched));
}
