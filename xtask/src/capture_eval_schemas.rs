// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `cargo xtask capture-eval-schemas` — populate
//! `terrashift-evals/.schema-cache/`.
//!
//! Walks `terrashift-evals/` for golden fixtures, computes the union of
//! every `required_schemas` declaration, captures each unique
//! `(provider, version)` pair via `terraform providers schema -json`, and
//! writes the result as `<provider>@<version>.json.zst` (zstd level 19,
//! per RFC §5.2). Updates `<.schema-cache>/manifest.json` with sha256 +
//! terraform_version + captured_at metadata so `cargo xtask verify-eval-schemas`
//! has something to audit against.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;
use terrashift_eval::discover_suite;
use terrashift_knowledge::registry_client::SchemaFetcher;
use terrashift_knowledge::schema_fetcher_cli::TerraformCliSchemaFetcher;
use tracing::info;

/// zstd compression level used for cached schemas. 19 is the documented
/// MVP target per RFC §5.2 — empirically ~10× ratio on typed JSON, with
/// compression cost amortised by the once-per-PR capture cadence.
const ZSTD_LEVEL: i32 = 19;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Override the suite root. Default `terrashift-evals`.
    #[arg(long)]
    pub suite_root: Option<PathBuf>,

    /// Default `hashicorp` (every official Terraform provider lives there).
    #[arg(long, default_value = "hashicorp")]
    pub namespace: String,

    /// Print the would-be captures without invoking terraform.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheManifest {
    schema_version: u32,
    captured_at: String,
    terraform_version: String,
    entries: Vec<CacheEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    schema_id: String,
    provider: String,
    version: String,
    sha256: String,
    size_bytes: u64,
    /// Bytes BEFORE zstd compression — useful for sanity-checking ratio.
    uncompressed_size_bytes: u64,
}

pub async fn run(args: Args) -> Result<()> {
    let suite_root = args
        .suite_root
        .unwrap_or_else(|| PathBuf::from("terrashift-evals"));

    if !suite_root.is_dir() {
        return Err(anyhow!(
            "suite root '{}' is not a directory",
            suite_root.display()
        ));
    }

    let goldens = discover_suite(&suite_root)
        .with_context(|| format!("discover golden fixtures in {}", suite_root.display()))?;

    info!(
        suite = %suite_root.display(),
        fixtures = goldens.len(),
        "scanning for required_schemas"
    );

    // Union the declarations across every fixture. BTreeSet keys deterministic
    // (matters for repro builds — same input produces same `.schema-cache/`).
    let mut required: BTreeSet<(String, String)> = BTreeSet::new();
    for g in &goldens {
        for req in &g.manifest.required_schemas {
            required.insert((req.provider.clone(), req.version.clone()));
        }
    }

    if required.is_empty() {
        println!(
            "No required_schemas declared by any fixture under {}.\n\
             Add `required_schemas = [...]` entries to fixture manifest.toml files \
             to populate the eval schema cache.",
            suite_root.display()
        );
        return Ok(());
    }

    let cache_dir = suite_root.join(".schema-cache");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("mkdir {}", cache_dir.display()))?;

    println!(
        "Eval schema cache: {} ({} unique schemas required)\n",
        cache_dir.display(),
        required.len()
    );

    if args.dry_run {
        println!("Dry run — would capture:");
        for (provider, version) in &required {
            println!("  {provider}@{version}");
        }
        return Ok(());
    }

    let fetcher = TerraformCliSchemaFetcher::new();
    fetcher
        .check_available()
        .await
        .context("terraform CLI unavailable")?;
    let terraform_version = detect_terraform_version().unwrap_or_else(|| "unknown".to_string());

    let mut entries: Vec<CacheEntry> = Vec::with_capacity(required.len());
    for (provider, version) in &required {
        print!("  • capturing {provider}@{version} ... ");
        let schema = fetcher
            .fetch(&args.namespace, provider, version)
            .await
            .with_context(|| format!("fetch {provider}@{version}"))?;

        let json = serde_json::to_vec(&schema).context("serialise ProviderSchema")?;
        let uncompressed_size = json.len() as u64;
        let compressed = zstd::encode_all(&json[..], ZSTD_LEVEL).context("zstd encode")?;
        let size_bytes = compressed.len() as u64;
        let sha256 = sha256_hex(&compressed);

        let schema_id = format!("{provider}@{version}");
        let path = cache_dir.join(format!("{schema_id}.json.zst"));
        std::fs::write(&path, &compressed).with_context(|| format!("write {}", path.display()))?;

        println!(
            "{} bytes raw, {} bytes compressed (ratio {:.1}×)",
            uncompressed_size,
            size_bytes,
            uncompressed_size as f64 / size_bytes.max(1) as f64
        );

        entries.push(CacheEntry {
            schema_id,
            provider: provider.clone(),
            version: version.clone(),
            sha256,
            size_bytes,
            uncompressed_size_bytes: uncompressed_size,
        });
    }

    let manifest = CacheManifest {
        schema_version: 1,
        captured_at: Utc::now().to_rfc3339(),
        terraform_version,
        entries,
    };
    let manifest_path = cache_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest).context("serialise manifest")?;
    std::fs::write(&manifest_path, manifest_json)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    println!(
        "\nDone. Manifest: {}\nCommit alongside the fixture changes that introduced these pins.",
        manifest_path.display()
    );
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(result.len() * 2);
    for b in result {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

fn detect_terraform_version() -> Option<String> {
    use std::process::Command;
    let out = Command::new("terraform").arg("version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .next()
        .and_then(|l| l.strip_prefix("Terraform v"))
        .map(|v| v.trim().to_string())
}
