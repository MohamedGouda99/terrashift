// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `cargo xtask resolve-latest` — rewrite the build-time pins file
//! `libs/knowledge/seed/manifest.toml` so each pin references the
//! highest stable version currently published to the Terraform Registry.
//!
//! ## Reconciliation with Article VI
//!
//! Article VI bans "latest" in production paths — meaning a *running*
//! migration must always cite a concrete version. This command DOES NOT
//! produce a "latest"-typed pin; it resolves "latest" once, at build time,
//! to a concrete semver string, and writes that string into the manifest.
//! The shipped binary therefore always bundles concrete versions; only
//! the *pipeline that produces the binary* ever observes the abstract
//! notion of "latest". Same model as `Cargo.lock` vs `Cargo.toml`.
//!
//! ## Operator override
//!
//! Operators who want a specific version write it into their profile:
//!
//! ```toml
//! [profiles.default.schemas]
//! pinned = [{ provider = "aws", version = "~> 5.30" }]
//! ```
//!
//! `terrashift schema update` resolves the constraint against the
//! registry's published version list, captures into
//! `~/.terrashift/schemas/`, and the Validator reads from THAT cache —
//! never the bundled one. Operator pin always wins at runtime.

use anyhow::{anyhow, Context, Result};
use semver::Version;
use serde::Deserialize;
use std::path::PathBuf;
use terrashift_knowledge::registry_client::TerraformRegistryClient;
use tracing::info;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Override the manifest path. Default `libs/knowledge/seed/manifest.toml`.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Print proposed changes without writing the manifest.
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[derive(Debug, Deserialize)]
struct SeedPinManifest {
    pin: Vec<SeedPin>,
}

#[derive(Debug, Deserialize, Clone)]
struct SeedPin {
    provider: String,
    version: String,
    source: String,
}

pub async fn run(args: Args) -> Result<()> {
    let manifest_path = args
        .manifest
        .unwrap_or_else(|| PathBuf::from("libs/knowledge/seed/manifest.toml"));

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
        println!("No pins to resolve.");
        return Ok(());
    }

    let client = TerraformRegistryClient::new().context("build registry HTTP client")?;
    let mut resolved: Vec<(SeedPin, String)> = Vec::with_capacity(manifest.pin.len());
    let mut any_changed = false;

    info!(
        manifest = %manifest_path.display(),
        pins = manifest.pin.len(),
        "resolving latest stable per pin"
    );

    for pin in &manifest.pin {
        let (namespace, name) = parse_source(&pin.source)?;
        let versions = client
            .list_versions(namespace, name)
            .await
            .with_context(|| format!("list_versions {}/{name}", namespace))?;

        // Filter to stable semver, find max. Prerelease (`-rc1`, `-beta`)
        // is excluded: shipping a prerelease schema bundled into a stable
        // Terrashift release would surprise operators with deprecated
        // attribute changes between RC and GA.
        let highest_stable = versions
            .versions
            .iter()
            .filter_map(|v| Version::parse(&v.version).ok())
            .filter(|v| v.pre.is_empty())
            .max();

        let new_version = match highest_stable {
            Some(v) => v.to_string(),
            None => {
                eprintln!(
                    "  ! {namespace}/{name}: no stable versions found in registry — keeping current pin {}",
                    pin.version
                );
                pin.version.clone()
            }
        };

        if new_version != pin.version {
            println!(
                "  ⬆ {provider}: {old} → {new}",
                provider = pin.provider,
                old = pin.version,
                new = new_version
            );
            any_changed = true;
        } else {
            println!("  ✓ {}: {} (already current)", pin.provider, pin.version);
        }
        resolved.push((pin.clone(), new_version));
    }

    if !any_changed {
        println!("\nAll pins are already at registry latest.");
        return Ok(());
    }

    if args.check {
        println!("\nCheck mode — manifest unchanged. Re-run without --check to apply.");
        return Ok(());
    }

    let new_text = render_manifest(&resolved);
    std::fs::write(&manifest_path, new_text)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    println!(
        "\nWrote {}.\nNext step: `cargo xtask capture-schemas` to refresh the bundled JSONs.",
        manifest_path.display()
    );
    Ok(())
}

fn parse_source(source: &str) -> Result<(&str, &str)> {
    source.split_once('/').ok_or_else(|| {
        anyhow!("invalid source '{source}' — expected '<namespace>/<name>' (e.g. 'hashicorp/aws')")
    })
}

/// Render the manifest with comments preserved as a fixed template.
/// The shape is small (3 pins by convention) and the comments are
/// load-bearing context for human reviewers; using `toml_edit` to
/// preserve free-form formatting would be heavier than this template.
fn render_manifest(resolved: &[(SeedPin, String)]) -> String {
    let mut out = String::new();
    out.push_str("# Build-time provider pins. `cargo xtask capture-schemas` reads this file\n");
    out.push_str("# and produces `libs/knowledge/seed/<provider>/<version>/schema.json` per\n");
    out.push_str("# pin via `terraform providers schema -json`.\n");
    out.push_str("#\n");
    out.push_str("# These versions ship with `cargo build --release`. Operators override\n");
    out.push_str("# them per-profile via `[profiles.<name>.schemas.pinned]` in their\n");
    out.push_str("# `~/.terrashift/profile.toml`.\n");
    out.push_str("#\n");
    out.push_str("# Auto-refreshed weekly by `.github/workflows/weekly-bundle-refresh.yml`\n");
    out.push_str("# (calls `cargo xtask resolve-latest` then opens a PR if anything moved).\n");
    out.push_str("#\n");
    out.push_str("# Pattern: RFC schema-source-migration §4.6.\n");
    out.push_str(
        "# Constitution: Article VI (every pin is a concrete version, never \"latest\");\n",
    );
    out.push_str("#               resolved at build time, frozen in the binary.\n");
    out.push('\n');

    for (pin, new_version) in resolved {
        out.push_str("[[pin]]\n");
        out.push_str(&format!("provider = \"{}\"\n", pin.provider));
        out.push_str(&format!("version  = \"{}\"\n", new_version));
        out.push_str(&format!("source   = \"{}\"\n", pin.source));
        out.push('\n');
    }

    out
}
