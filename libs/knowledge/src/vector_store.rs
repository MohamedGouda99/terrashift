//! In-memory vector store with cosine similarity for RAG retrieval.
//!
//! Pattern: terrashift_plan.md §7 (Knowledge service multi-tier; Stage 1 uses
//! in-memory, Stage 3 swaps in LanceDB without changing the trait surface).
//! Constitution: Article XII rule 2 (cache-first — vector search before LLM).
//!
//! ## Why in-memory for Stage 1
//!
//! AWS provider has ~1,400 resources, Azure ~900, GCP ~400. Total ~2,700
//! vectors at 1,024 dims = ~10MB. Brute-force cosine over all of them is
//! microseconds. LanceDB would add operational complexity without measurable
//! benefit at this scale.
//!
//! ## When to swap to LanceDB (S17)
//!
//! - >50K vectors (per-attribute embeddings, not just per-resource)
//! - Persistent index that survives restarts (mmap)
//! - Approximate nearest-neighbor for sub-linear search

use crate::embedding::EmbeddingError;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::RwLock;
use thiserror::Error;
use tracing::instrument;

#[derive(Debug, Error)]
pub enum VectorStoreError {
    #[error("dimension mismatch: store={store_dim}, vector={vector_dim}")]
    DimensionMismatch { store_dim: usize, vector_dim: usize },

    #[error("embedding error: {0}")]
    Embedding(#[from] EmbeddingError),

    #[error("not found: id={0}")]
    NotFound(String),
}

/// One row in the vector store: an opaque ID + its embedding + free-form metadata.
///
/// Metadata is `BTreeMap<String, String>` (not `serde_json::Value`) for two
/// reasons: (1) lighter on memory, (2) ordered iteration for deterministic
/// test output.
#[derive(Debug, Clone)]
pub struct VectorRow {
    pub id: String,
    pub embedding: Vec<f32>,
    pub metadata: BTreeMap<String, String>,
}

/// Search hit: ID, similarity score (0..1), original metadata.
#[derive(Debug, Clone)]
pub struct VectorHit {
    pub id: String,
    pub score: f32,
    pub metadata: BTreeMap<String, String>,
}

/// Store-and-search trait. Stage 1: `InMemoryVectorStore`. Stage 3+:
/// `LanceDbVectorStore` (S17).
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Vector dimension. All rows + queries must match.
    fn dimension(&self) -> usize;

    /// Insert (or replace) rows. Idempotent on `id`.
    async fn upsert(&self, rows: Vec<VectorRow>) -> Result<(), VectorStoreError>;

    /// Find top-K rows most similar to `query`. Cosine similarity assumed
    /// (rows + query should be L2-normalized; the stub embedding service
    /// guarantees this). Returns hits in descending score order.
    async fn search(&self, query: &[f32], top_k: usize)
        -> Result<Vec<VectorHit>, VectorStoreError>;

    /// Total row count (for tests + diagnostics).
    async fn len(&self) -> usize;

    /// Whether the store is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// Brute-force cosine-similarity store. Suitable for ≤10k vectors.
pub struct InMemoryVectorStore {
    dim: usize,
    rows: RwLock<Vec<VectorRow>>,
}

impl InMemoryVectorStore {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            rows: RwLock::new(Vec::new()),
        }
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    fn dimension(&self) -> usize {
        self.dim
    }

    #[instrument(skip(self, rows), fields(count = rows.len()))]
    async fn upsert(&self, rows: Vec<VectorRow>) -> Result<(), VectorStoreError> {
        for row in &rows {
            if row.embedding.len() != self.dim {
                return Err(VectorStoreError::DimensionMismatch {
                    store_dim: self.dim,
                    vector_dim: row.embedding.len(),
                });
            }
        }
        let mut store = self.rows.write().map_err(|_| {
            VectorStoreError::Embedding(EmbeddingError::Model("rwlock poisoned".to_string()))
        })?;
        for new_row in rows {
            // Replace if id matches; else push
            if let Some(existing) = store.iter_mut().find(|r| r.id == new_row.id) {
                *existing = new_row;
            } else {
                store.push(new_row);
            }
        }
        Ok(())
    }

    #[instrument(skip(self, query))]
    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>, VectorStoreError> {
        if query.len() != self.dim {
            return Err(VectorStoreError::DimensionMismatch {
                store_dim: self.dim,
                vector_dim: query.len(),
            });
        }
        let store = self.rows.read().map_err(|_| {
            VectorStoreError::Embedding(EmbeddingError::Model("rwlock poisoned".to_string()))
        })?;
        let mut scored: Vec<VectorHit> = store
            .iter()
            .map(|r| VectorHit {
                id: r.id.clone(),
                score: cosine_similarity(query, &r.embedding),
                metadata: r.metadata.clone(),
            })
            .collect();
        // Descending score
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        Ok(scored)
    }

    async fn len(&self) -> usize {
        match self.rows.read() {
            Ok(g) => g.len(),
            Err(_) => 0,
        }
    }
}

/// Cosine similarity assuming L2-normalized inputs (== dot product).
/// Used by `search`. If inputs aren't normalized, results aren't true cosine
/// but still consistent enough for top-K ordering.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, v: Vec<f32>) -> VectorRow {
        VectorRow {
            id: id.to_string(),
            embedding: v,
            metadata: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn upsert_and_search() {
        let store = InMemoryVectorStore::new(3);
        store
            .upsert(vec![
                row("a", vec![1.0, 0.0, 0.0]),
                row("b", vec![0.0, 1.0, 0.0]),
                row("c", vec![0.0, 0.0, 1.0]),
            ])
            .await
            .expect("upsert");

        // Query close to "a"
        let hits = store.search(&[0.9, 0.1, 0.0], 2).await.expect("search");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "a");
    }

    #[tokio::test]
    async fn upsert_dedup_by_id() {
        let store = InMemoryVectorStore::new(3);
        store
            .upsert(vec![row("a", vec![1.0, 0.0, 0.0])])
            .await
            .expect("ok");
        store
            .upsert(vec![row("a", vec![0.0, 1.0, 0.0])])
            .await
            .expect("ok");
        assert_eq!(store.len().await, 1);

        let hits = store.search(&[0.0, 1.0, 0.0], 1).await.expect("search");
        assert_eq!(hits[0].id, "a");
        assert!((hits[0].score - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn dimension_mismatch_loud() {
        let store = InMemoryVectorStore::new(3);
        let err = store
            .upsert(vec![row("bad", vec![1.0, 2.0])])
            .await
            .expect_err("expected err");
        assert!(matches!(err, VectorStoreError::DimensionMismatch { .. }));
    }
}
