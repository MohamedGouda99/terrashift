// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Executor integration tests — Article XIII rule 6 enforcement at
//! the orchestration boundary.
//!
//! All offline. The actual terraform subprocess wires up in S5 close;
//! Stage 1 verifies that the approval gate fires correctly BEFORE
//! any subprocess could run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

use terrashift_engine::executor::{ApprovalMode, Executor, ExecutorError};
use terrashift_shell_tool_approvals::{stage1_policy, Policy, Verdict};

// ─────────────────────────────────────────────────────────────────────
// US1 — Article XIII rule 6: terraform apply is denied at the gate
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us1_terraform_apply_is_denied_before_subprocess() {
    let exec = Executor::stage1_default();
    let run_id = Uuid::new_v4();
    let result = exec
        .apply(run_id, &["terraform apply mainplan.tfplan"])
        .await;

    match result {
        Err(ExecutorError::DeniedByApprovalGate { command, verdict }) => {
            assert_eq!(command, "terraform apply mainplan.tfplan");
            assert_eq!(verdict, Verdict::Deny);
        }
        other => panic!(
            "expected DeniedByApprovalGate for `terraform apply`; got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn us1_terraform_destroy_is_denied() {
    let exec = Executor::stage1_default();
    let run_id = Uuid::new_v4();
    let result = exec.apply(run_id, &["terraform destroy"]).await;
    assert!(matches!(
        result,
        Err(ExecutorError::DeniedByApprovalGate { .. })
    ));
}

// ─────────────────────────────────────────────────────────────────────
// US2 — Pipeline gate: deny-anywhere-in-list short-circuits
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us2_deny_in_list_stops_before_subsequent_commands() {
    let exec = Executor::stage1_default();
    let run_id = Uuid::new_v4();
    let result = exec
        .apply(
            run_id,
            &[
                "terraform init",                    // Allow
                "terraform validate",                // Allow
                "terraform apply mainplan.tfplan",   // Deny → STOP
                "echo this should never be reached", // would be Allow but never checked
            ],
        )
        .await;

    match result {
        Err(ExecutorError::DeniedByApprovalGate { command, .. }) => {
            // The Deny is on the apply, not the echo.
            assert!(command.starts_with("terraform apply"));
        }
        other => panic!("expected Deny; got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────
// US3 — All-Allow path reaches the NotImplementedYet boundary
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us3_all_allow_path_returns_not_implemented_yet() {
    let exec = Executor::stage1_default();
    let run_id = Uuid::new_v4();
    let result = exec
        .apply(
            run_id,
            &[
                "terraform init",
                "terraform validate",
                "terraform plan -out=plan.tfplan",
            ],
        )
        .await;

    // No Deny, no Prompt — Stage 1 hits the subprocess wall.
    match result {
        Err(ExecutorError::NotImplementedYet { which, session }) => {
            assert_eq!(which, "terraform_subprocess_via_docker");
            assert_eq!(session, "S5");
        }
        other => panic!(
            "expected NotImplementedYet for an all-Allow path; got {:?}",
            other
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────
// US4 — Prompt verdict in non-interactive mode is refused
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us4_prompt_in_non_interactive_is_refused() {
    let exec = Executor::stage1_default();
    let run_id = Uuid::new_v4();
    // `terraform foo` (unknown subcommand) falls back to scope
    // `terraform → Prompt` per stage1_policy.
    let result = exec.apply(run_id, &["terraform foo"]).await;
    match result {
        Err(ExecutorError::PromptRequiredButNonInteractive { command }) => {
            assert!(command.starts_with("terraform foo"));
        }
        other => panic!("expected PromptRequiredButNonInteractive; got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────
// pre_apply_check direct verification
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pre_apply_check_returns_correct_verdict() {
    let exec = Executor::stage1_default();
    assert_eq!(
        exec.pre_apply_check("terraform plan -out=plan.tfplan"),
        Verdict::Allow
    );
    assert_eq!(
        exec.pre_apply_check("terraform apply mainplan.tfplan"),
        Verdict::Deny
    );
    assert_eq!(exec.pre_apply_check("rm -rf /tmp/foo"), Verdict::Deny);
    // Empty input — pre_apply_check uses default (Prompt).
    assert_eq!(exec.pre_apply_check(""), Verdict::Prompt);
}

// ─────────────────────────────────────────────────────────────────────
// Custom policy injection
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn custom_policy_overrides_stage1_default() {
    let mut policy: Policy = stage1_policy();
    // Make `terraform apply` Allow in this custom Executor — used by
    // tests that need to verify the all-Allow path reaches the
    // NotImplementedYet boundary even for apply.
    policy.insert("run_command::terraform::apply".to_string(), Verdict::Allow);
    let exec = Executor::new(policy, ApprovalMode::NonInteractive);
    let run_id = Uuid::new_v4();
    let result = exec.apply(run_id, &["terraform apply foo.tfplan"]).await;
    // Custom policy → all Allow → still hits NotImplementedYet
    // (stage1_default would have Deny'd here).
    assert!(matches!(
        result,
        Err(ExecutorError::NotImplementedYet { .. })
    ));
}

// ─────────────────────────────────────────────────────────────────────
// Working directory shape
// ─────────────────────────────────────────────────────────────────────

#[test]
fn working_dir_includes_run_id() {
    let run_id = Uuid::new_v4();
    let dir = terrashift_engine::executor::working_dir_for_run(run_id);
    let s = dir.to_string_lossy();
    assert!(s.contains(&run_id.to_string()));
    assert!(s.contains("terrashift-exec-"));
}

// ─────────────────────────────────────────────────────────────────────
// Sanity — every Stage 1 terraform invocation has a verdict
// ─────────────────────────────────────────────────────────────────────

#[test]
fn every_stage1_terraform_invocation_has_a_verdict() {
    let exec = Executor::stage1_default();
    let stage1_invocations = [
        ("terraform init", Verdict::Allow),
        ("terraform validate", Verdict::Allow),
        ("terraform fmt", Verdict::Allow),
        ("terraform plan -out=plan.tfplan", Verdict::Allow),
        ("terraform show plan.tfplan", Verdict::Allow),
        ("terraform output", Verdict::Allow),
        ("terraform apply plan.tfplan", Verdict::Deny),
        ("terraform destroy", Verdict::Deny),
    ];
    for (cmd, expected) in stage1_invocations {
        assert_eq!(
            exec.pre_apply_check(cmd),
            expected,
            "stage1_policy verdict for `{cmd}` should be {expected:?}"
        );
    }
}
