//! Terrashift CLI entry point.
//!
//! Argument parsing and command dispatch. Business logic lives in `libs/`.
//!
//! Pattern: the architecture reference section 14 (CLI surface), section 16 (TUI integration).
//! Constitution: Article VII (repository hygiene — thin CLI, fat libs).

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "terrashift",
    version,
    about = "Cross-cloud Terraform migration"
)]
struct Cli {
    /// Subcommand to run. When omitted, opens the TUI.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Print version and build info
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Version) => {
            println!("terrashift {}", env!("CARGO_PKG_VERSION"));
        }
        None => {
            // TUI dispatch lands here in P-14
            println!(
                "terrashift {} — TUI not yet implemented (see P-14)",
                env!("CARGO_PKG_VERSION")
            );
        }
    }

    Ok(())
}
