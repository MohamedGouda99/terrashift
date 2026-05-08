// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Executor — orchestration for terraform plan/apply with command-level
//! approval (Article XIII rule 6) and audit emission.
//!
//! Pattern: the architecture reference §29 (Warden sandbox) + §30 (shell command-
//! level approvals). Source: the reference codebase (see ATTRIBUTIONS.md)
//! (the re-exec-in-Docker pattern for S5 close) +
//! the reference codebase (see ATTRIBUTIONS.md) (the gate, lifted as
//! P-09a).
//!
//! ## Stage 1 vs S5 close
//!
//! Stage 1 ships the orchestration shell — it constructs every
//! command, runs it through P-09a's `resolve()`, and would emit
//! audit. The actual subprocess invocation returns
//! `ExecutorError::NotImplementedYet { session: "S5" }` until Docker
//! + cloud creds land.
//!
//! Why ship the shell now: when S5 wires Docker, the only change is
//! `run_command_subprocess` going from "return NotImplementedYet" to
//! "spawn `docker run ... terraform <args>`." The approval gate, the
//! audit emission, the run-dir scaffolding, and the Result-typed API
//! all stay the same.
//!
//! Constitution: Article V (sandboxed apply, command-level approval),
//! Article X (every action traced via tracing spans), Article XIII
//! rule 6 (gate is non-optional in the apply path), Article XIII
//! rule 7 (no disk-bound secret writes — env vars stay in the
//! caller's process; subprocess inherits via OS).

pub mod errors;
pub mod runner;

pub use errors::ExecutorError;
pub use runner::{DockerRunner, LocalRunner, SubprocessOutcome, SubprocessRunner};

use std::path::PathBuf;
use std::sync::Arc;
use terrashift_shell_tool_approvals::{resolve, stage1_policy, Policy, Verdict};
use uuid::Uuid;

/// What an Executor invocation returns. Populated with real subprocess
/// outcomes when a runner is wired (S5 close); when no runner is set
/// (legacy), `apply()` returns `NotImplementedYet` instead.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    /// run_id passed in; echoed for audit correlation.
    pub run_id: Uuid,
    /// The terraform commands that were authorized to run (each
    /// passed P-09a's gate).
    pub authorized_commands: Vec<String>,
    /// Per-command subprocess outcomes (exit code, duration, log path).
    /// Empty when no runner is wired.
    pub subprocess_outcomes: Vec<SubprocessOutcome>,
    /// Working directory used. Default: `/tmp/terrashift-exec-{run_id}/`
    /// (S5 close: configurable via `Executor::with_cwd`).
    pub working_dir: PathBuf,
}

/// Approval mode — controls how Prompt verdicts are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalMode {
    /// Stage 1 default: any `Prompt` verdict refuses the command.
    /// This makes CI runs explicit — no silent fall-through.
    NonInteractive,
    /// Stage 5+: a TUI prompt asks the operator. Until the agent
    /// loop kernel (S9) wires the prompt path, this is identical
    /// to `NonInteractive`.
    Interactive,
}

/// Executor configuration. Constructed once, used per-apply.
pub struct Executor {
    policy: Policy,
    approval_mode: ApprovalMode,
    /// Subprocess runner. When `Some`, `apply()` invokes real terraform;
    /// when `None`, returns `NotImplementedYet` (legacy Stage 1 behavior).
    runner: Option<Arc<dyn SubprocessRunner>>,
    /// Working directory override. Default: `/tmp/terrashift-exec-{run_id}`.
    cwd_override: Option<PathBuf>,
    /// Environment variables to inject into every subprocess. Typically
    /// populated from the cred broker (AWS_*/GOOGLE_*/ARM_* etc.).
    env: Vec<(String, String)>,
}

impl Executor {
    /// Build an Executor with the canonical Stage 1 policy
    /// (`stage1_policy()` from P-09a) and non-interactive mode.
    /// **No runner wired** — `apply()` returns `NotImplementedYet`.
    /// Callers that want real subprocess execution should chain
    /// `.with_runner(...)`.
    pub fn stage1_default() -> Self {
        Self {
            policy: stage1_policy(),
            approval_mode: ApprovalMode::NonInteractive,
            runner: None,
            cwd_override: None,
            env: Vec::new(),
        }
    }

    /// Build with a custom policy (testing, future stages).
    pub fn new(policy: Policy, approval_mode: ApprovalMode) -> Self {
        Self {
            policy,
            approval_mode,
            runner: None,
            cwd_override: None,
            env: Vec::new(),
        }
    }

    /// Wire a subprocess runner. Without this, `apply()` returns
    /// `NotImplementedYet`. Use `Arc::new(LocalRunner)` for
    /// `--no-sandbox` mode or `Arc::new(DockerRunner::new())` for
    /// the default sandboxed posture.
    pub fn with_runner(mut self, runner: Arc<dyn SubprocessRunner>) -> Self {
        self.runner = Some(runner);
        self
    }

    /// Override the working directory. Default: a temp dir per run_id.
    /// `terrashift apply <path>` sets this to `<path>` so terraform
    /// reads/writes the migrated HCL in place.
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd_override = Some(cwd);
        self
    }

    /// Inject environment variables into every subprocess. Typically
    /// the resolved cloud credentials (`AWS_ACCESS_KEY_ID=...`).
    /// Values are passed via `Command::env()` only — never written
    /// to disk (Article XIII rule 7).
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// Run the approval gate on a single command. Returns the
    /// resolved verdict — does NOT spawn a subprocess.
    ///
    /// This is the seam Stage 5+ uses to layer per-command audit
    /// emission, TUI prompts, etc. Stage 1 callers use it via
    /// `apply()` which checks Deny/Prompt and returns Err for both.
    ///
    /// Note: we do NOT re-apply `clamp_failure_closed` here — P-09a's
    /// `resolve()` already applies it on `ParseError` paths. Adding
    /// another clamp would push every `Allow` up to `Prompt` and
    /// break the all-Allow happy path.
    pub fn pre_apply_check(&self, command: &str) -> Verdict {
        resolve(command, &self.policy, Verdict::Prompt)
    }

    /// Run a sequence of terraform commands as part of a migration
    /// apply. Each command goes through `pre_apply_check`; the
    /// Executor short-circuits on the first `Deny` or `Prompt`
    /// (in non-interactive mode).
    ///
    /// Stage 1 returns `ExecutorError::NotImplementedYet` even when
    /// every command is `Allow`'d, because the actual subprocess
    /// invocation requires Docker. Tests verify the gate logic
    /// without needing terraform binary.
    ///
    /// `commands` example for a typical apply:
    /// ```ignore
    /// vec![
    ///     "terraform init",
    ///     "terraform validate",
    ///     "terraform plan -out=plan.tfplan",
    ///     "terraform apply plan.tfplan",  // <- this is gated Deny by default
    /// ]
    /// ```
    pub async fn apply(
        &self,
        run_id: Uuid,
        commands: &[&str],
    ) -> Result<ApplyOutcome, ExecutorError> {
        tracing::debug!(
            run_id = %run_id,
            n_commands = commands.len(),
            has_runner = self.runner.is_some(),
            "Executor::apply start"
        );

        let working_dir = self
            .cwd_override
            .clone()
            .unwrap_or_else(|| working_dir_for_run(run_id));

        let mut authorized: Vec<String> = Vec::with_capacity(commands.len());

        for &cmd in commands {
            let verdict = self.pre_apply_check(cmd);
            match verdict {
                Verdict::Allow => {
                    authorized.push(cmd.to_string());
                }
                Verdict::Prompt => {
                    // Stage 1: both NonInteractive and Interactive branches
                    // return the same Err today. Interactive becomes a real
                    // TUI prompt in Stage 5+ once the agent loop kernel's
                    // prompt path is built. The explicit check on
                    // `approval_mode` documents that future seam.
                    if self.approval_mode == ApprovalMode::NonInteractive {
                        return Err(ExecutorError::PromptRequiredButNonInteractive {
                            command: cmd.to_string(),
                        });
                    }
                    return Err(ExecutorError::PromptRequiredButNonInteractive {
                        command: cmd.to_string(),
                    });
                }
                Verdict::Deny => {
                    return Err(ExecutorError::DeniedByApprovalGate {
                        command: cmd.to_string(),
                        verdict,
                    });
                }
            }
        }

        let runner = match &self.runner {
            Some(r) => r,
            None => {
                return Err(ExecutorError::NotImplementedYet {
                    which: "no_runner_wired_call_with_runner_on_executor",
                    session: "S5",
                });
            }
        };

        let log_dir = working_dir
            .join(".terrashift")
            .join("runs")
            .join(run_id.to_string())
            .join("logs");
        tokio::fs::create_dir_all(&log_dir)
            .await
            .map_err(|e| ExecutorError::Io {
                path: log_dir.clone(),
                source: e,
            })?;

        let mut subprocess_outcomes: Vec<SubprocessOutcome> = Vec::with_capacity(authorized.len());
        for (idx, cmd_string) in authorized.iter().enumerate() {
            let parts: Vec<String> = cmd_string.split_whitespace().map(String::from).collect();
            if parts.is_empty() {
                continue;
            }
            let log_path = log_dir.join(format!("{:02}-{}.log", idx, parts[0]));
            let outcome = runner
                .run(&parts, &self.env, &working_dir, &log_path)
                .await?;
            if outcome.exit_code != 0 && outcome.exit_code != 2 {
                // terraform plan returns 2 for "changes detected" — that's a
                // success signal, not a failure. Other non-zero codes are fatal.
                return Err(ExecutorError::NonZeroExit {
                    program: outcome.program.clone(),
                    exit_code: outcome.exit_code,
                });
            }
            subprocess_outcomes.push(outcome);
        }

        Ok(ApplyOutcome {
            run_id,
            authorized_commands: authorized,
            subprocess_outcomes,
            working_dir,
        })
    }

    /// Read-only accessor for the policy; used by tests + future
    /// audit emission paths to log which rules fired.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::stage1_default()
    }
}

/// Compute the canonical working directory for a run. Used by tests.
pub fn working_dir_for_run(run_id: Uuid) -> PathBuf {
    std::env::temp_dir().join(format!("terrashift-exec-{}", run_id))
}
