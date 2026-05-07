// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `cargo xtask refresh-eval-schemas [--check]` — registry drift report.
//!
//! Reads `terrashift-evals/.schema-cache/manifest.json`, queries
//! `https://registry.terraform.io/v1/providers/<source>/versions` per
//! cached entry, and reports any pinned provider with a newer published
//! version. With `--check` (default) only reports; without `--check` the
//! command additionally re-runs the full `capture-eval-schemas` non-dry
//! flow against the discovered newer versions.
//!
//! Pattern: terrashift_plan.md §6.X (registry as metadata source).
//! Constitution: Article VI (every refresh writes back a NEW pin in the
//! manifest — never silently swaps versions).

use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;
use std::path::PathBuf;
use terrashift_knowledge::registry_client::TerraformRegistryClient;
use tracing::info;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Override the suite root. Default `terrashift-evals`.
    #[arg(long)]
    pub suite_root: Option<PathBuf>,

    /// Provider namespace. Defaults to `hashicorp` (every official Terraform
    /// provider lives there).
    #[arg(long, default_value = "hashicorp")]
    pub namespace: String,

    /// Only report drift; do not write. Default true — non-check writes
    /// land alongside the capture-eval-schemas refactor when CI Job C
    /// (RFC §5.4) gets full PR-opening authority.
    #[arg(long, default_value_t = true)]
    pub check: bool,
}

#[derive(Debug, Deserialize)]
struct CacheManifestEntry {
    provider: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct CacheManifest {
    entries: Vec<CacheManifestEntry>,
}

pub async fn run(args: Args) -> Result<()> {
    let suite_root = args
        .suite_root
        .unwrap_or_else(|| PathBuf::from("terrashift-evals"));
    let manifest_path = suite_root.join(".schema-cache").join("manifest.json");

    if !manifest_path.is_file() {
        println!(
            "No manifest.json at {} — nothing to refresh.\n\
             Run `cargo xtask capture-eval-schemas` first.",
            manifest_path.display()
        );
        return Ok(());
    }

    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: CacheManifest = serde_json::from_str(&text)
        .with_context(|| format!("parse {}", manifest_path.display()))?;

    info!(
        entries = manifest.entries.len(),
        "checking for newer versions"
    );

    let client = TerraformRegistryClient::new().context("build registry HTTP client")?;
    let mut newer_found = 0usize;

    for entry in &manifest.entries {
        let versions = match client.list_versions(&args.namespace, &entry.provider).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "  ! {}/{} (v{}): registry query failed — {e}",
                    args.namespace, entry.provider, entry.version
                );
                continue;
            }
        };

        // Semver comparison — lexicographic max returns 5.9.0 > 5.10.0,
        // which is wrong. Filter to stable (no prerelease) and use the
        // typed semver max.
        let cached_semver = Version::parse(&entry.version).ok();
        let highest_stable = versions
            .versions
            .iter()
            .filter_map(|v| Version::parse(&v.version).ok())
            .filter(|v| v.pre.is_empty())
            .max();

        match (cached_semver, highest_stable) {
            (Some(cached), Some(latest)) if latest > cached => {
                println!(
                    "  ⬆ {provider}: {cached_v} cached, {latest_v} available",
                    provider = entry.provider,
                    cached_v = entry.version,
                    latest_v = latest
                );
                newer_found += 1;
            }
            _ => {
                println!("  ✓ {}: {} (current)", entry.provider, entry.version);
            }
        }
    }

    println!();
    if newer_found == 0 {
        println!("All cached schemas are up-to-date with the registry.");
    } else {
        println!("{newer_found} provider(s) have newer versions available.");
        if !args.check {
            println!(
                "Non-check refresh: re-running capture-eval-schemas to capture newer pins.\n\
                 (Stage 1: this writes new manifest entries; the fixture manifest.toml \
                 files still need a hand edit to reference the new versions before \
                 the eval suite consumes them — Article VI: every pin is a deliberate human action.)"
            );
            // Defer to capture-eval-schemas with the current required_schemas
            // set; the fixture authors decide when to bump pins in their own files.
            crate::capture_eval_schemas::run(crate::capture_eval_schemas::Args {
                suite_root: Some(suite_root),
                namespace: args.namespace,
                dry_run: false,
            })
            .await?;
        } else {
            println!("(report-only — pass without --check to re-capture against new versions)");
        }
    }

    Ok(())
}
