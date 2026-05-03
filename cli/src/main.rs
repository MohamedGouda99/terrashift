//! Terrashift CLI entry point.
//!
//! Argument parsing and command dispatch. Business logic lives in `libs/`.
//!
//! Pattern: the architecture reference section 14 (CLI surface),
//! section 16 (TUI integration).
//! Constitution: Article VII (repository hygiene — thin CLI, fat libs).

mod commands;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "terrashift",
    version,
    about = "Cross-cloud Terraform migration",
    long_about = "Terrashift — migrate Terraform-managed infrastructure between cloud providers \
                  (AWS ↔ Azure ↔ GCP). Run `terrashift help <command>` for details on a subcommand."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to the operator profile (default: ~/.terrashift/profile.toml).
    /// See assets/profile.example.toml for the schema.
    #[arg(long, global = true)]
    profile: Option<PathBuf>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Print version + build info.
    Version,

    /// Migrate Terraform from one provider to another.
    Migrate(commands::migrate::Args),

    /// Schema cache operations (list / sync / seed).
    #[command(subcommand)]
    Schemas(commands::schemas::Cmd),

    /// Read .tf files and print the resource inventory (no LLM).
    Scan(commands::scan::Args),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Version) => print_version(),
        Some(Command::Migrate(args)) => commands::migrate::run(args, cli.profile).await?,
        Some(Command::Schemas(cmd)) => commands::schemas::run(cmd).await?,
        Some(Command::Scan(args)) => commands::scan::run(args).await?,
        None => print_welcome(),
    }

    Ok(())
}

fn print_version() {
    println!("terrashift {}", env!("CARGO_PKG_VERSION"));
    println!("rust       {}", env!("CARGO_PKG_RUST_VERSION"));
}

fn print_welcome() {
    let v = env!("CARGO_PKG_VERSION");
    println!("terrashift {v} — cross-cloud Terraform migration\n");
    println!("Common commands:");
    println!("  terrashift migrate --source <dir> --from aws --to azurerm");
    println!("                                Migrate a Terraform tree.");
    println!("  terrashift scan <dir>          Print resource inventory of <dir>.");
    println!("  terrashift schemas sync        Pull provider schemas from registry.");
    println!("    --provider aws --version 5.30.0");
    println!("  terrashift schemas list        Show what's in the local schema cache.");
    println!("  terrashift schemas seed        Smoke-test the bundled seed loader.");
    println!("  terrashift version             Print version.\n");
    println!("Run `terrashift help <subcommand>` for full options.");
    println!("Profile: ~/.terrashift/profile.toml (see assets/profile.example.toml).");
}
