// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Terrashift knowledge layer — Terraform Registry mirror + RAG retrieval.
//!
//! ## What this crate provides
//!
//! The Mapper (P-05, S4) needs to map source-cloud resources to target-cloud
//! resources. AWS has ~1,400 resources, Azure ~900, GCP ~400. Even on Llama
//! 3.3's 128K context window, you can't shove the entire target provider's
//! schema into one prompt. RAG retrieval is the binding architectural choice
//! per user direction (S2 brief).
//!
//! ## Layered architecture
//!
//! ```text
//!   KnowledgeService (Mapper-facing facade)
//!     │
//!     ├── SchemaStore         ← JSON storage of provider schemas (SQLite)
//!     ├── VectorStore         ← cosine-similarity retrieval (in-memory)
//!     ├── EmbeddingService    ← text → vector (Stub / FastEmbed / HF API)
//!     └── SchemaFetcher       ← Registry API + `terraform providers schema -json`
//! ```
//!
//! ## Stage 1 status (this crate, S2 commit)
//!
//! | Component | Stage 1 impl | Production swap-in |
//! |---|---|---|
//! | SchemaStore | `LocalSchemaStore` (sqlx + SQLite, JSON column) | same |
//! | VectorStore | `InMemoryVectorStore` (brute cosine, ≤10K vectors) | LanceDB (S17) |
//! | EmbeddingService | `StubEmbeddingService` (deterministic hash) | `FastEmbedService` (S4 swap-in) |
//! | SchemaFetcher | `StubSchemaFetcher` (hand-curated test schemas) | `TerraformCliSchemaFetcher` (S4) |
//! | KnowledgeService | wires all the above | unchanged |
//!
//! Pattern: terrashift_plan.md §7 (Knowledge Service multi-tier architecture).
//! Constitution: Article VI (version pinning), XII rule 2 (cache-first).

pub mod embedding;
pub mod errors;
pub mod knowledge_service;
pub mod local_schema_store;
pub mod registry_client;
pub mod schema_fetcher_cli;
pub mod schema_store;
pub mod types;
pub mod vector_store;

pub use embedding::{
    EmbeddingError, EmbeddingService, StubEmbeddingService, DEFAULT_EMBEDDING_DIM,
};
pub use errors::SchemaError;
pub use knowledge_service::{
    FetchOutcome, FirstLaunchReport, KnowledgeError, KnowledgeService, ResourceMatch,
};
pub use local_schema_store::LocalSchemaStore;
pub use registry_client::{
    ProviderMetadata, ProviderVersionEntry, ProviderVersions, RegistryError, SchemaFetcher,
    StubSchemaFetcher, TerraformRegistryClient,
};
pub use schema_fetcher_cli::TerraformCliSchemaFetcher;
pub use schema_store::SchemaStore;
pub use types::{AttributeSchema, MappingExample, ProviderSchema, ResourceSchema};
pub use vector_store::{InMemoryVectorStore, VectorHit, VectorRow, VectorStore, VectorStoreError};
