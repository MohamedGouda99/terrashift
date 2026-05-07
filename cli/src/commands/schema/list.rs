// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schema list` — show every cached `(provider, version)`
//! tuple with capture metadata.
//!
//! TTY-aware output: when stdout is a terminal, emits aligned columns.
//! When piped, emits tab-separated rows with no padding so awk/grep work.

use anyhow::{Context, Result};
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
        if super::stdout_is_tty() {
            println!("Cache empty: {}", cache_root.display());
            println!(
                "Run `terrashift schema update --provider aws --version \"~> 5.30\"` to populate."
            );
        }
        return Ok(());
    }

    let tty = super::stdout_is_tty();
    if tty {
        println!("Cache: {}\n", cache_root.display());
        println!(
            "{:<10}  {:<10}  {:<20}  {:>10}  sha256",
            "provider", "version", "captured", "size"
        );
        println!("{}", "-".repeat(80));
    }

    let mut entries = manifest.schemas.clone();
    entries.sort_by(|a, b| a.provider.cmp(&b.provider).then(b.version.cmp(&a.version)));

    for e in entries {
        let captured = e.captured_at.format("%Y-%m-%d %H:%M");
        let size = format_size(e.size_bytes);
        let sha_prefix = if e.sha256.len() >= 12 {
            // String-slice on lowercase ASCII hex — the workspace lints
            // forbid `&s[..n]` because of the panic on UTF-8 boundary
            // mid-codepoint. SHA-256 hex is pure ASCII, but the lint
            // doesn't know that, so we use char-based truncation instead.
            e.sha256.chars().take(12).collect::<String>()
        } else {
            e.sha256.clone()
        };

        if tty {
            println!(
                "{:<10}  {:<10}  {:<20}  {:>10}  {}…",
                e.provider, e.version, captured, size, sha_prefix
            );
        } else {
            // Pipe-friendly: tab-separated, no decoration.
            println!(
                "{}\t{}\t{}\t{}\t{}",
                e.provider, e.version, captured, e.size_bytes, e.sha256
            );
        }
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
