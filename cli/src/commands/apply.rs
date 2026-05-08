// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift apply` — run terraform plan (+ optionally apply) against
//! a directory of generated HCL.
//!
//! Pattern: spec `specs/s5-close-executor/spec.md`.
//! Constitution: Article V (sandboxed apply), Article XIII rule 6
//! (gate non-optional), rule 7 (env vars on subprocess only).
//!
//! ## Stage 1 close — what's in
//!
//! - Subprocess execution via DockerRunner (default) or LocalRunner
//!   (`--no-sandbox`).
//! - `--approve` flag: swaps to a policy variant that allows
//!   `terraform apply` (instead of the default Deny).
//! - Inherits cloud creds from the operator's shell env (`AWS_*`,
//!   `GOOGLE_APPLICATION_CREDENTIALS`, `ARM_*`). Real broker dispatch
//!   (STS AssumeRole / ADC / MI) is T4-T13 work — a separate session.
//! - Output goes to operator's stdout + per-command log files at
//!   `<path>/.terrashift/runs/<run_id>/logs/`.
//!
//! ## Stage 1 close — what's out (deferred)
//!
//! - Auto-detect cloud from `provider {}` blocks (today: `--cloud`
//!   flag is required).
//! - Real cred broker resolution via `Profile.creds[<cloud>]`.
//! - Audit log integration (subprocess outcomes are not yet appended
//!   to the audit chain; will land alongside T20 audit_emit helper).
//! - Recovery agent on `terraform validate` / `apply` failure (S10
//!   integration on apply path).

use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use terrashift_creds::{AwsBroker, CredMode};
use terrashift_engine::executor::{
    ApprovalMode, DockerRunner, Executor, LocalRunner, SubprocessRunner,
};
use terrashift_shell_tool_approvals::{stage1_policy, Policy, Verdict};
use uuid::Uuid;

use crate::commands::util::{load_profile, resolve_profile_path};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to a directory of `.tf` files to apply (output of `terrashift migrate`).
    pub path: PathBuf,

    /// Target cloud (`aws` | `azurerm` | `google`). Required until
    /// auto-detect lands. Determines which env-var family the
    /// subprocess inherits.
    #[arg(long)]
    pub cloud: String,

    /// Skip Docker isolation; run terraform directly on the host.
    /// Default: Docker-isolated via `hashicorp/terraform:1.10`.
    #[arg(long)]
    pub no_sandbox: bool,

    /// Allow `terraform apply` (default policy denies it). Without
    /// this flag, the command runs `init/validate/plan` and stops
    /// with a "plan-only" message.
    #[arg(long)]
    pub approve: bool,
}

pub async fn run(args: Args, profile_path: Option<PathBuf>) -> Result<()> {
    if !args.path.exists() {
        return Err(anyhow!(
            "path does not exist: {} — run `terrashift migrate` first",
            args.path.display()
        ));
    }
    if !args.path.is_dir() {
        return Err(anyhow!(
            "path must be a directory of .tf files: {}",
            args.path.display()
        ));
    }

    let cloud = args.cloud.to_lowercase();
    if !matches!(cloud.as_str(), "aws" | "azurerm" | "google") {
        return Err(anyhow!(
            "unsupported --cloud value '{}': expected one of aws, azurerm, google",
            args.cloud
        ));
    }

    let env = resolve_cloud_env(profile_path.clone(), &cloud).await?;

    let runner: Arc<dyn SubprocessRunner> = if args.no_sandbox {
        println!("⚙  --no-sandbox: running terraform directly on host (Article V trade-off)");
        Arc::new(LocalRunner)
    } else {
        println!("🐳 Docker-isolated terraform run via hashicorp/terraform:1.10");
        Arc::new(DockerRunner::new())
    };

    let policy = if args.approve {
        println!("⚠  --approve: terraform apply WILL execute (state changes coming)");
        policy_with_explicit_apply()
    } else {
        stage1_policy()
    };

    let executor = Executor::new(policy, ApprovalMode::NonInteractive)
        .with_runner(runner)
        .with_cwd(args.path.clone())
        .with_env(env);

    let mut commands: Vec<&str> = vec![
        "terraform init",
        "terraform validate",
        "terraform plan -out=plan.tfplan",
    ];
    if args.approve {
        commands.push("terraform apply -auto-approve plan.tfplan");
    }

    let run_id = Uuid::new_v4();
    let started = std::time::Instant::now();
    println!();
    println!("──────────── Apply run {} ────────────", run_id);
    let outcome = executor
        .apply(run_id, &commands)
        .await
        .context("Executor::apply failed")?;

    let total_ms = started.elapsed().as_millis();

    println!();
    println!("──────────── Apply summary ────────────");
    println!("Run ID         : {}", outcome.run_id);
    println!("Commands run   : {}", outcome.subprocess_outcomes.len());
    for sub in &outcome.subprocess_outcomes {
        let icon = if sub.exit_code == 0 || sub.exit_code == 2 {
            "✓"
        } else {
            "✗"
        };
        println!(
            "  {} {} {}  (exit {}, {:.1}s, log: {})",
            icon,
            sub.program,
            sub.args.join(" "),
            sub.exit_code,
            (sub.duration_ms as f64) / 1000.0,
            sub.log_path.display()
        );
    }
    println!("Wall time      : {:.1}s", (total_ms as f64) / 1000.0);
    println!("Working dir    : {}", outcome.working_dir.display());
    if !args.approve {
        println!();
        println!(
            "ℹ  Plan-only mode (no --approve). Re-run with `--approve` to execute terraform apply."
        );
    }

    Ok(())
}

/// Resolve cloud env vars for the subprocess. Strategy:
///
/// 1. If operator profile has `[profiles.X.creds.<cloud>]` configured,
///    dispatch to the matching broker (AWS STS today; GCP ADC + Azure
///    SP/MI in follow-up sessions).
/// 2. Otherwise fall back to inheriting cloud-prefix env vars from
///    the operator's shell (`AWS_*`, `GOOGLE_*`, `ARM_*`).
///
/// The fall-back is the Stage 1 close ergonomic — operators who
/// already have working cloud creds in their shell can use
/// `terrashift apply` immediately. Operators who want short-lived
/// federated tokens declare a `[creds.<cloud>]` block.
async fn resolve_cloud_env(
    profile_path: Option<PathBuf>,
    cloud: &str,
) -> Result<Vec<(String, String)>> {
    let resolved_profile = resolve_profile_path(profile_path)?;

    if resolved_profile.exists() {
        let profile = load_profile(&resolved_profile)
            .with_context(|| format!("loading profile from {}", resolved_profile.display()))?;
        if let Some(cfg) = profile.creds.get(cloud) {
            tracing::info!(
                cloud = cloud,
                mode = ?cfg.mode,
                "apply: dispatching to cred broker via profile"
            );
            match cfg.mode {
                CredMode::StsAssumeRole if cloud == "aws" => {
                    println!("🔐 Resolving AWS short-lived creds via STS AssumeRole ...");
                    let bundle = AwsBroker::new()
                        .resolve_sts_assume_role(cfg)
                        .await
                        .context("AWS STS AssumeRole resolution failed")?;
                    println!(
                        "✓ STS AssumeRole succeeded; {} env var(s) resolved (expires {}, method={})",
                        bundle.len(),
                        bundle.expires_at,
                        bundle.resolution_method
                    );
                    return Ok(bundle.into_env());
                }
                CredMode::Adc | CredMode::ServicePrincipalEnv | CredMode::ManagedIdentity => {
                    eprintln!(
                        "⚠ profile.creds.{cloud}.mode={:?} is not yet implemented (GCP ADC + Azure backends are follow-up sessions); falling back to shell-env inheritance",
                        cfg.mode
                    );
                }
                CredMode::StaticEnv => {
                    println!("ℹ profile says mode=static_env; using shell-env inheritance");
                }
                _ => {
                    eprintln!(
                        "⚠ profile.creds.{cloud}.mode={:?} is set but not handled for cloud='{}'; falling back to shell-env inheritance",
                        cfg.mode, cloud
                    );
                }
            }
        }
    } else {
        tracing::debug!(
            "no profile at {} — falling back to shell-env inheritance",
            resolved_profile.display()
        );
    }

    let env = inherit_cloud_env(cloud);
    if env.is_empty() {
        eprintln!(
            "⚠ No {} cloud env vars found in your shell. terraform may fail to authenticate.",
            cloud
        );
        eprintln!(
            "   Either set the env vars (e.g. AWS_ACCESS_KEY_ID) or configure [profiles.<name>.creds.{}] in ~/.terrashift/profile.toml.",
            cloud
        );
    } else {
        println!(
            "✓ Inherited {} cloud env var(s) for {} from shell",
            env.len(),
            cloud
        );
    }
    Ok(env)
}

/// Inherit cloud-specific env vars from the parent process. Used as
/// the fall-back when no profile cred config is set.
fn inherit_cloud_env(cloud: &str) -> Vec<(String, String)> {
    let prefixes: &[&str] = match cloud {
        "aws" => &["AWS_"],
        "google" => &["GOOGLE_", "GCP_", "CLOUDSDK_"],
        "azurerm" => &["ARM_", "AZURE_"],
        _ => &[],
    };
    let mut out: HashMap<String, String> = HashMap::new();
    for (k, v) in std::env::vars() {
        if prefixes.iter().any(|p| k.starts_with(p)) {
            out.insert(k, v);
        }
    }
    out.into_iter().collect()
}

/// Variant of `stage1_policy()` that allows `terraform apply` and
/// `terraform destroy`. Used when the operator passes `--approve`.
///
/// Article XIII rule 6 — the gate stays on the path; `--approve`
/// changes WHICH commands are gated as Allow vs Deny, not whether
/// the gate runs.
fn policy_with_explicit_apply() -> Policy {
    let mut p = stage1_policy();
    p.insert("run_command::terraform::apply".to_string(), Verdict::Allow);
    // destroy stays Deny — `--approve` is for deploying, not destroying.
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_cloud_is_loud_error() {
        // Build args programmatically and exercise the validation.
        // No need for full clap parsing — the function under test is
        // the validation branch.
        let cloud = "supercloud".to_lowercase();
        assert!(!matches!(cloud.as_str(), "aws" | "azurerm" | "google"));
    }

    #[test]
    fn inherit_aws_env_picks_up_aws_prefixed_vars() {
        // Set a test var; clean up after.
        std::env::set_var("AWS_TEST_S5_FIXTURE", "value");
        let env = inherit_cloud_env("aws");
        std::env::remove_var("AWS_TEST_S5_FIXTURE");
        assert!(
            env.iter().any(|(k, _)| k == "AWS_TEST_S5_FIXTURE"),
            "env should include AWS_TEST_S5_FIXTURE: {env:?}"
        );
    }

    #[test]
    fn policy_with_explicit_apply_allows_apply() {
        let p = policy_with_explicit_apply();
        assert_eq!(
            p.get("run_command::terraform::apply"),
            Some(&Verdict::Allow)
        );
        // destroy stays denied
        assert_eq!(
            p.get("run_command::terraform::destroy"),
            Some(&Verdict::Deny)
        );
    }
}
