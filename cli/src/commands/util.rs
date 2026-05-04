// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Shared CLI helpers: profile location, seed directory discovery,
//! KnowledgeService construction.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use terrashift_ai::{Profile, RealClient, Resolver, Tier};
use terrashift_knowledge::{
    InMemoryVectorStore, KnowledgeService, LocalSchemaStore, StubEmbeddingService,
    StubSchemaFetcher,
};

pub const VECTOR_DIM: usize = 384;

/// Resolve the operator profile path. Priority:
///   1. explicit --profile <path>
///   2. $TERRASHIFT_PROFILE env var
///   3. ~/.terrashift/profile.toml (or %USERPROFILE%\.terrashift\profile.toml on Windows)
pub fn resolve_profile_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Ok(env) = std::env::var("TERRASHIFT_PROFILE") {
        return Ok(PathBuf::from(env));
    }

    let home = home_dir().context(
        "could not locate user home dir; set $TERRASHIFT_PROFILE or pass --profile <path>",
    )?;
    Ok(home.join(".terrashift").join("profile.toml"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Load + parse the operator profile.
pub fn load_profile(path: &Path) -> Result<Profile> {
    let toml = std::fs::read_to_string(path).with_context(|| {
        format!(
            "could not read profile at {} — see assets/profile.example.toml",
            path.display()
        )
    })?;
    Profile::from_toml(&toml).map_err(|e| anyhow!("parse profile {}: {e}", path.display()))
}

/// Locate the bundled seed dir. Tries, in order:
///   1. <exe-dir>/../share/terrashift/seed   (installed layout)
///   2. <exe-dir>/../libs/knowledge/seed      (running from target/<profile>/)
///   3. <CWD>/libs/knowledge/seed             (running via cargo from workspace)
///   4. <CWD>/seed                            (custom layout)
///
/// Returns `None` if nothing found — the caller should warn but continue.
pub fn locate_seed_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(
                parent
                    .join("..")
                    .join("share")
                    .join("terrashift")
                    .join("seed"),
            );
            candidates.push(
                parent
                    .join("..")
                    .join("..")
                    .join("libs")
                    .join("knowledge")
                    .join("seed"),
            );
            candidates.push(
                parent
                    .join("..")
                    .join("..")
                    .join("..")
                    .join("libs")
                    .join("knowledge")
                    .join("seed"),
            );
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("libs").join("knowledge").join("seed"));
        candidates.push(cwd.join("seed"));
    }

    candidates.into_iter().find(|p| p.exists() && p.is_dir())
}

/// Build a `KnowledgeService` with the bundled seed loaded. Uses
/// stub embedding (deterministic) until FastEmbed swap lands.
pub async fn build_knowledge_service() -> Result<KnowledgeService> {
    let store = Arc::new(
        LocalSchemaStore::in_memory()
            .await
            .context("init in-memory schema store")?,
    );
    let vector = Arc::new(InMemoryVectorStore::new(VECTOR_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(VECTOR_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    let knowledge = KnowledgeService::new(store, vector, embedder, fetcher);

    if let Some(seed_dir) = locate_seed_dir() {
        let n = knowledge
            .seed_from_bundle(&seed_dir)
            .await
            .with_context(|| format!("seed from bundle at {}", seed_dir.display()))?;
        tracing::info!("📚 Seeded {} resources from {}", n, seed_dir.display());
    } else {
        tracing::warn!(
            "no seed bundle found — Mapper RAG context will be empty. \
            Run from workspace root or install with seed at <prefix>/share/terrashift/seed/"
        );
    }

    Ok(knowledge)
}

/// Build the LLM client for a given tier from the resolved profile.
pub fn build_llm_client(profile: &Profile, tier: Tier) -> Result<RealClient> {
    let resolved = Resolver::resolve_for_tier(profile, None, None, tier)
        .with_context(|| format!("resolve {tier:?} model from profile"))?;
    RealClient::new(resolved).context("construct LlmClient")
}
