// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schemas {sync,list,seed}` — schema cache operations.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use terrashift_knowledge::{
    schema_fetcher_cli::TerraformCliSchemaFetcher, FetchOutcome, InMemoryVectorStore,
    KnowledgeService, LocalSchemaStore, SchemaStore, StubEmbeddingService,
};

use super::util::{locate_seed_dir, VECTOR_DIM};

#[derive(clap::Subcommand, Debug)]
pub enum Cmd {
    /// Pull provider schemas from the Terraform Registry. Requires
    /// `terraform >= 1.0` on PATH.
    Sync {
        /// Provider namespace (default "hashicorp").
        #[arg(long, default_value = "hashicorp")]
        namespace: String,
        /// Provider name (e.g. "aws", "azurerm", "google").
        #[arg(long)]
        provider: String,
        /// Provider version pin (e.g. "5.30.0"). No default — explicit
        /// versioning is required (Article VI; no "latest" in production).
        #[arg(long)]
        version: String,
        /// Cache file location (default ~/.terrashift/cache/schemas.sqlite).
        #[arg(long)]
        cache: Option<PathBuf>,
    },

    /// List cached (provider, version) tuples.
    List {
        #[arg(long)]
        cache: Option<PathBuf>,
    },

    /// Load only the bundled seed (offline; no registry calls).
    /// Useful as a smoke test that the seed bundle is reachable.
    Seed,
}

pub async fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Sync {
            namespace,
            provider,
            version,
            cache,
        } => sync_provider(namespace, provider, version, cache).await,
        Cmd::List { cache } => list_cached(cache).await,
        Cmd::Seed => seed_only().await,
    }
}

async fn sync_provider(
    namespace: String,
    provider: String,
    version: String,
    cache: Option<PathBuf>,
) -> Result<()> {
    let cache_path = resolve_cache_path(cache)?;
    println!("📂 Cache: {}", cache_path.display());
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create cache dir {}", parent.display()))?;
    }

    let store = Arc::new(LocalSchemaStore::open(&cache_path).await?);
    let vector = Arc::new(InMemoryVectorStore::new(VECTOR_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(VECTOR_DIM));
    let fetcher = Arc::new(TerraformCliSchemaFetcher::new());
    let knowledge = KnowledgeService::new(store, vector, embedder, fetcher);

    let seed_dir = locate_seed_dir().unwrap_or_else(|| PathBuf::from("./libs/knowledge/seed"));

    println!(
        "📚 Loading seed from {} (always-on baseline)",
        seed_dir.display()
    );
    println!(
        "🌐 Fetching {namespace}/{provider}@{version} from Terraform Registry \
         (this runs `terraform init` — first call ~30s while terraform downloads \
         the provider binary; subsequent calls are cache-warm)..."
    );

    let report = knowledge
        .first_launch_sync(
            &seed_dir,
            &[(namespace.clone(), provider.clone(), version.clone())],
        )
        .await?;

    println!("\n✓ Seed: {} resources loaded", report.seed_resource_count);
    for (name, ver, count, outcome) in &report.per_provider {
        match outcome {
            FetchOutcome::Fetched => {
                println!("✓ {name}@{ver}: fetched {count} resources from registry");
            }
            FetchOutcome::AlreadyCached => {
                println!("✓ {name}@{ver}: already cached (no network call)");
            }
            FetchOutcome::FetchFailed(e) => {
                println!("⚠ {name}@{ver}: fetch failed — {e}");
                println!("  (seed coverage still active; rerun when network/CLI is available)");
            }
        }
    }
    Ok(())
}

async fn list_cached(cache: Option<PathBuf>) -> Result<()> {
    let cache_path = resolve_cache_path(cache)?;
    if !cache_path.exists() {
        println!("Cache empty: {} does not exist yet.", cache_path.display());
        println!("Run `terrashift schemas sync --provider aws --version 5.30.0` to populate.");
        return Ok(());
    }
    let store = LocalSchemaStore::open(&cache_path).await?;

    println!("📂 Cache: {}\n", cache_path.display());
    let mut found_any = false;
    for provider in ["aws", "azurerm", "google", "hashicorp"] {
        let versions = store.list_versions(provider).await?;
        if !versions.is_empty() {
            found_any = true;
            println!("  {provider}: {versions:?}");
        }
    }
    if !found_any {
        println!("(empty — run `terrashift schemas sync` to populate)");
    }
    Ok(())
}

async fn seed_only() -> Result<()> {
    let knowledge = super::util::build_knowledge_service().await?;
    // build_knowledge_service already loaded + logged the seed.
    drop(knowledge);
    println!("✓ Seed loaded successfully.");
    Ok(())
}

fn resolve_cache_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("no $USERPROFILE / $HOME — pass --cache <path>"))?;
    Ok(home
        .join(".terrashift")
        .join("cache")
        .join("schemas.sqlite"))
}
