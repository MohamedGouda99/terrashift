// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schema verify` — recompute SHA-256 of every cached schema
//! and compare against the manifest. Used by CI to gate on schema integrity.
//!
//! Exit 0 when every cached `(provider, version)` matches its manifest entry.
//! Exit 1 (via `Err` propagation) on any mismatch. Reports each drift line
//! by line with the field that disagrees.

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use terrashift_knowledge::RuntimeManifest;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Cache root override (default ~/.terrashift/schemas/).
    #[arg(long)]
    pub cache: Option<PathBuf>,
}

pub async fn run(args: Args, _profile: Option<PathBuf>) -> Result<()> {
    let cache_root = super::resolve_cache_root(args.cache)?;
    let manifest_path = super::manifest_path(&cache_root);
    let manifest = RuntimeManifest::load(&manifest_path)
        .with_context(|| format!("load manifest at {}", manifest_path.display()))?;

    if manifest.schemas.is_empty() {
        println!("No schemas to verify (cache empty).");
        return Ok(());
    }

    let mut drift = 0usize;
    let mut missing = 0usize;
    for entry in &manifest.schemas {
        let path = cache_root.join(&entry.schema_path);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                let actual = hex_lower(&hasher.finalize());
                let actual_size = bytes.len() as u64;

                if actual != entry.sha256 {
                    eprintln!(
                        "error: schema sha256 mismatch for {}@{} ({}): expected {}, got {}",
                        entry.provider,
                        entry.version,
                        entry.schema_path,
                        truncate_hex(&entry.sha256),
                        truncate_hex(&actual)
                    );
                    drift += 1;
                } else if actual_size != entry.size_bytes {
                    // SHA-256 matched but recorded size disagrees — possible
                    // record corruption rather than data tampering.
                    eprintln!(
                        "warn: size mismatch for {}@{} (manifest says {} bytes, file is {} bytes)",
                        entry.provider, entry.version, entry.size_bytes, actual_size
                    );
                } else {
                    println!(
                        "  ✓ {}@{}  {}  ({} bytes)",
                        entry.provider,
                        entry.version,
                        truncate_hex(&entry.sha256),
                        actual_size
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "error: cached schema missing on disk: {} (manifest entry orphaned)",
                    path.display()
                );
                missing += 1;
            }
            Err(e) => {
                eprintln!("error: read {}: {e}", path.display());
                drift += 1;
            }
        }
    }

    println!();
    if drift == 0 && missing == 0 {
        println!("Verified {} schemas, all clean.", manifest.schemas.len());
        Ok(())
    } else {
        Err(anyhow!(
            "verification failed: {drift} drift, {missing} missing"
        ))
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

fn truncate_hex(s: &str) -> String {
    if s.len() >= 12 {
        let prefix: String = s.chars().take(12).collect();
        format!("{prefix}…")
    } else {
        s.to_string()
    }
}
