// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `cargo xtask verify-eval-schemas` — SHA-256 audit of the eval schema
//! cache.
//!
//! Walks `terrashift-evals/.schema-cache/`, recomputes SHA-256 of every
//! captured file, compares against `manifest.json` entries. Exit 1 on any
//! drift. Used by CI (RFC §5.4 Job A).
//!
//! Honours an empty cache as `Ok` — fixtures that don't declare
//! `required_schemas` produce zero cache entries; a cache with zero
//! entries is verifiably consistent.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Override the suite root. Default `terrashift-evals`.
    #[arg(long)]
    pub suite_root: Option<PathBuf>,
}

pub async fn run(args: Args) -> Result<()> {
    let suite_root = args
        .suite_root
        .unwrap_or_else(|| PathBuf::from("terrashift-evals"));
    let cache_dir = suite_root.join(".schema-cache");

    if !cache_dir.is_dir() {
        println!(
            "No eval schema cache at {} — nothing to verify (this is fine \
             when no fixture declares required_schemas).",
            cache_dir.display()
        );
        return Ok(());
    }

    let manifest_path = cache_dir.join("manifest.json");
    if !manifest_path.is_file() {
        println!(
            "No manifest.json at {} — nothing to verify against.",
            manifest_path.display()
        );
        return Ok(());
    }

    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parse {}", manifest_path.display()))?;

    let entries = manifest
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "manifest at {} missing 'entries' array",
                manifest_path.display()
            )
        })?;

    let mut drift = 0usize;
    let mut missing = 0usize;
    for entry in entries {
        let id = entry
            .get("schema_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("manifest entry missing schema_id"))?;
        let expected_sha = entry
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("manifest entry {} missing sha256", id))?;
        let path = cache_dir.join(format!("{id}.json.zst"));
        match std::fs::read(&path) {
            Ok(bytes) => {
                let actual = sha256_hex(&bytes);
                if actual == expected_sha {
                    println!("  ✓ {id}  {}", truncate_hex(expected_sha));
                } else {
                    eprintln!(
                        "error: sha256 drift for {id}: expected {}, got {}",
                        truncate_hex(expected_sha),
                        truncate_hex(&actual)
                    );
                    drift += 1;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("error: cache file missing: {}", path.display());
                missing += 1;
            }
            Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    if drift == 0 && missing == 0 {
        println!("\nVerified {} eval schema(s), all clean.", entries.len());
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "verification failed: {drift} drift, {missing} missing"
        ))
    }
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

fn truncate_hex(s: &str) -> String {
    if s.len() >= 12 {
        let prefix: String = s.chars().take(12).collect();
        format!("{prefix}…")
    } else {
        s.to_string()
    }
}

#[allow(dead_code)]
fn _ensure_path_used(_p: &Path) {}
