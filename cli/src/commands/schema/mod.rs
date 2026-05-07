// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift schema` — five subcommands for managing the cached
//! provider-schema artifacts at `~/.terrashift/schemas/`.
//!
//! | Subcommand | Purpose |
//! |---|---|
//! | `list`   | Show what's cached. Tabular, TTY-aware. |
//! | `update` | Capture or refresh schemas via `terraform providers schema -json`. |
//! | `show`   | Print the resource list inside one cached schema. |
//! | `verify` | SHA-256 every cached schema, compare against the manifest. |
//! | `gc`     | Delete schemas not referenced by any profile pin. |
//!
//! Each subcommand is its own module file. This `mod.rs` owns the `Cmd`
//! enum, the dispatcher, and shared cache-path resolution helpers used
//! by all five.
//!
//! Pattern: terrashift_plan.md §15 (CLI design — mirrors stakpak's shape).
//! RFC schema-source-migration §4.2.
//! Constitution: Article IV (every error is loud + actionable),
//! Article VI (version pinning preserved through every path).

pub mod gc;
pub mod install_hint;
pub mod list;
pub mod show;
pub mod update;
pub mod verify;

use anyhow::{anyhow, Result};
use std::path::PathBuf;

#[derive(clap::Subcommand, Debug)]
pub enum Cmd {
    /// Show cached schemas: provider · version · captured · size · sha256 prefix.
    List(list::Args),

    /// Capture or refresh schemas. Requires `terraform >= 1.0` on PATH.
    Update(update::Args),

    /// Print a human-readable summary of one cached schema.
    Show(show::Args),

    /// SHA-256 every cached schema; report drift. Exit 1 if anything is corrupt.
    Verify(verify::Args),

    /// Delete schemas not referenced by any profile pin.
    Gc(gc::Args),
}

pub async fn run(cmd: Cmd, profile: Option<PathBuf>) -> Result<()> {
    match cmd {
        Cmd::List(args) => list::run(args, profile).await,
        Cmd::Update(args) => update::run(args, profile).await,
        Cmd::Show(args) => show::run(args, profile).await,
        Cmd::Verify(args) => verify::run(args, profile).await,
        Cmd::Gc(args) => gc::run(args, profile).await,
    }
}

/// Resolve the cache root. Priority:
///   1. explicit `--cache <path>` from the subcommand
///   2. `~/.terrashift/schemas/`
///
/// All five subcommands use this — kept in one place so a `--cache` override
/// in any subcommand has identical semantics.
pub fn resolve_cache_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("no $USERPROFILE / $HOME — pass --cache <path>"))?;
    Ok(home.join(".terrashift").join("schemas"))
}

/// Path to the manifest file inside a cache root. Centralised so subcommands
/// don't drift on file naming.
pub fn manifest_path(cache_root: &std::path::Path) -> PathBuf {
    cache_root.join("manifest.json")
}

/// Whether stdout is connected to a TTY. Subcommands read this to decide
/// whether to emit ANSI codes and pagination — pipe-friendly when not a TTY.
pub fn stdout_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
