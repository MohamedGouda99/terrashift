//! `KnowledgeService` — the Mapper-facing facade combining storage + embeddings + retrieval.
//!
//! Pattern: terrashift_plan.md §7 (multi-tier knowledge architecture).
//! Constitution: Article XII rule 2 (cache-first: vector search → cold LLM only on miss).
//!
//! ## What this is
//!
//! The single API the Mapper (P-05, S4) calls:
//!
//! ```ignore
//! let candidates = knowledge.find_similar_resources("aws_vpc -> azure equivalent", 5).await?;
//! ```
//!
//! Returns the top-K most semantically similar resources from the cache,
//! along with their full schemas. Mapper passes ONLY those to the LLM —
//! never all 1,400 AWS resources or all 900 Azure resources.
//!
//! ## How it composes
//!
//! ```text
//!   query: "aws_vpc -> azure equivalent"
//!         │
//!         ▼
//!   EmbeddingService.embed(query)   ← pure compute (StubEmbedding for now)
//!         │
//!         ▼
//!   VectorStore.search(top_k=5)     ← cosine over in-memory vectors
//!         │
//!         ▼  hits with resource IDs
//!   SchemaStore.fetch_provider_schema(per hit)  ← SQLite JSON
//!         │
//!         ▼
//!   Mapper LLM context (only top-K, not the full universe)
//! ```

use crate::embedding::EmbeddingService;
use crate::errors::SchemaError;
use crate::registry_client::{RegistryError, SchemaFetcher};
use crate::schema_store::SchemaStore;
use crate::types::{MappingExample, ProviderSchema, ResourceSchema};
use crate::vector_store::{VectorRow, VectorStore, VectorStoreError};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, instrument, warn};

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("schema store error: {0}")]
    Schema(#[from] SchemaError),

    #[error("vector store error: {0}")]
    Vector(#[from] VectorStoreError),

    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),
}

/// One retrieval hit with the full resource schema attached.
#[derive(Debug, Clone)]
pub struct ResourceMatch {
    pub provider: String,
    pub provider_version: String,
    pub resource_type: String,
    pub schema: ResourceSchema,
    pub similarity: f32,
}

/// Composed RAG service. Construct once, share across the app via Arc.
pub struct KnowledgeService {
    pub schema_store: Arc<dyn SchemaStore>,
    pub vector_store: Arc<dyn VectorStore>,
    pub embedding_service: Arc<dyn EmbeddingService>,
    pub schema_fetcher: Arc<dyn SchemaFetcher>,
}

impl KnowledgeService {
    pub fn new(
        schema_store: Arc<dyn SchemaStore>,
        vector_store: Arc<dyn VectorStore>,
        embedding_service: Arc<dyn EmbeddingService>,
        schema_fetcher: Arc<dyn SchemaFetcher>,
    ) -> Self {
        Self {
            schema_store,
            vector_store,
            embedding_service,
            schema_fetcher,
        }
    }

    /// One-shot sync: fetch a provider schema via the SchemaFetcher, store
    /// the JSON in SchemaStore, embed each resource description, populate
    /// the VectorStore. Idempotent on (provider, version).
    ///
    /// This is what `terrashift sync --providers aws --version 5.30.0` calls
    /// (S6 wires the CLI command).
    #[instrument(skip(self), fields(namespace, name, version))]
    pub async fn sync_provider(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<usize, KnowledgeError> {
        // 1. Fetch schema (via terraform CLI subprocess in production, or stub in tests)
        let schema = self.schema_fetcher.fetch(namespace, name, version).await?;

        let resource_count = schema.resources.len();
        info!(
            "fetched {} resources for {namespace}/{name}@{version}",
            resource_count
        );

        // 2. Cache JSON
        self.schema_store.cache_schema(&schema).await?;

        // 3. Build embedding texts: "resource_name: description"
        let mut texts: Vec<String> = Vec::with_capacity(resource_count);
        let mut ids: Vec<String> = Vec::with_capacity(resource_count);
        for (rname, rschema) in &schema.resources {
            let text = format!(
                "{rname}: {}",
                rschema.description.as_deref().unwrap_or("(no description)")
            );
            texts.push(text);
            ids.push(format!("{}@{}::{}", schema.provider, schema.version, rname));
        }

        if texts.is_empty() {
            warn!("provider {name}@{version} has no resources to embed");
            return Ok(0);
        }

        // 4. Embed all in one batch
        let vectors = self
            .embedding_service
            .embed_batch(&texts)
            .await
            .map_err(VectorStoreError::Embedding)?;

        // 5. Upsert into vector store with metadata for hit-to-schema lookup
        let rows: Vec<VectorRow> = ids
            .into_iter()
            .zip(vectors)
            .zip(schema.resources.keys())
            .map(|((id, embedding), rname)| {
                let mut metadata = BTreeMap::new();
                metadata.insert("provider".to_string(), schema.provider.clone());
                metadata.insert("version".to_string(), schema.version.clone());
                metadata.insert("resource_type".to_string(), rname.clone());
                VectorRow {
                    id,
                    embedding,
                    metadata,
                }
            })
            .collect();

        self.vector_store.upsert(rows).await?;

        Ok(resource_count)
    }

    /// Mapper-facing: find resources semantically similar to `query`. Returns
    /// top-K hits with full schemas attached.
    ///
    /// `query` is free-text — typical Mapper query: `"aws_vpc azure equivalent"`,
    /// or `"S3 bucket Azure storage account similar"`.
    #[instrument(skip(self, query), fields(top_k))]
    pub async fn find_similar_resources(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<ResourceMatch>, KnowledgeError> {
        // 1. Embed the query
        let qvec = self
            .embedding_service
            .embed(query)
            .await
            .map_err(VectorStoreError::Embedding)?;

        // 2. Top-K vector search
        let hits = self.vector_store.search(&qvec, top_k).await?;

        // 3. Resolve each hit to its full ResourceSchema via SchemaStore
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let provider = hit.metadata.get("provider").cloned().unwrap_or_default();
            let version = hit.metadata.get("version").cloned().unwrap_or_default();
            let rtype = hit
                .metadata
                .get("resource_type")
                .cloned()
                .unwrap_or_default();

            // Cache hit: we already stored the full ProviderSchema; pull it out.
            let schema = self
                .schema_store
                .fetch_provider_schema(&provider, &version)
                .await?;

            if let Some(rschema) = schema.resources.get(&rtype) {
                results.push(ResourceMatch {
                    provider: provider.clone(),
                    provider_version: version.clone(),
                    resource_type: rtype,
                    schema: rschema.clone(),
                    similarity: hit.score,
                });
            } else {
                warn!(
                    "vector hit references missing resource: provider={provider} version={version} type={rtype}"
                );
            }
        }

        Ok(results)
    }

    /// Convenience: find candidates filtered to a specific provider.
    /// Useful for "given source resource X (AWS), find Azure equivalents."
    pub async fn find_similar_in_provider(
        &self,
        query: &str,
        target_provider: &str,
        top_k: usize,
    ) -> Result<Vec<ResourceMatch>, KnowledgeError> {
        // Stage 1: search broadly, filter post-hoc. Stage 3 will use VectorStore
        // metadata-aware filtering for efficiency at large scales.
        let raw = self.find_similar_resources(query, top_k * 4).await?;
        let filtered: Vec<ResourceMatch> = raw
            .into_iter()
            .filter(|m| m.provider == target_provider)
            .take(top_k)
            .collect();
        Ok(filtered)
    }

    /// Direct mapping examples from the curated corpus (Stage 3+ has data;
    /// Stage 1 returns the SchemaStore's stub which is empty).
    pub async fn search_curated_mappings(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<MappingExample>, KnowledgeError> {
        Ok(self.schema_store.search_mappings(query, top_k).await?)
    }

    /// Direct schema fetch — bypasses RAG when the Mapper already knows
    /// exactly which (provider, version, resource_type) it wants.
    pub async fn fetch_schema(
        &self,
        provider: &str,
        version: &str,
    ) -> Result<ProviderSchema, KnowledgeError> {
        Ok(self
            .schema_store
            .fetch_provider_schema(provider, version)
            .await?)
    }
}
