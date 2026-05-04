// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift scan <dir>` — read .tf files, print resource inventory.
//! Read-only. No LLM, no network. Useful as a sanity check before
//! running `terrashift migrate`.

use anyhow::{Context, Result};
use std::path::PathBuf;
use terrashift_engine::scanner::Scanner;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Directory to scan recursively for `.tf` files.
    pub source: PathBuf,
}

pub async fn run(args: Args) -> Result<()> {
    println!("📂 Scanning {} ...", args.source.display());
    let inv =
        Scanner::scan(&args.source).with_context(|| format!("scan {}", args.source.display()))?;

    let total_files = inv.files.len();
    let total_resources: usize = inv.files.iter().map(|f| f.resources.len()).sum();
    println!("✓ {total_files} file(s); {total_resources} resource(s)\n");

    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for f in &inv.files {
        for r in &f.resources {
            *counts.entry(r.resource_type.as_str()).or_insert(0) += 1;
        }
    }

    println!("Resource types:");
    for (rtype, count) in &counts {
        println!("  {rtype}: {count}");
    }

    Ok(())
}
