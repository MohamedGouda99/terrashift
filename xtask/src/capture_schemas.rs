// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `cargo xtask capture-schemas` — populate `libs/knowledge/seed/`.
//!
//! Reads `libs/knowledge/seed/manifest.toml` for pinned `(provider, version)`
//! tuples. Runs `TerraformCliSchemaFetcher::fetch` for each. Writes the
//! result to `libs/knowledge/seed/<provider>/<version>/schema.json`.
//!
//! This is the build-time prerequisite for `cargo build --release` to
//! produce a binary that ships with bundled schemas.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use terrashift_knowledge::registry_client::SchemaFetcher;
use terrashift_knowledge::schema_fetcher_cli::TerraformCliSchemaFetcher;
use tracing::{info, warn};

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Override the seed manifest path. Default `libs/knowledge/seed/manifest.toml`.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Override the seed output root. Default `libs/knowledge/seed/`.
    #[arg(long)]
    pub seed_root: Option<PathBuf>,

    /// Set TF_PLUGIN_CACHE_DIR for terraform init. Default
    /// `target/build-schemas/plugin-cache/` so re-runs reuse provider binaries.
    #[arg(long)]
    pub plugin_cache: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct SeedPinManifest {
    pin: Vec<SeedPin>,
}

#[derive(Debug, Deserialize)]
struct SeedPin {
    provider: String,
    version: String,
    source: String,
}

pub async fn run(args: Args) -> Result<()> {
    let manifest_path = args
        .manifest
        .unwrap_or_else(|| PathBuf::from("libs/knowledge/seed/manifest.toml"));
    let seed_root = args
        .seed_root
        .unwrap_or_else(|| PathBuf::from("libs/knowledge/seed"));
    let plugin_cache = args
        .plugin_cache
        .unwrap_or_else(|| PathBuf::from("target/build-schemas/plugin-cache"));

    if !manifest_path.is_file() {
        return Err(anyhow!(
            "seed manifest not found at {} — create one with [[pin]] entries first",
            manifest_path.display()
        ));
    }

    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: SeedPinManifest =
        toml::from_str(&text).with_context(|| format!("parse {}", manifest_path.display()))?;

    if manifest.pin.is_empty() {
        warn!("seed manifest has no pins — nothing to capture");
        return Ok(());
    }

    std::fs::create_dir_all(&plugin_cache)
        .with_context(|| format!("mkdir {}", plugin_cache.display()))?;

    let fetcher = TerraformCliSchemaFetcher::new().with_plugin_cache_dir(plugin_cache.clone());
    fetcher
        .check_available()
        .await
        .context("terraform unavailable — see `terrashift schema update` install hint")?;

    info!(
        manifest = %manifest_path.display(),
        seed_root = %seed_root.display(),
        plugin_cache = %plugin_cache.display(),
        pins = manifest.pin.len(),
        "capturing bundled schemas"
    );

    let mut succeeded = 0usize;
    let mut failed = 0usize;
    for pin in &manifest.pin {
        let (namespace, name) = parse_source(&pin.source)?;
        match fetcher.fetch(namespace, name, &pin.version).await {
            Ok(schema) => {
                let dir = seed_root.join(&pin.provider).join(&pin.version);
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("mkdir {}", dir.display()))?;
                let schema_path = dir.join("schema.json");
                let json =
                    serde_json::to_vec_pretty(&schema).context("serialise ProviderSchema")?;
                std::fs::write(&schema_path, &json)
                    .with_context(|| format!("write {}", schema_path.display()))?;
                println!(
                    "  ✓ {}/{}@{}  ({} resources)",
                    namespace,
                    name,
                    pin.version,
                    schema.resources.len()
                );
                succeeded += 1;
            }
            Err(e) => {
                eprintln!("  ✗ {}/{}@{}  {e}", namespace, name, pin.version);
                failed += 1;
            }
        }
    }

    println!();
    println!(
        "Done: {succeeded} captured, {failed} failed. Seed: {}",
        seed_root.display()
    );
    if failed > 0 {
        Err(anyhow!("{failed} pin(s) failed to capture"))
    } else {
        Ok(())
    }
}

fn parse_source(source: &str) -> Result<(&str, &str)> {
    source.split_once('/').ok_or_else(|| {
        anyhow!("invalid source '{source}' — expected '<namespace>/<name>' (e.g. 'hashicorp/aws')")
    })
}

// Forward declaration — the helper lives in the public `terrashift-knowledge`
// crate but isn't re-exported. Keeping it here as a local trait import for
// the `with_plugin_cache_dir` call above.
#[allow(dead_code)]
fn _ensure_path_used(_p: &Path) {}
