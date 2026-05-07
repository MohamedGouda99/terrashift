// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schema update` — capture or refresh schemas via
//! `terraform providers schema -json`.
//!
//! Per RFC §4.7, every successful capture emits one
//! `AuditPayload::SchemaCapture` audit entry against `~/.terrashift/audit.db`.
//! `terraform`-on-PATH failures surface a platform-aware install hint via
//! the `install_hint` module instead of a Rust backtrace.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Instant;
use terrashift_audit::{
    Actor, AuditEntry, AuditPayload, AuditStore, CaptureVia, LocalAuditStore, Outcome,
    SessionSigner,
};
use terrashift_knowledge::{
    registry_client::SchemaFetcher, schema_fetcher_cli::TerraformCliSchemaFetcher, RuntimeManifest,
    RuntimeManifestEntry,
};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Provider name (e.g. "aws", "azurerm", "google"). Required unless --all.
    #[arg(long)]
    pub provider: Option<String>,

    /// Concrete version to capture (e.g. "5.30.0"). Required when --provider
    /// is set. Constraint syntax like "~> 5.30" is not yet supported —
    /// pass an exact version for now.
    #[arg(long)]
    pub version: Option<String>,

    /// Provider namespace. Defaults to "hashicorp" — the registry namespace
    /// every official Terraform provider lives under.
    #[arg(long, default_value = "hashicorp")]
    pub namespace: String,

    /// Refresh every cached `(provider, version)` pair. Mutually exclusive
    /// with --provider/--version.
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// Take all defaults, no prompts. For CI use.
    #[arg(long, default_value_t = false)]
    pub non_interactive: bool,

    /// Cache root override (default ~/.terrashift/schemas/).
    #[arg(long)]
    pub cache: Option<PathBuf>,
}

pub async fn run(args: Args, _profile: Option<PathBuf>) -> Result<()> {
    let cache_root = super::resolve_cache_root(args.cache)?;
    std::fs::create_dir_all(&cache_root)
        .with_context(|| format!("create cache dir {}", cache_root.display()))?;

    let manifest_path = super::manifest_path(&cache_root);
    let mut manifest = RuntimeManifest::load(&manifest_path)?;

    let fetcher = TerraformCliSchemaFetcher::new();

    // Loud failure if terraform is missing — install hint, not a panic.
    if let Err(e) = fetcher.check_available().await {
        let hint = super::install_hint::for_current_platform();
        eprintln!("error: terraform CLI unavailable: {e}");
        eprintln!();
        eprintln!("{hint}");
        return Err(anyhow!("terraform not on PATH"));
    }

    let targets: Vec<(String, String, String)> = if args.all {
        manifest
            .cached_pairs()
            .into_iter()
            .map(|(p, v)| (args.namespace.clone(), p, v))
            .collect()
    } else {
        let provider = args
            .provider
            .clone()
            .ok_or_else(|| anyhow!("--provider is required (or use --all)"))?;
        let version = args
            .version
            .clone()
            .ok_or_else(|| anyhow!("--version is required (or use --all)"))?;
        vec![(args.namespace.clone(), provider, version)]
    };

    if targets.is_empty() {
        eprintln!("no targets — pass --provider/--version, or run --all against a non-empty cache");
        return Ok(());
    }

    // Open the audit store and register a fresh run id for this command.
    let audit_path = cache_root
        .parent()
        .map(|p| p.join("audit.db"))
        .unwrap_or_else(|| cache_root.join("audit.db"));
    let audit_store = LocalAuditStore::open(&audit_path)
        .await
        .with_context(|| format!("open audit store at {}", audit_path.display()))?;
    let signer = SessionSigner::generate();
    let run_id = Uuid::new_v4();
    audit_store
        .register_run(run_id, signer.verify_key_bytes())
        .await
        .context("register run")?;

    // The capture-via flag is the same for both single-target and --all
    // invocations of `terrashift schema update`; the distinction between
    // operator-driven and auto-update lives at the call boundary, not here.
    let captured_via = CaptureVia::UserCommand;

    let total = targets.len();
    println!("Capturing {total} provider schema(s)...");
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for (namespace, provider, version) in targets {
        // Progress indication: the fetch step shells out to `terraform init`
        // (~30s cold cache for AWS, ~10s warm) and then `terraform providers
        // schema -json`. Without a heartbeat the operator stares at a blank
        // terminal for up to a minute. We print the "starting" line BEFORE
        // the call so the prompt is visible, and the "✓" or "✗" line after.
        print!("  ⏳ {namespace}/{provider}@{version} (this runs `terraform init` — ~30s on a cold cache)... ");
        // Flush so the line shows up before the long-running call.
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        let started = Instant::now();
        match fetcher.fetch(&namespace, &provider, &version).await {
            Ok(schema) => {
                let json = serde_json::to_vec_pretty(&schema)?;
                let mut hasher = Sha256::new();
                hasher.update(&json);
                let sha256 = hex_lower(&hasher.finalize());

                let dir = cache_root.join(&provider).join(&version);
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("create {}", dir.display()))?;
                let schema_path_abs = dir.join("schema.json");
                std::fs::write(&schema_path_abs, &json)
                    .with_context(|| format!("write {}", schema_path_abs.display()))?;
                let size_bytes = json.len() as u64;

                let duration_ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;

                manifest.upsert(RuntimeManifestEntry {
                    provider: provider.clone(),
                    source: format!("{namespace}/{provider}"),
                    version: version.clone(),
                    version_constraint: format!("= {version}"),
                    captured_at: Utc::now(),
                    captured_by: "schema_update_command".to_string(),
                    terraform_version: detect_terraform_version()
                        .unwrap_or_else(|| "unknown".to_string()),
                    sha256: sha256.clone(),
                    size_bytes,
                    schema_path: format!("{provider}/{version}/schema.json"),
                });

                let mut audit = AuditEntry::new(
                    run_id,
                    Actor::User {
                        id: whoami_or_unknown(),
                    },
                    "schema.capture",
                    Outcome::Ok,
                    AuditPayload::SchemaCapture {
                        provider: provider.clone(),
                        source: format!("{namespace}/{provider}"),
                        version_constraint: format!("= {version}"),
                        resolved_version: version.clone(),
                        terraform_version: detect_terraform_version()
                            .unwrap_or_else(|| "unknown".to_string()),
                        captured_via: captured_via.clone(),
                        sha256,
                        duration_ms,
                    },
                );
                audit_store
                    .append(&signer, &mut audit)
                    .await
                    .context("audit append")?;

                println!(
                    "✓ {} resources ({:.1}s)",
                    schema.resources.len(),
                    duration_ms as f64 / 1000.0
                );
                succeeded += 1;
            }
            Err(e) => {
                println!("✗");
                warn!(error = %e, "schema fetch failed");
                eprintln!("    error: {e}");
                failed += 1;
            }
        }
    }

    manifest.save(&manifest_path)?;
    info!(
        cache = %cache_root.display(),
        manifest = %manifest_path.display(),
        succeeded,
        failed,
        "schema update complete"
    );

    println!();
    println!(
        "Done: {succeeded} captured, {failed} failed. Manifest: {}",
        manifest_path.display()
    );
    if failed > 0 {
        Err(anyhow!("{failed} provider(s) failed to capture"))
    } else {
        Ok(())
    }
}

/// Render a 32-byte digest as 64-char lowercase hex without unwrap.
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

/// Best-effort terraform version detection. Used as metadata only — failure
/// is non-fatal (we still record the capture).
fn detect_terraform_version() -> Option<String> {
    use std::process::Command;
    let out = Command::new("terraform").arg("version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // First line is "Terraform v1.7.5"
    s.lines()
        .next()
        .and_then(|l| l.strip_prefix("Terraform v"))
        .map(|v| v.trim().to_string())
}

/// Best-effort actor identity. The audit log records a string here; the
/// fallback `"unknown"` is harmless because the chain itself anchors the
/// trust (signed by a key registered against the run).
fn whoami_or_unknown() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string())
}
