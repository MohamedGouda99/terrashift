// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! End-to-end RAG pipeline test using stub fetcher + stub embeddings.
//!
//! Verifies:
//!   1. sync_provider: fetch → cache JSON → embed → populate vector store
//!   2. find_similar_resources: query → embed → vector search → return ResourceMatches
//!   3. find_similar_in_provider: filter by provider
//!   4. Multi-provider sync + cross-cloud retrieval

use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::Arc;
use terrashift_knowledge::{
    AttributeSchema, InMemoryVectorStore, KnowledgeService, LocalSchemaStore, ProviderSchema,
    ResourceSchema, StubEmbeddingService, StubSchemaFetcher, DEFAULT_EMBEDDING_DIM,
};

fn aws_schema() -> ProviderSchema {
    let mut resources = BTreeMap::new();
    resources.insert(
        "aws_vpc".to_string(),
        ResourceSchema {
            name: "aws_vpc".to_string(),
            description: Some("Provides a VPC resource — AWS virtual private cloud.".to_string()),
            attributes: btmap(&[("cidr_block", true)]),
        },
    );
    resources.insert(
        "aws_s3_bucket".to_string(),
        ResourceSchema {
            name: "aws_s3_bucket".to_string(),
            description: Some("Provides an S3 bucket resource.".to_string()),
            attributes: btmap(&[("bucket", false)]),
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

fn azure_schema() -> ProviderSchema {
    let mut resources = BTreeMap::new();
    resources.insert(
        "azurerm_virtual_network".to_string(),
        ResourceSchema {
            name: "azurerm_virtual_network".to_string(),
            description: Some(
                "Provides a virtual network resource — Azure VNet, the equivalent of AWS VPC."
                    .to_string(),
            ),
            attributes: btmap(&[("address_space", true)]),
        },
    );
    resources.insert(
        "azurerm_storage_account".to_string(),
        ResourceSchema {
            name: "azurerm_storage_account".to_string(),
            description: Some(
                "Provides an Azure storage account — equivalent of AWS S3 bucket.".to_string(),
            ),
            attributes: btmap(&[("name", true)]),
        },
    );
    ProviderSchema {
        provider: "azurerm".to_string(),
        version: "3.50.0".to_string(),
        resources,
        data_sources: BTreeMap::new(),
        fetched_at: Utc::now(),
    }
}

fn btmap(entries: &[(&str, bool)]) -> BTreeMap<String, AttributeSchema> {
    entries
        .iter()
        .map(|(name, required)| {
            (
                name.to_string(),
                AttributeSchema {
                    name: name.to_string(),
                    attribute_type: "string".to_string(),
                    required: *required,
                    optional: !*required,
                    computed: false,
                    sensitive: false,
                    deprecated: None,
                    description: None,
                },
            )
        })
        .collect()
}

async fn build_knowledge() -> KnowledgeService {
    let schema_store = Arc::new(LocalSchemaStore::in_memory().await.expect("schema store"));
    let vector_store = Arc::new(InMemoryVectorStore::new(DEFAULT_EMBEDDING_DIM));
    let embedding = Arc::new(StubEmbeddingService::new());
    let fetcher = Arc::new(
        StubSchemaFetcher::new()
            .with_schema("aws", "5.30.0", aws_schema())
            .with_schema("azurerm", "3.50.0", azure_schema()),
    );

    KnowledgeService::new(schema_store, vector_store, embedding, fetcher)
}

#[tokio::test]
async fn sync_populates_store_and_vectors() {
    let svc = build_knowledge().await;

    let count = svc
        .sync_provider("hashicorp", "aws", "5.30.0")
        .await
        .expect("sync");
    assert_eq!(count, 2, "AWS schema has 2 resources");

    // Schema is now in SchemaStore
    let schema = svc.fetch_schema("aws", "5.30.0").await.expect("fetch");
    assert!(schema.resources.contains_key("aws_vpc"));

    // Vectors are now in VectorStore
    assert_eq!(svc.vector_store.len().await, 2);
}

#[tokio::test]
async fn search_returns_top_k_with_full_schemas() {
    let svc = build_knowledge().await;
    svc.sync_provider("hashicorp", "aws", "5.30.0")
        .await
        .expect("aws sync");
    svc.sync_provider("hashicorp", "azurerm", "3.50.0")
        .await
        .expect("azure sync");

    let hits = svc
        .find_similar_resources("vpc network", 3)
        .await
        .expect("search");

    assert!(!hits.is_empty(), "expected at least one hit");
    assert!(hits.len() <= 3, "top_k must be respected");

    // Each hit has the full ResourceSchema attached
    for hit in &hits {
        assert!(!hit.resource_type.is_empty());
        assert_eq!(hit.schema.name, hit.resource_type);
    }
}

#[tokio::test]
async fn find_similar_in_provider_filters() {
    let svc = build_knowledge().await;
    svc.sync_provider("hashicorp", "aws", "5.30.0")
        .await
        .expect("aws");
    svc.sync_provider("hashicorp", "azurerm", "3.50.0")
        .await
        .expect("azure");

    let hits = svc
        .find_similar_in_provider("storage bucket", "azurerm", 5)
        .await
        .expect("filter search");

    for hit in &hits {
        assert_eq!(
            hit.provider, "azurerm",
            "filter should restrict to azurerm provider only"
        );
    }
}

#[tokio::test]
async fn multi_provider_sync_idempotent() {
    let svc = build_knowledge().await;
    svc.sync_provider("hashicorp", "aws", "5.30.0")
        .await
        .expect("first");
    svc.sync_provider("hashicorp", "aws", "5.30.0")
        .await
        .expect("second");
    // Vector store should still have just 2 entries (upsert dedup by id)
    assert_eq!(svc.vector_store.len().await, 2);
}
