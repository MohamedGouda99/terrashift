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
async fn main() {
    if let Err(e) = run().await {
        // Print the error chain via Display, no Rust backtrace —
        // backtraces belong in a panic, not in a recoverable CLI error.
        eprintln!("error: {e}");
        for cause in e.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
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
        None => launch_tui(cli.profile).await?,
    }

    Ok(())
}

/// No-subcommand path — launch the interactive TUI. Mirrors the
/// "type the binary name, get an interactive session" UX of stakpak,
/// claude, etc.
async fn launch_tui(profile: Option<std::path::PathBuf>) -> Result<()> {
    use terrashift_tui::StatusInfo;

    // Best-effort gather of status footer info. None of these failures
    // should prevent the TUI from launching — operators can fix the
    // profile and re-run /scan / /help inside the TUI.
    let profile_path = commands::util::resolve_profile_path(profile).ok();

    let status = StatusInfo {
        profile_path: profile_path.clone(),
        seed_resources: None,
        tier: "eco".to_string(),
    };

    terrashift_tui::start_tui(status)
        .await
        .map_err(|e| anyhow::anyhow!("TUI failed: {e}"))
}

fn print_version() {
    println!("terrashift {}", env!("CARGO_PKG_VERSION"));
    println!("rust       {}", env!("CARGO_PKG_RUST_VERSION"));
}
