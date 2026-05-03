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

pub use errors::ExecutorError;

use std::path::PathBuf;
use terrashift_shell_tool_approvals::{resolve, stage1_policy, Policy, Verdict};
use uuid::Uuid;

/// What an Executor invocation returns when it would have succeeded.
/// Stage 1 doesn't actually run subprocesses, but the *shape* is
/// stable so S5 close just fills in the fields.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    /// run_id passed in; echoed for audit correlation.
    pub run_id: Uuid,
    /// The terraform commands that were authorized to run (each
    /// passed P-09a's gate). Stage 1: never actually spawned;
    /// returned for inspection in tests.
    pub authorized_commands: Vec<String>,
    /// Working directory the Executor would have used. Stage 1:
    /// `/tmp/terrashift-exec-{run_id}/` per the P-09 prompt
    /// (`terrashift_prompts.md:521`).
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
}

impl Executor {
    /// Build an Executor with the canonical Stage 1 policy
    /// (`stage1_policy()` from P-09a) and non-interactive mode.
    pub fn stage1_default() -> Self {
        Self {
            policy: stage1_policy(),
            approval_mode: ApprovalMode::NonInteractive,
        }
    }

    /// Build with a custom policy (testing, future stages).
    pub fn new(policy: Policy, approval_mode: ApprovalMode) -> Self {
        Self {
            policy,
            approval_mode,
        }
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
            "Executor::apply start"
        );

        let _working_dir = std::env::temp_dir().join(format!("terrashift-exec-{}", run_id));

        // Stage 1 collects the authorized commands so the seam is
        // visible — S5 close fills `ApplyOutcome.authorized_commands`
        // from this and spawns each subprocess. Today the all-Allow
        // path returns `NotImplementedYet` before the vec is used.
        let mut _authorized: Vec<String> = Vec::with_capacity(commands.len());

        for &cmd in commands {
            let verdict = self.pre_apply_check(cmd);
            match verdict {
                Verdict::Allow => {
                    _authorized.push(cmd.to_string());
                }
                Verdict::Prompt => {
                    if self.approval_mode == ApprovalMode::NonInteractive {
                        return Err(ExecutorError::PromptRequiredButNonInteractive {
                            command: cmd.to_string(),
                        });
                    }
                    // Interactive — Stage 5+ would dispatch the TUI prompt
                    // here. Stage 1 short-circuits same as non-interactive
                    // because the prompt path isn't built yet.
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

        // Stage 1: every command authorized; subprocess invocation
        // gated on Docker + cloud creds (S5 close).
        Err(ExecutorError::NotImplementedYet {
            which: "terraform_subprocess_via_docker",
            session: "S5",
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
