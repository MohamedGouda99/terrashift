// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Embedding service — converts text to dense vectors for RAG retrieval.
//!
//! Pattern: terrashift_plan.md §6.10 + §7 (RAG over the Terraform Registry —
//! provider schemas are too large to fit in any LLM context window).
//! Constitution: Article XII rule 2 (cache-first; embeddings are the cache key).
//!
//! ## Stage 1 implementations
//!
//! - `StubEmbeddingService` — deterministic hash-based vectors. Fast, no
//!   downloads, no API keys. Used in tests + for "RAG plumbing works"
//!   verification. NOT useful for actual semantic search quality.
//!
//! ## Stage 1.5 / S4 swap-in
//!
//! - `FastEmbedService` — `fastembed` crate, BAAI/bge-m3 model. ~150MB
//!   one-time download. Real semantic search. Pure Rust + ONNX.
//! - `HuggingFaceEmbeddingService` — HF Inference API (requires HF_TOKEN).
//!   Free tier, no local model. Slower than fastembed but no model download.
//!
//! Trait stays stable across all impls; swap is a single config flip.

use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Default embedding dimension for Stage 1. Matches BAAI/bge-m3 (1024) so
/// the swap to real embeddings doesn't change downstream code shapes.
pub const DEFAULT_EMBEDDING_DIM: usize = 1024;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("embedding service not configured: {0}")]
    NotConfigured(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("model error: {0}")]
    Model(String),
}

/// Convert text to a fixed-dimension dense vector.
///
/// `embed_batch` is the primary API — embedding services are usually
/// throughput-optimized for batches (HF API has per-request overhead;
/// fastembed amortizes ONNX session setup).
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// Vector dimension this service produces. Must be constant for the lifetime.
    fn dimension(&self) -> usize;

    /// Embed one or more texts. Returns vectors in input order.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Convenience wrapper for single embedding. Default impl calls embed_batch.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let mut v = self.embed_batch(&[text.to_string()]).await?;
        v.pop().ok_or(EmbeddingError::Model(
            "embed_batch returned empty result".to_string(),
        ))
    }
}

/// Deterministic hash-based stand-in. Stage 1 default — gives every text a
/// stable vector based on its character histogram + length, so the RAG
/// pipeline can be tested end-to-end without a real model.
///
/// Quality is poor — two texts with similar character distributions look
/// similar even when semantically unrelated. Sufficient for "RAG plumbing
/// works" tests; insufficient for actual semantic mapping. Swap in
/// `FastEmbedService` for production.
pub struct StubEmbeddingService {
    dim: usize,
}

impl StubEmbeddingService {
    pub fn new() -> Self {
        Self {
            dim: DEFAULT_EMBEDDING_DIM,
        }
    }

    pub fn with_dimension(dim: usize) -> Self {
        Self { dim }
    }

    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0_f32; self.dim];

        // Character histogram into the first 256 dimensions
        let buckets = self.dim.min(256);
        for ch in text.chars() {
            let idx = (ch as usize) % buckets;
            v[idx] += 1.0;
        }

        // Length features into the remaining dimensions
        if self.dim > 256 {
            let len_bucket = (text.len() % (self.dim - 256)) + 256;
            v[len_bucket] = 1.0;
        }

        // Hash-based pseudo-random distribution into all dimensions for variety
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let h = hasher.finish();
        for (i, slot) in v.iter_mut().enumerate() {
            let bit = (h >> (i % 64)) & 1;
            *slot += bit as f32 * 0.1;
        }

        // L2 normalize so cosine similarity == dot product (faster + standard)
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        v
    }
}

impl Default for StubEmbeddingService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingService for StubEmbeddingService {
    fn dimension(&self) -> usize {
        self.dim
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_embeddings_are_normalized() {
        let svc = StubEmbeddingService::new();
        let v = svc.embed("aws_vpc").await.expect("embed ok");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "embedding should be L2-normalized; norm = {norm}"
        );
        assert_eq!(v.len(), DEFAULT_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn stub_embeddings_are_deterministic() {
        let svc = StubEmbeddingService::new();
        let a = svc.embed("aws_vpc").await.expect("ok");
        let b = svc.embed("aws_vpc").await.expect("ok");
        assert_eq!(a, b, "same text → same vector");
    }

    #[tokio::test]
    async fn stub_embeddings_differ_for_different_texts() {
        let svc = StubEmbeddingService::new();
        let a = svc.embed("aws_vpc").await.expect("ok");
        let b = svc.embed("azurerm_virtual_network").await.expect("ok");
        assert_ne!(a, b, "different texts should produce different vectors");
    }
}
