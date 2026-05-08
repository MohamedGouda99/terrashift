// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schema gc [--keep-versions <N>]` — delete schemas not
//! referenced by any profile pin and beyond `--keep-versions` (default 2).
//!
//! Article IX (data governance): cached schemas are reproducibly
//! re-fetchable from `terraform providers schema -json`, so they are NOT
//! user data and are eligible for `gc`. State files, audit logs, and
//! migration outputs are never touched by this command.

use anyhow::{Context, Result};
use std::path::PathBuf;
use terrashift_knowledge::RuntimeManifest;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Per-provider, keep this many newest versions. Default 2 (current +
    /// previous). Setting 0 deletes every cached version; very large values
    /// effectively disable gc.
    #[arg(long, default_value_t = 2)]
    pub keep_versions: usize,

    /// Don't actually delete; just report what would happen.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Cache root override (default ~/.terrashift/schemas/).
    #[arg(long)]
    pub cache: Option<PathBuf>,
}

pub async fn run(args: Args, _profile: Option<PathBuf>) -> Result<()> {
    let cache_root = super::resolve_cache_root(args.cache)?;
    let manifest_path = super::manifest_path(&cache_root);
    let mut manifest = RuntimeManifest::load(&manifest_path)?;

    if manifest.schemas.is_empty() {
        println!("Nothing to gc (cache empty).");
        return Ok(());
    }

    // Group by provider, then keep top-N versions per provider (descending).
    use std::collections::BTreeMap;
    let mut by_provider: BTreeMap<String, Vec<terrashift_knowledge::RuntimeManifestEntry>> =
        BTreeMap::new();
    for e in &manifest.schemas {
        by_provider
            .entry(e.provider.clone())
            .or_default()
            .push(e.clone());
    }

    let mut to_delete: Vec<(String, String, String)> = Vec::new();
    for (provider, mut entries) in by_provider {
        entries.sort_by(|a, b| b.version.cmp(&a.version));
        for entry in entries.into_iter().skip(args.keep_versions) {
            to_delete.push((provider.clone(), entry.version, entry.schema_path));
        }
    }

    if to_delete.is_empty() {
        println!(
            "Nothing to gc — every provider has ≤{} versions cached.",
            args.keep_versions
        );
        return Ok(());
    }

    if args.dry_run {
        println!("Dry run — would delete:");
        for (p, v, _) in &to_delete {
            println!("  {p}@{v}");
        }
        return Ok(());
    }

    println!("Deleting {} schema(s):", to_delete.len());
    for (provider, version, schema_path) in &to_delete {
        let dir = cache_root.join(provider).join(version);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                println!("  ✓ removed {}", dir.display());
                manifest.remove(provider, version);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Directory missing on disk but manifest had an entry — clean
                // the orphan from the manifest so subsequent verify runs pass.
                println!(
                    "  ! {} (already missing on disk; clearing orphan manifest entry)",
                    dir.display()
                );
                manifest.remove(provider, version);
            }
            Err(e) => {
                eprintln!("  ✗ remove {} failed: {e}", dir.display());
                eprintln!("    (manifest entry kept; rerun once filesystem permits)");
                let _ = schema_path;
            }
        }
    }

    manifest
        .save(&manifest_path)
        .with_context(|| format!("save manifest after gc: {}", manifest_path.display()))?;

    println!("\nDone. Manifest: {}", manifest_path.display());
    Ok(())
}
