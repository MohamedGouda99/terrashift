// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Terraform Registry API client + provider schema fetcher.
//!
//! Pattern: terrashift_plan.md §7.4 (sync worker), §6.10 (provider schemas).
//! Constitution: Article VI (version pinning — `terraform providers schema`
//! is run against a frozen `required_providers` block, never `latest`).
//!
//! ## Two layers
//!
//! 1. **`TerraformRegistryClient`** — HTTPS to `registry.terraform.io` for
//!    provider metadata (versions, latest, namespaces). Pure HTTP, no
//!    side effects.
//! 2. **`SchemaFetcher`** — wraps `terraform providers schema -json`
//!    subprocess to extract the actual resource/attribute schemas. The
//!    Registry API itself doesn't return schemas — they live inside the
//!    provider binary, accessible only via `terraform init` + the schema
//!    subcommand. Subprocess wrapper is the canonical approach (also used
//!    by Pulumi tf2pulumi, Terraformer, etc.).
//!
//! ## Status
//!
//! - `TerraformRegistryClient` is HTTP-only; pure Rust via reqwest. **Used for
//!   metadata only** — listing available versions so `terrashift schema update`
//!   can expand a constraint like `~> 5.30` against the registry's published
//!   version list. It does NOT — and never has — fetched schemas; the registry
//!   does not expose them as a REST endpoint (see paragraph above).
//! - `SchemaFetcher` is a trait with two impls: `StubSchemaFetcher` (tests)
//!   and `TerraformCliSchemaFetcher` (the real impl, in `schema_fetcher_cli.rs`).
//!   The CLI fetcher runs the canonical `tempdir → versions.tf → terraform init →
//!   terraform providers schema -json → ProviderSchema` flow.
//! - The two layers are wired by `KnowledgeService::sync_provider`: the registry
//!   client expands the version constraint; the CLI fetcher extracts the schema
//!   for each resolved version.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, instrument};

use crate::types::ProviderSchema;

const REGISTRY_BASE: &str = "https://registry.terraform.io/v1/providers";

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("provider not found: {namespace}/{name}")]
    NotFound { namespace: String, name: String },

    #[error("schema fetch failed: {0}")]
    SchemaFetch(String),

    #[error("terraform CLI unavailable: {0}")]
    TerraformUnavailable(String),

    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Subset of the Registry's provider metadata response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub id: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub description: Option<String>,
}

/// Subset of the Registry's versions response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderVersions {
    pub id: String,
    pub versions: Vec<ProviderVersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderVersionEntry {
    pub version: String,
    pub protocols: Vec<String>,
    pub platforms: Vec<ProviderPlatform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPlatform {
    pub os: String,
    pub arch: String,
}

/// Pure HTTP client for the Terraform Registry.
pub struct TerraformRegistryClient {
    http: reqwest::Client,
    base_url: String,
}

impl TerraformRegistryClient {
    pub fn new() -> Result<Self, RegistryError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("terrashift/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base_url: REGISTRY_BASE.to_string(),
        })
    }

    /// Override base URL (used in tests against a mock).
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    /// `GET /v1/providers/<namespace>/<name>` — latest version metadata.
    #[instrument(skip(self), fields(namespace, name))]
    pub async fn get_provider(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<ProviderMetadata, RegistryError> {
        let url = format!("{}/{}/{}", self.base_url, namespace, name);
        debug!("GET {url}");
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }
        let resp = resp.error_for_status()?;
        Ok(resp.json::<ProviderMetadata>().await?)
    }

    /// `GET /v1/providers/<namespace>/<name>/versions` — all versions.
    #[instrument(skip(self), fields(namespace, name))]
    pub async fn list_versions(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<ProviderVersions, RegistryError> {
        let url = format!("{}/{}/{}/versions", self.base_url, namespace, name);
        debug!("GET {url}");
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }
        let resp = resp.error_for_status()?;
        Ok(resp.json::<ProviderVersions>().await?)
    }
}

/// Per-provider schema extraction. The Registry API doesn't return schemas
/// directly — they live inside the provider binary. The canonical approach
/// is to (1) write a tiny `versions.tf` declaring the provider, (2) `terraform
/// init`, (3) `terraform providers schema -json`, (4) parse the result.
///
/// This trait abstracts that flow so tests can use `StubSchemaFetcher` and
/// production uses `TerraformCliSchemaFetcher` (S4).
#[async_trait]
pub trait SchemaFetcher: Send + Sync {
    async fn fetch(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<ProviderSchema, RegistryError>;
}

/// Test/dev fetcher — returns a hand-curated schema for known (provider,
/// version) pairs. Useful for unit tests that exercise the RAG flow without
/// depending on terraform CLI being installed.
pub struct StubSchemaFetcher {
    schemas: std::collections::HashMap<(String, String), ProviderSchema>,
}

impl StubSchemaFetcher {
    pub fn new() -> Self {
        Self {
            schemas: std::collections::HashMap::new(),
        }
    }

    pub fn with_schema(mut self, name: &str, version: &str, schema: ProviderSchema) -> Self {
        self.schemas
            .insert((name.to_string(), version.to_string()), schema);
        self
    }
}

impl Default for StubSchemaFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaFetcher for StubSchemaFetcher {
    async fn fetch(
        &self,
        _namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<ProviderSchema, RegistryError> {
        self.schemas
            .get(&(name.to_string(), version.to_string()))
            .cloned()
            .ok_or_else(|| {
                RegistryError::SchemaFetch(format!(
                    "stub fetcher has no schema for {name}@{version}"
                ))
            })
    }
}
