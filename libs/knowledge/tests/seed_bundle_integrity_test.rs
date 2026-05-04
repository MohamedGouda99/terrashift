// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Seed bundle integrity tests — ensure every JSON file in
//! `libs/knowledge/seed/<provider>/<category>/<resource>.json` parses
//! and round-trips through the full RAG pipeline.
//!
//! These tests catch:
//!   1. A new seed file with a typo in its JSON envelope
//!   2. A schema-shape change in `ResourceSchema` that breaks past seeds
//!   3. A break in `seed_from_bundle()` that silently drops resources
//!   4. A break in similarity retrieval against real seed content

use std::path::{Path, PathBuf};
use std::sync::Arc;
use terrashift_knowledge::{
    InMemoryVectorStore, KnowledgeService, LocalSchemaStore, ResourceSchema, StubEmbeddingService,
    StubSchemaFetcher, VectorStore, DEFAULT_EMBEDDING_DIM,
};

/// Resolve the seed directory relative to this crate's manifest, regardless
/// of where `cargo test` is invoked. Workspace layout: `libs/knowledge/seed`.
fn seed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("seed")
}

/// Walk the seed dir and return every leaf `.json` path under `<provider>/`.
/// (Excludes `seed/scripts/`, `seed/README.md`, and any non-provider dirs.)
fn collect_seed_jsons(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip scripts/ helper dir at any depth.
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "scripts" {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                out.push(path);
            }
        }
    }
    // Only descend into provider directories.
    for provider in &["aws", "azurerm", "google"] {
        let p = root.join(provider);
        if p.is_dir() {
            walk(&p, &mut out);
        }
    }
    out
}

#[test]
fn every_seed_json_parses_as_resource_schema() {
    let dir = seed_dir();
    assert!(
        dir.is_dir(),
        "seed directory missing: {} (run from workspace root)",
        dir.display()
    );

    let jsons = collect_seed_jsons(&dir);
    assert!(
        jsons.len() >= 100,
        "expected at least 100 seed files, found {} — bundle truncated?",
        jsons.len()
    );

    let mut bad: Vec<(PathBuf, String)> = Vec::new();
    for path in &jsons {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                bad.push((path.clone(), format!("read: {e}")));
                continue;
            }
        };
        match serde_json::from_str::<ResourceSchema>(&content) {
            Ok(rs) => {
                // Light shape check: name must match the file stem.
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(no stem)");
                if rs.name != stem {
                    bad.push((
                        path.clone(),
                        format!("name `{}` != file stem `{}`", rs.name, stem),
                    ));
                }
            }
            Err(e) => bad.push((path.clone(), format!("parse: {e}"))),
        }
    }
    assert!(
        bad.is_empty(),
        "{} seed file(s) failed integrity check:\n{}",
        bad.len(),
        bad.iter()
            .map(|(p, e)| format!("  {} — {}", p.display(), e))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn seed_counts_per_provider_match_filesystem() {
    let dir = seed_dir();
    let jsons = collect_seed_jsons(&dir);
    let mut per_provider: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for p in &jsons {
        // `seed/<provider>/<category>/<resource>.json` — provider is the
        // first segment after `seed/`.
        let rel = p.strip_prefix(&dir).unwrap();
        let provider = rel
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("(unknown)")
            .to_string();
        *per_provider.entry(provider).or_insert(0) += 1;
    }
    // Sanity: each of the three providers should be present.
    for p in &["aws", "azurerm", "google"] {
        let count = per_provider.get(*p).copied().unwrap_or(0);
        assert!(count > 0, "no seed entries for provider `{p}`");
    }
    eprintln!("seed counts: {per_provider:?}");
}

#[tokio::test]
async fn seed_from_bundle_loads_all_files_into_knowledge_service() {
    let store = Arc::new(LocalSchemaStore::in_memory().await.expect("schema store"));
    let vector = Arc::new(InMemoryVectorStore::new(DEFAULT_EMBEDDING_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(DEFAULT_EMBEDDING_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    let svc = KnowledgeService::new(store, vector.clone(), embedder, fetcher);

    let dir = seed_dir();
    let total = svc.seed_from_bundle(&dir).await.expect("seed_from_bundle");

    let fs_count = collect_seed_jsons(&dir).len();
    assert_eq!(
        total, fs_count,
        "loaded {total} resources but seed dir has {fs_count} files. \
         Difference indicates silently-skipped files (parse fail, name mismatch, etc.)"
    );
    assert_eq!(
        vector.len().await,
        fs_count,
        "vector store should have one entry per loaded resource"
    );
}

#[tokio::test]
async fn find_similar_in_provider_returns_provider_filtered_results() {
    let store = Arc::new(LocalSchemaStore::in_memory().await.expect("schema store"));
    let vector = Arc::new(InMemoryVectorStore::new(DEFAULT_EMBEDDING_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(DEFAULT_EMBEDDING_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    let svc = KnowledgeService::new(store, vector, embedder, fetcher);
    svc.seed_from_bundle(&seed_dir()).await.expect("seed");

    // Stub embedding is deterministic but content-agnostic, so we can't
    // assert *which* resource ranks #1 — only that the filter excludes
    // foreign providers from the results.
    let hits = svc
        .find_similar_in_provider("storage account blob bucket", "azurerm", 5)
        .await
        .expect("retrieve");

    assert!(
        !hits.is_empty(),
        "expected at least one azurerm hit; the seed has azurerm/storage/* entries"
    );
    for h in &hits {
        assert_eq!(
            h.provider, "azurerm",
            "find_similar_in_provider must filter by provider; got {h:?}"
        );
        assert!(!h.schema.attributes.is_empty(), "resource schema is hollow");
    }
}

#[tokio::test]
async fn seed_from_bundle_is_idempotent_on_rerun() {
    let store = Arc::new(LocalSchemaStore::in_memory().await.expect("schema store"));
    let vector = Arc::new(InMemoryVectorStore::new(DEFAULT_EMBEDDING_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(DEFAULT_EMBEDDING_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    let svc = KnowledgeService::new(store, vector.clone(), embedder, fetcher);

    let n1 = svc.seed_from_bundle(&seed_dir()).await.expect("first seed");
    let after_first = vector.len().await;

    let _n2 = svc
        .seed_from_bundle(&seed_dir())
        .await
        .expect("second seed");
    let after_second = vector.len().await;

    assert_eq!(
        after_first, after_second,
        "vector store should be idempotent under re-seeding (upsert by id)"
    );
    assert_eq!(
        after_first, n1,
        "vector count should match first-seed count"
    );
}
