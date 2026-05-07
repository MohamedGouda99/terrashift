// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Terrashift workspace task runner.
//!
//! Run via the `cargo xtask` alias defined in `.cargo/config.toml`:
//!
//! ```text
//! cargo xtask capture-schemas         # populate libs/knowledge/seed/{provider}/{version}/
//! cargo xtask capture-eval-schemas    # populate terrashift-evals/.schema-cache/
//! cargo xtask verify-eval-schemas     # SHA-256 audit the eval schema cache
//! cargo xtask refresh-eval-schemas --check  # nightly: surface newer patch versions
//! ```
//!
//! Pattern: terrashift_plan.md §15 (CLI design); RFC schema-source-migration §3 + §5.4.
//! The xtask workflow is a separate compile target so day-to-day developers
//! don't need `terraform` on PATH unless they explicitly run a capture command.

mod capture_eval_schemas;
mod capture_schemas;
mod refresh_eval_schemas;
mod resolve_latest;
mod verify_eval_schemas;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "xtask",
    about = "Terrashift workspace task runner",
    long_about = "Run schema capture, eval cache management, and release prep \
                  steps that aren't part of `cargo build --workspace`."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Populate `libs/knowledge/seed/{provider}/{version}/schema.json` from
    /// the build-time pins in `libs/knowledge/seed/manifest.toml`.
    /// Required ahead of `cargo build --release` so the release binary
    /// can `include_bytes!` bundled schemas.
    CaptureSchemas(capture_schemas::Args),

    /// Populate `terrashift-evals/.schema-cache/<provider>@<version>.json.zst`
    /// from the union of `required_schemas` declared by every fixture's
    /// `manifest.toml`. The output is checked into git.
    CaptureEvalSchemas(capture_eval_schemas::Args),

    /// SHA-256 every cached eval schema, compare against
    /// `terrashift-evals/.schema-cache/manifest.json`. Used by CI
    /// (RFC §5.4 Job A).
    VerifyEvalSchemas(verify_eval_schemas::Args),

    /// Nightly refresh check — runs `terraform providers schema -json` for
    /// each pinned provider, compares with cached. With `--check` only
    /// reports drift; without `--check` writes new schemas. Used by CI
    /// (RFC §5.4 Job C).
    RefreshEvalSchemas(refresh_eval_schemas::Args),

    /// Rewrite `libs/knowledge/seed/manifest.toml` so each pin references
    /// the highest STABLE version currently published in the registry.
    /// Resolves "latest" at build time to a concrete version, preserving
    /// Article VI inside the shipped binary. Used by the weekly bundle
    /// refresh workflow (`.github/workflows/weekly-bundle-refresh.yml`).
    ResolveLatest(resolve_latest::Args),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if let Err(e) = run().await {
        eprintln!("xtask error: {e}");
        for cause in e.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::CaptureSchemas(args) => capture_schemas::run(args).await,
        Command::CaptureEvalSchemas(args) => capture_eval_schemas::run(args).await,
        Command::VerifyEvalSchemas(args) => verify_eval_schemas::run(args).await,
        Command::RefreshEvalSchemas(args) => refresh_eval_schemas::run(args).await,
        Command::ResolveLatest(args) => resolve_latest::run(args).await,
    }
}
