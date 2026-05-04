// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

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

    /// Bootstrap path — load a `ProviderSchema` directly from a bundled
    /// or pre-fetched source (skipping `SchemaFetcher`). Does the same
    /// cache-then-embed-then-vector-upsert pipeline as `sync_provider`,
    /// just sourced from JSON or in-memory data instead of the network.
    ///
    /// Use case: `seed_from_bundle()` and CLI `terrashift schemas import`
    /// (S17a). Avoids hitting the registry on cold-start when the
    /// operator has already curated schemas (e.g., from CloudForge).
    #[instrument(skip(self, schema), fields(provider = %schema.provider, version = %schema.version))]
    pub async fn seed_provider(&self, schema: ProviderSchema) -> Result<usize, KnowledgeError> {
        let resource_count = schema.resources.len();
        info!(
            "seeding {} resources for {}@{}",
            resource_count, schema.provider, schema.version
        );

        // 1. Cache JSON
        self.schema_store.cache_schema(&schema).await?;

        // 2. Build embedding texts
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
            warn!(
                "seed: provider {}@{} has no resources to embed",
                schema.provider, schema.version
            );
            return Ok(0);
        }

        // 3. Embed in batch
        let vectors = self
            .embedding_service
            .embed_batch(&texts)
            .await
            .map_err(VectorStoreError::Embedding)?;

        // 4. Upsert into vector store
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

    /// Walk a seed bundle laid out as `<dir>/<provider>/<category>/<resource>.json`
    /// and seed the knowledge layer. Each leaf JSON file is one
    /// `ResourceSchema`. Files are grouped by their parent provider
    /// directory and emitted as one `ProviderSchema` per provider, so
    /// downstream lookups by `(provider, version)` work uniformly.
    ///
    /// Idempotent: `cache_schema` uses `INSERT OR IGNORE`, so re-running
    /// is a no-op when versions match. Misformatted JSON files are
    /// warn-skipped, never fatal.
    ///
    /// Layout example:
    /// ```text
    /// seed/
    /// ├── aws/
    /// │   └── networking/
    /// │       ├── aws_vpc.json
    /// │       └── aws_subnet.json
    /// └── azurerm/
    ///     ├── networking/
    ///     │   └── azurerm_virtual_network.json
    ///     └── security/
    ///         └── azurerm_resource_group.json
    /// ```
    ///
    /// Returns the total resource count across all loaded providers.
    pub async fn seed_from_bundle(&self, dir: &std::path::Path) -> Result<usize, KnowledgeError> {
        if !dir.exists() {
            warn!("seed bundle dir does not exist: {}", dir.display());
            return Ok(0);
        }

        let provider_entries = std::fs::read_dir(dir).map_err(|e| {
            KnowledgeError::Schema(crate::errors::SchemaError::Storage(
                sqlx::Error::Configuration(format!("read seed dir: {e}").into()),
            ))
        })?;

        let mut total = 0_usize;
        for provider_entry in provider_entries.flatten() {
            let provider_path = provider_entry.path();
            if !provider_path.is_dir() {
                continue;
            }
            let provider_name = match provider_path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let mut resources: BTreeMap<String, crate::types::ResourceSchema> = BTreeMap::new();
            walk_resource_jsons(&provider_path, &mut resources);

            if resources.is_empty() {
                continue;
            }

            let schema = ProviderSchema {
                provider: provider_name.clone(),
                version: SEED_VERSION.to_string(),
                resources,
                data_sources: BTreeMap::new(),
                fetched_at: chrono::Utc::now(),
            };

            let n = self.seed_provider(schema).await?;
            info!(
                "seed: loaded {} resources for provider '{}'",
                n, provider_name
            );
            total += n;
        }

        Ok(total)
    }

    /// First-launch boot: load the bundled seed (always), then for each
    /// `(namespace, name, version)` in `providers_to_fetch` that isn't
    /// already cached, fetch from the registry via the configured
    /// `SchemaFetcher` and seed it. Idempotent — re-running on a
    /// populated cache is a near-no-op.
    ///
    /// The registry-fetched schemas are STORED ALONGSIDE the seed (different
    /// version row in SQLite) — `terrashift-seed-2025.01` for the bundled
    /// curated catalog vs the real provider version (e.g., `5.30.0`) for
    /// the registry pull. Mapper retrieval queries by `provider` only,
    /// so both versions surface in candidate hits and the bigger /
    /// more-current registry pull wins on coverage.
    ///
    /// Returns `(seed_count, fetched_provider_resource_counts)` so the
    /// caller can log progress.
    pub async fn first_launch_sync(
        &self,
        seed_dir: &std::path::Path,
        providers_to_fetch: &[(String, String, String)],
    ) -> Result<FirstLaunchReport, KnowledgeError> {
        // 1. Always load the seed first (instant, offline-capable).
        let seed_count = self.seed_from_bundle(seed_dir).await?;

        // 2. For each (namespace, name, version), check cache; fetch if missing.
        let mut per_provider: Vec<(String, String, usize, FetchOutcome)> =
            Vec::with_capacity(providers_to_fetch.len());
        for (namespace, name, version) in providers_to_fetch {
            let already_cached = self
                .schema_store
                .fetch_provider_schema(name, version)
                .await
                .is_ok();

            if already_cached {
                info!("first_launch: {name}@{version} already cached; skipping fetch");
                per_provider.push((
                    name.clone(),
                    version.clone(),
                    0,
                    FetchOutcome::AlreadyCached,
                ));
                continue;
            }

            info!("first_launch: fetching {namespace}/{name}@{version} from registry...");
            match self.schema_fetcher.fetch(namespace, name, version).await {
                Ok(schema) => {
                    let n = self.seed_provider(schema).await?;
                    info!(
                        "first_launch: fetched + cached {} resources for {name}@{version}",
                        n
                    );
                    per_provider.push((name.clone(), version.clone(), n, FetchOutcome::Fetched));
                }
                Err(err) => {
                    warn!(
                        "first_launch: failed to fetch {name}@{version}: {err} \
                         (continuing — seed coverage still active)"
                    );
                    per_provider.push((
                        name.clone(),
                        version.clone(),
                        0,
                        FetchOutcome::FetchFailed(err.to_string()),
                    ));
                }
            }
        }

        Ok(FirstLaunchReport {
            seed_resource_count: seed_count,
            per_provider,
        })
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

/// What `first_launch_sync` returns. Lets the caller log progress
/// without scraping log lines.
#[derive(Debug, Clone)]
pub struct FirstLaunchReport {
    pub seed_resource_count: usize,
    /// One entry per `(namespace, name, version)` requested. The
    /// `usize` is the resource count fetched from the registry; `0`
    /// when the cache already had the schema or the fetch failed.
    pub per_provider: Vec<(String, String, usize, FetchOutcome)>,
}

#[derive(Debug, Clone)]
pub enum FetchOutcome {
    /// `(provider, version)` was already in `schema_store`; skipped.
    AlreadyCached,
    /// Fetched from registry and cached. Resource count in the
    /// `usize` field of the parent tuple.
    Fetched,
    /// Registry fetch failed. Operator decides whether to retry; the
    /// seed still provides baseline coverage.
    FetchFailed(String),
}

// ─────────────────────────────────────────────────────────────────────
// Seed bundle helpers
// ─────────────────────────────────────────────────────────────────────

/// Version label stamped on bundled-seed `ProviderSchema` rows.
/// Distinct from real Terraform provider versions (e.g., `5.30.0`)
/// because the seed is a curated subset, not a 1:1 registry dump.
/// Operators can still pin against real versions via
/// `terrashift schemas refresh --provider aws --version 5.30.0`,
/// which fetches from the live registry and supersedes the seed.
const SEED_VERSION: &str = "terrashift-seed-2025.01";

/// Recursively walk a provider directory, parse every leaf `*.json`
/// as a `ResourceSchema`, and accumulate into the resources map keyed
/// by `resource.name`. Misformatted files are warn-skipped.
fn walk_resource_jsons(
    dir: &std::path::Path,
    resources: &mut BTreeMap<String, crate::types::ResourceSchema>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            warn!("seed: cannot read {}: {err}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_resource_jsons(&path, resources);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                warn!("seed: skip {} (read error: {e})", path.display());
                continue;
            }
        };
        let resource: crate::types::ResourceSchema = match serde_json::from_str(&json) {
            Ok(r) => r,
            Err(e) => {
                warn!("seed: skip {} (parse error: {e})", path.display());
                continue;
            }
        };
        resources.insert(resource.name.clone(), resource);
    }
}
