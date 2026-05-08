// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schema show <provider>@<version> [--filter <prefix>]` —
//! print the resource list inside one cached schema.
//!
//! Output is alphabetical. When stdout is a TTY, the resource list is
//! paginated (~30 rows per screen). When piped, no pagination, no ANSI —
//! `terrashift schema show aws@5.30.0 | grep iam` works as expected.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use terrashift_knowledge::RuntimeSchemaCache;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// `provider@version` spec (e.g. `aws@5.30.0`).
    pub spec: String,

    /// Optional resource-type prefix filter (e.g. `aws_vpc` or `aws_iam`).
    /// Case-sensitive prefix match.
    #[arg(long)]
    pub filter: Option<String>,

    /// Cache root override (default ~/.terrashift/schemas/).
    #[arg(long)]
    pub cache: Option<PathBuf>,
}

pub async fn run(args: Args, _profile: Option<PathBuf>) -> Result<()> {
    let cache_root = super::resolve_cache_root(args.cache)?;

    let (provider, version) = parse_spec(&args.spec)?;

    let cache = RuntimeSchemaCache::new(&cache_root);
    let schema = cache.load(&provider, &version).await.map_err(|_| {
        anyhow!(
            "{provider}@{version} is not cached.\n\
             \n  to capture it now:    terrashift schema update --provider {provider} --version {version}\
             \n  to see what is cached: terrashift schema list"
        )
    })?;

    let mut resources: Vec<&String> = schema.resources.keys().collect();
    resources.sort();
    let mut data_sources: Vec<&String> = schema.data_sources.keys().collect();
    data_sources.sort();

    let filter = args.filter.as_deref();
    let filtered_resources: Vec<&&String> = resources
        .iter()
        .filter(|r| filter.map(|f| r.starts_with(f)).unwrap_or(true))
        .collect();
    let deprecated_count: usize = schema
        .resources
        .values()
        .flat_map(|r| r.attributes.values())
        .filter(|a| a.deprecated.is_some())
        .count();

    let tty = super::stdout_is_tty();
    if tty {
        println!("{provider}@{version}");
        println!(
            "  resources    : {} ({}{})",
            schema.resources.len(),
            if filter.is_some() {
                format!("{} matched, ", filtered_resources.len())
            } else {
                String::new()
            },
            if let Some(f) = filter {
                format!("filter='{f}'")
            } else {
                "unfiltered".to_string()
            }
        );
        println!("  data sources : {}", schema.data_sources.len());
        println!("  deprecated   : {deprecated_count}");
        println!();
        println!("Resources:");
    }

    for r in &filtered_resources {
        if tty {
            println!("  {r}");
        } else {
            println!("{r}");
        }
    }

    Ok(())
}

fn parse_spec(spec: &str) -> Result<(String, String)> {
    let (provider, version) = spec
        .split_once('@')
        .ok_or_else(|| anyhow!("invalid spec '{spec}' — expected '<provider>@<version>'"))?;
    if provider.is_empty() || version.is_empty() {
        return Err(anyhow!(
            "invalid spec '{spec}' — provider and version both required"
        ));
    }
    Ok((provider.to_string(), version.to_string()))
}
