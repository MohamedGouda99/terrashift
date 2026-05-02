//! `LocalSchemaStore` — sqlx + SQLite implementation of `SchemaStore`.
//!
//! Pattern: stakpak_arch.md §9 (analogous to Stakpak's local libsql backend).
//! Source: refs/stakpak/libs/api/src/local/storage.rs (sqlx pattern reference).
//! Constitution: Article VI (PK on (provider, version) enforces version pinning;
//! INSERT OR IGNORE makes pinned-rewrite a silent no-op signaled via Ok(false)).

use crate::errors::SchemaError;
use crate::schema_store::SchemaStore;
use crate::types::{MappingExample, ProviderSchema};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, instrument};

const SCHEMA_DDL: &str = "
CREATE TABLE IF NOT EXISTS provider_schemas (
    provider TEXT NOT NULL,
    version TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (provider, version)
);
CREATE INDEX IF NOT EXISTS idx_provider_schemas_provider ON provider_schemas(provider);
";

/// SQLite-backed schema store.
pub struct LocalSchemaStore {
    pool: SqlitePool,
}

impl LocalSchemaStore {
    /// Open (or create) a file-backed store at the given path.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, SchemaError> {
        let opts = SqliteConnectOptions::new()
            .filename(path.as_ref())
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(opts).await?;
        Self::init_schema(&pool).await?;
        Ok(Self { pool })
    }

    /// Open an in-memory store (tests, ephemeral CLI runs).
    pub async fn in_memory() -> Result<Self, SchemaError> {
        let opts =
            SqliteConnectOptions::from_str("sqlite::memory:").map_err(SchemaError::Storage)?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        Self::init_schema(&pool).await?;
        Ok(Self { pool })
    }

    async fn init_schema(pool: &SqlitePool) -> Result<(), SchemaError> {
        sqlx::query(SCHEMA_DDL).execute(pool).await?;
        Ok(())
    }
}

#[async_trait]
impl SchemaStore for LocalSchemaStore {
    #[instrument(skip(self), fields(provider, version))]
    async fn fetch_provider_schema(
        &self,
        provider: &str,
        version: &str,
    ) -> Result<ProviderSchema, SchemaError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT schema_json FROM provider_schemas WHERE provider = ?1 AND version = ?2",
        )
        .bind(provider)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((json,)) => {
                debug!("schema cache hit");
                let schema: ProviderSchema = serde_json::from_str(&json)?;
                Ok(schema)
            }
            None => Err(SchemaError::NotFound {
                provider: provider.to_string(),
                version: version.to_string(),
            }),
        }
    }

    #[instrument(skip(self, schema), fields(provider = %schema.provider, version = %schema.version))]
    async fn cache_schema(&self, schema: &ProviderSchema) -> Result<bool, SchemaError> {
        let json = serde_json::to_string(schema)?;
        let result = sqlx::query(
            "INSERT OR IGNORE INTO provider_schemas (provider, version, schema_json, fetched_at) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&schema.provider)
        .bind(&schema.version)
        .bind(&json)
        .bind(schema.fetched_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self), fields(provider))]
    async fn list_versions(&self, provider: &str) -> Result<Vec<String>, SchemaError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT version FROM provider_schemas WHERE provider = ?1 ORDER BY version DESC",
        )
        .bind(provider)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    #[instrument(skip(self))]
    async fn search_mappings(
        &self,
        _query: &str,
        _top_k: usize,
    ) -> Result<Vec<MappingExample>, SchemaError> {
        // SchemaStore-level stub. The real RAG retrieval lives in
        // KnowledgeService (composes this store with VectorStore + EmbeddingService).
        // Direct callers of LocalSchemaStore.search_mappings() get an empty Vec —
        // they should be calling KnowledgeService instead for actual retrieval.
        Ok(Vec::new())
    }
}
